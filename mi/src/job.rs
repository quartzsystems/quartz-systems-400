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
use crate::msg::{ClearMode, Message, MsgQueue, QueueRef};
use crate::space::Spaces;
use qir::MsgType;

/// A program implemented in Rust, called like any other: the job and
/// the parameter addresses.
pub type NativeFn = fn(&mut Job, &[u64]);

/// A resolved message queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueTarget {
    Frame(usize),
    Ext,
    SysOpr,
}

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
    /// Mapped user spaces.
    pub usrspcs: crate::usrspc::UserSpaces,
    /// The job's external message queue.
    pub ext_msgq: MsgQueue,
    /// `QSYSOPR`, as this job sees it; `MIGRATE(phase5)`.
    pub sysopr: MsgQueue,
    /// Message descriptions.
    pub msgs: Box<dyn msgf::MsgLookup>,
    next_key: u32,
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
            usrspcs: crate::usrspc::UserSpaces::default(),
            ext_msgq: MsgQueue::default(),
            sysopr: MsgQueue::default(),
            msgs: Box::new(msgf::BuiltinMsgs::new()),
            next_key: 1,
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

    /// Destroy an activation group by number; its static areas and
    /// heap storage die.
    pub fn reclaim_group(&mut self, id: u32) {
        if let Ok(bases) = self.act.reclaim_id(id) {
            for b in bases {
                self.spaces.kill_static(b);
                self.spaces.kill_at(b);
            }
        }
    }

    /// `%ALLOC` from the current group's default heap.
    pub fn alloc(&mut self, n: u64) -> Result<crate::space::FatPtr, Cond> {
        self.alloc_from(0, n)
    }

    /// `CEEGTST`: storage from a heap of the current group.
    pub fn alloc_from(
        &mut self,
        heap: crate::heap::HeapId,
        n: u64,
    ) -> Result<crate::space::FatPtr, Cond> {
        let ag = self.act.current as usize;
        let h = self.act.groups[ag]
            .heaps
            .iter_mut()
            .find(|h| h.id == heap)
            .ok_or(Cond::Mch3601)?;
        Ok(h.alloc(n, &mut self.spaces))
    }

    /// `%REALLOC` and `CEECZST`: the block resized, contents kept.
    pub fn realloc(
        &mut self,
        p: crate::space::FatPtr,
        n: u64,
    ) -> Result<crate::space::FatPtr, Cond> {
        let (space, _) = self.spaces.space_info(p)?;
        for g in &mut self.act.groups {
            for h in &mut g.heaps {
                if let Some(q) = h.realloc(space, n, &mut self.spaces) {
                    return Ok(q);
                }
            }
        }
        Err(Cond::Mch3601)
    }

    /// `DEALLOC` and `CEEFRST`: a block freed.
    pub fn dealloc(&mut self, p: crate::space::FatPtr) -> Result<(), Cond> {
        let (space, _) = self.spaces.space_info(p)?;
        for g in &mut self.act.groups {
            for h in &mut g.heaps {
                if h.free(space, &mut self.spaces) {
                    return Ok(());
                }
            }
        }
        Err(Cond::Mch3601)
    }

    /// `CEECRHP`: a user heap in the current group; its number.
    pub fn create_heap(&mut self) -> crate::heap::HeapId {
        let ag = self.act.current as usize;
        let g = &mut self.act.groups[ag];
        let id = g.heaps.iter().map(|h| h.id).max().map_or(1, |m| m + 1);
        g.heaps.push(crate::heap::Heap::new(id));
        id
    }

    /// `CEEDSHP`: a user heap of the current group discarded with
    /// everything in it; the default heap (0) cannot be.
    pub fn discard_heap(&mut self, heap: crate::heap::HeapId) -> Result<(), Cond> {
        if heap == 0 {
            return Err(Cond::Mch3601);
        }
        let ag = self.act.current as usize;
        let g = &mut self.act.groups[ag];
        let i = g
            .heaps
            .iter()
            .position(|h| h.id == heap)
            .ok_or(Cond::Mch3601)?;
        let h = g.heaps.remove(i);
        for b in &h.blocks {
            self.spaces.kill(b.space);
        }
        Ok(())
    }

    /// `QUSPTRUS`: a space pointer to a user space, mapped on first
    /// use; writes reach the object at [`Job::sync_user_spaces`].
    pub fn map_user_space(&mut self, name: &str, lib: &str) -> Result<crate::space::FatPtr, Cond> {
        self.usrspcs.map(name, lib, &mut self.spaces)
    }

    /// Write every changed user space back to its object.
    pub fn sync_user_spaces(&mut self) {
        self.usrspcs.sync();
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

    /// The queue a reference names, from the current entry: a frame
    /// index, or the external or operator queue.
    fn queue_target(&self, q: QueueRef) -> Result<QueueTarget, Cond> {
        let n = self.act.frames.len();
        let frame = |i: Option<usize>| i.map(QueueTarget::Frame).ok_or(Cond::Mch3601);
        match q {
            QueueRef::Same => frame(n.checked_sub(1)),
            QueueRef::Prv => frame(n.checked_sub(2)),
            QueueRef::Up(k) => frame(n.checked_sub(1 + k as usize)),
            QueueRef::CtlBdy => {
                let mut i = n.checked_sub(1).ok_or(Cond::Mch3601)?;
                loop {
                    if self.act.frames[i].control_boundary {
                        return Ok(QueueTarget::Frame(i));
                    }
                    if i == 0 {
                        return Ok(QueueTarget::Ext);
                    }
                    i -= 1;
                }
            }
            QueueRef::Ext => Ok(QueueTarget::Ext),
            QueueRef::SysOpr => Ok(QueueTarget::SysOpr),
        }
    }

    fn queue_mut(&mut self, t: QueueTarget) -> &mut MsgQueue {
        match t {
            QueueTarget::Frame(i) => &mut self.act.frames[i].msgq,
            QueueTarget::Ext => &mut self.ext_msgq,
            QueueTarget::SysOpr => &mut self.sysopr,
        }
    }

    /// The messages on a queue, for displays.
    pub fn queue(&self, q: QueueRef) -> Option<&MsgQueue> {
        Some(match self.queue_target(q).ok()? {
            QueueTarget::Frame(i) => &self.act.frames[i].msgq,
            QueueTarget::Ext => &self.ext_msgq,
            QueueTarget::SysOpr => &self.sysopr,
        })
    }

    /// A message's text, help and severity from its description.
    pub fn describe_message(
        &mut self,
        msgid: &str,
        msgf: &str,
        lib: &str,
        data: &[u8],
    ) -> (String, String, u16) {
        match self.msgs.describe(msgid, msgf, lib) {
            Some(d) => (d.format(data), d.format_help(data), d.severity),
            None => (
                format!("{msgid} {}", ccsid::decode_trimmed(data)),
                String::new(),
                0,
            ),
        }
    }

    /// Send a program message. An escape message becomes the pending
    /// exception of the entry it was sent to; a message to an entry
    /// that does not exist is `MCH3601` here (`CPF2469` on the
    /// machine).
    pub fn send_message(
        &mut self,
        msgid: &str,
        msgf: &str,
        lib: &str,
        data: Vec<u8>,
        msg_type: MsgType,
        to: QueueRef,
    ) -> Result<u32, Cond> {
        let target = self.queue_target(to)?;
        let (text, help, severity) = self.describe_message(msgid, msgf, lib, &data);
        let key = self.next_key;
        self.next_key += 1;
        let from_depth = self.depth;
        let m = Message {
            key,
            msgid: msgid.into(),
            msgf: msgf.into(),
            msgf_lib: lib.into(),
            msg_type,
            data: data.clone(),
            text,
            help,
            severity,
            from_depth,
            received: false,
            reply: None,
        };
        self.queue_mut(target).push(m);
        if msg_type == MsgType::Escape
            && let QueueTarget::Frame(i) = target
        {
            let depth = u32::try_from(i + 1).expect("few frames");
            let e = Exception::message(msgid, msgf, data, depth);
            self.raised.push(e.clone());
            self.exception = Some(e);
            self.abi.pending = 1;
        }
        Ok(key)
    }

    /// Receive the oldest message of a type, or the keyed one, from a
    /// queue; marked received or removed.
    pub fn receive_message(
        &mut self,
        from: QueueRef,
        ty: Option<MsgType>,
        key: Option<u32>,
        rmv: bool,
    ) -> Option<Message> {
        let t = self.queue_target(from).ok()?;
        self.queue_mut(t).receive(ty, key, rmv)
    }

    /// Remove one message by key, or clear by mode; how many went.
    pub fn remove_messages(
        &mut self,
        from: QueueRef,
        key: Option<u32>,
        clear: ClearMode,
    ) -> Result<usize, Cond> {
        let t = self.queue_target(from)?;
        let q = self.queue_mut(t);
        Ok(match key {
            Some(k) => usize::from(q.remove(k)),
            None => q.clear(clear),
        })
    }

    /// Live activation groups, for `DSPACTGRP`.
    pub fn groups(&self) -> impl Iterator<Item = &ActivationGroup> {
        self.act.groups()
    }
}
