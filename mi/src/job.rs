// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job stand-in: what generated code and the runtime library share
//! while a program runs: the pending exception, the call stack and
//! activation groups, the spaces pointers address, the console. Phase 5
//! replaces its creation and scheduling; the fields are the contract
//! (`MIGRATE(phase5)`).

use std::collections::{HashMap, VecDeque};

use codegen::Cond;
use codegen::abi::Env;
use ile::{ActGrpAttr, PgmKind};

use crate::act::{ActivationGroup, Activator, AgKind, ObjectSource, call_entry};
use crate::cond::Exception;
use crate::space::Spaces;

/// A program implemented in Rust, called like any other: the job and
/// the parameter addresses.
pub type NativeFn = fn(&mut Job, &[u64]);

/// The job. The `abi` header comes first so the pointer generated code
/// holds is also a pointer to the whole record.
#[repr(C)]
pub struct Job {
    /// The part generated code reads: the pending flag.
    pub abi: codegen::abi::Job,
    /// The exception percolating now, if any.
    pub exception: Option<Exception>,
    /// Exceptions handled by a `monitor`, oldest first.
    pub handled: Vec<Exception>,
    /// Every exception raised, oldest first; the dump reads it.
    pub raised: Vec<Exception>,
    /// Call-stack depth: procedures entered and not yet left.
    pub depth: u32,
    /// The deepest the call stack has been.
    pub max_depth: u32,
    /// Spaces pointers address.
    pub spaces: Spaces,
    /// Lines `dsply` wrote; `MIGRATE(phase5)` to the job's display.
    pub console: Vec<String>,
    /// Replies `dsply` reads, oldest first.
    pub replies: VecDeque<String>,
    /// Programs, activation groups and the call stack.
    pub act: Activator,
    /// Native programs by name, found before any object (`QSYS`).
    pub natives: HashMap<String, NativeFn>,
    /// The runtime call table.
    pub table: Vec<usize>,
    env: Box<Env>,
}

impl Job {
    /// A new job with its default activation group and nothing pending.
    /// Boxed, because the environment points back into it.
    #[must_use]
    pub fn boxed(source: Box<dyn ObjectSource>) -> Box<Self> {
        let mut j = Box::new(Self {
            abi: codegen::abi::Job::default(),
            exception: None,
            handled: Vec::new(),
            raised: Vec::new(),
            depth: 0,
            max_depth: 0,
            spaces: Spaces::default(),
            console: Vec::new(),
            replies: VecDeque::new(),
            act: Activator::new(source),
            natives: HashMap::new(),
            table: crate::rtlib::table(),
            env: Box::new(Env {
                statics: std::ptr::null_mut(),
                job: std::ptr::null_mut(),
                entry: std::ptr::null_mut(),
            }),
        });
        j.env.job = std::ptr::from_mut(&mut j.abi);
        j
    }

    /// The environment generated code receives.
    pub fn env(&mut self) -> *mut Env {
        std::ptr::from_mut(&mut *self.env)
    }

    /// Whether an exception is pending.
    #[must_use]
    pub fn pending(&self) -> bool {
        self.abi.pending != 0
    }

    /// Clear the pending exception, as a handler outside generated code
    /// does.
    pub fn clear(&mut self) -> Option<Exception> {
        self.abi.pending = 0;
        self.exception.take()
    }

    /// Call a program by name with parameter addresses, as a dynamic
    /// call does: a native program first, else the object, activated
    /// in the group its attribute names. A pending exception the
    /// program did not handle is left on the job.
    pub fn call_program(&mut self, name: &str, lib: &str, args: &[u64]) -> Result<(), Cond> {
        if (lib == "QSYS" || lib == "*LIBL")
            && let Some(f) = self.natives.get(name).copied()
        {
            f(self, args);
            return Ok(());
        }
        let table = self.table.clone();
        let p = self.act.load_program(name, lib, PgmKind::Pgm, &table)?;
        let entry = p.entry_addr().ok_or(Cond::Mch3401)?;
        let saved = self.act.current;
        let created = if p.object.attr.actgrp == ActGrpAttr::New {
            Some(self.act.create_group("*NEW", AgKind::New))
        } else {
            None
        };
        let target = created.unwrap_or_else(|| self.act.group_for(&p.object, saved));
        self.act.current = target;
        let env = self.env();
        call_entry(entry, env, args);
        self.act.current = saved;
        if let Some(id) = created {
            self.reclaim_group(id);
        }
        Ok(())
    }

    /// Destroy an activation group by number; its static areas die.
    pub fn reclaim_group(&mut self, id: u32) {
        if let Ok(bases) = self.act.reclaim_id(id) {
            for b in bases {
                self.spaces.kill_static(b);
            }
        }
    }

    /// `RCLACTGRP(name)`.
    pub fn rclactgrp(&mut self, name: &str) -> Result<(), &'static str> {
        let id = self
            .act
            .groups()
            .find(|g| g.kind == AgKind::Named && g.name == name)
            .map(|g| g.id)
            .ok_or("no such activation group")?;
        let bases = self.act.reclaim_id(id)?;
        for b in bases {
            self.spaces.kill_static(b);
        }
        Ok(())
    }

    /// `RCLACTGRP(*ELIGIBLE)`.
    pub fn rclactgrp_eligible(&mut self) {
        for b in self.act.reclaim_eligible() {
            self.spaces.kill_static(b);
        }
    }

    /// Live activation groups, for `DSPACTGRP`.
    pub fn groups(&self) -> impl Iterator<Item = &ActivationGroup> {
        self.act.groups()
    }
}
