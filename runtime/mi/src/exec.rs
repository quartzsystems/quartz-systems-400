// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running one module as a program: compile it, bind it on its own,
//! give a job a source that holds it, and call its entry. The
//! conformance programs and the service console use this until the
//! store supplies programs.

use std::sync::Arc;

use codegen::abi::Env;
use ile::{BindInput, BindOptions, CreateData, ModuleObject, PgmKind, ProgramObject};
use qir::Module;

use crate::act::{LoadedProgram, MemSource};
use crate::error::{MiError, Result};
use crate::job::Job;

/// A module bound as a program, with a job to run it in.
pub struct Program {
    job: Box<Job>,
    name: String,
    object: Arc<ProgramObject>,
}

impl Program {
    /// Compile and bind a module on its own. A module with an entry
    /// procedure becomes a `*PGM`, otherwise a `*SRVPGM` exporting
    /// everything; imports nothing resolves are left unresolved and
    /// fail with `MCH3401` when called.
    pub fn new(module: Module) -> Result<Self> {
        let name = module.name.clone();
        let kind = if module.entry().is_some() {
            PgmKind::Pgm
        } else {
            PgmKind::SrvPgm
        };
        let mo = ModuleObject::compile(module, CreateData::default(), 0)?;
        let mut options = BindOptions::new(&name, kind);
        options.allow_unresolved = true;
        let (object, _) = ile::bind(BindInput {
            modules: vec![(mo, "QTEMP".into())],
            bnddirs: Vec::new(),
            lookup: &|_| None,
            options,
        })?;
        let object = Arc::new(object);
        let mut source = MemSource::default();
        source.programs.push(("QTEMP".into(), object.clone()));
        let job = Job::boxed(Box::new(source));
        Ok(Self { job, name, object })
    }

    /// Parse, compile and bind a module from its text form.
    pub fn from_text(text: &str) -> Result<Self> {
        Self::new(qir::parse(text)?)
    }

    /// The bound program.
    #[must_use]
    pub fn object(&self) -> &ProgramObject {
        &self.object
    }

    /// The job.
    pub fn job(&mut self) -> &mut Job {
        &mut self.job
    }

    /// The job, read-only.
    #[must_use]
    pub fn job_ref(&self) -> &Job {
        &self.job
    }

    /// The environment generated code receives.
    pub fn env(&mut self) -> *mut Env {
        self.job.env()
    }

    /// The program's code, loaded.
    pub fn loaded(&mut self) -> Result<Arc<LoadedProgram>> {
        let table = self.job.table.clone();
        let kind = self.object.attr.kind;
        self.job
            .act
            .load_program(&self.name, "QTEMP", kind, &table)
            .map_err(MiError::Cond)
    }

    /// Native address of a procedure's entry.
    pub fn proc_addr(&mut self, name: &str) -> Result<usize> {
        let p = self.loaded()?;
        let bm = &p.object.modules[0];
        let i = bm
            .module
            .code
            .procs
            .iter()
            .position(|x| x.name == name)
            .ok_or_else(|| MiError::NoProc(name.into()))?;
        Ok(p.proc_addrs[0][i])
    }

    /// The bytes of a module-scope static, from the program's
    /// activation; `None` before the program ran.
    #[must_use]
    pub fn static_bytes(&self, var: &str) -> Option<Vec<u8>> {
        let bm = self.object.modules.first()?;
        let i = bm.module.ir.vars.iter().position(|v| v.name == var)?;
        let slot = bm.module.code.statics.module_var(u32::try_from(i).ok()?)?;
        let at = usize::try_from(bm.frame_offset + slot.offset).ok()?;
        let len = usize::try_from(slot.size).ok()?;
        for g in self.job.groups() {
            for a in &g.activations {
                if Arc::ptr_eq(&a.program.object, &self.object) {
                    return a.statics.get(at..at + len).map(<[u8]>::to_vec);
                }
            }
        }
        None
    }

    /// Call the entry procedure with no parameters. A pending exception
    /// that percolated out stays on the job.
    pub fn call_entry(&mut self) -> Result<()> {
        if self.object.entry.is_none() {
            return Err(MiError::NoEntry(self.name.clone()));
        }
        self.job
            .call_program(&self.name, "QTEMP", &[])
            .map_err(MiError::Cond)
    }
}
