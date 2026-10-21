// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running one module: compile it, place its code against the runtime
//! table, give it a static area, a job and an environment, and call
//! its procedures. Program activation proper belongs to `ile`; this is
//! what the conformance programs and the service console use until then.

use codegen::abi::Env;
use codegen::{Loaded, ModuleCode, RelocTarget};
use qir::Module;

use crate::error::{MiError, Result};
use crate::job::Job;
use crate::rtlib;

/// A module ready to run.
pub struct Program {
    module: Module,
    code: ModuleCode,
    loaded: Loaded,
    statics: Vec<u8>,
    job: Box<Job>,
    env: Box<Env>,
}

/// The signature of an entry procedure called with no arguments.
type Entry = extern "sysv64" fn(*mut Env, i64, i64);

impl Program {
    /// Compile and place a module. Imported procedures resolve through
    /// `imports`, by name to a native address.
    pub fn new(module: Module, imports: &[(String, usize)]) -> Result<Self> {
        let code = codegen::compile(&module)?;
        let table = rtlib::table();
        let loaded = Loaded::new(&code, &|t: &RelocTarget| match t {
            RelocTarget::Runtime(f) => Ok(table[f.index() as usize] as u64),
            RelocTarget::StaticFrame => Ok(0),
            RelocTarget::ImportProc(name) => imports
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, a)| *a as u64)
                .ok_or_else(|| {
                    codegen::CodegenError::Load(format!("import {name} is not resolved"))
                }),
            RelocTarget::ImportData(name) => Err(codegen::CodegenError::Load(format!(
                "data import {name} is not resolved"
            ))),
        })?;
        let mut statics = code.static_init.clone();
        statics.resize(statics.len().max(16), 0);
        let mut job = Box::new(Job::new());
        job.spaces
            .register_static(statics.as_ptr() as u64, statics.len() as u64);
        let env = Box::new(Env {
            statics: statics.as_mut_ptr(),
            job: std::ptr::from_mut(&mut job.abi),
            entry: std::ptr::null_mut(),
        });
        Ok(Self {
            module,
            code,
            loaded,
            statics,
            job,
            env,
        })
    }

    /// Parse, compile and place a module from its text form.
    pub fn from_text(text: &str, imports: &[(String, usize)]) -> Result<Self> {
        Self::new(qir::parse(text)?, imports)
    }

    /// The module.
    #[must_use]
    pub fn module(&self) -> &Module {
        &self.module
    }

    /// The generated code and its maps.
    #[must_use]
    pub fn code(&self) -> &ModuleCode {
        &self.code
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
        std::ptr::from_mut(&mut *self.env)
    }

    /// Native address of a procedure's entry.
    pub fn proc_addr(&self, name: &str) -> Result<usize> {
        self.loaded
            .proc_addr(name)
            .ok_or_else(|| MiError::NoProc(name.into()))
    }

    /// The bytes of a module-scope static.
    #[must_use]
    pub fn static_bytes(&self, var: &str) -> Option<&[u8]> {
        let i = self.module.vars.iter().position(|v| v.name == var)?;
        let slot = self.code.statics.module_var(u32::try_from(i).ok()?)?;
        let at = usize::try_from(slot.offset).ok()?;
        let len = usize::try_from(slot.size).ok()?;
        self.statics.get(at..at + len)
    }

    /// Call the entry procedure with no parameters. The pending
    /// exception, if one percolated out, stays on the job.
    pub fn call_entry(&mut self) -> Result<()> {
        let (_, entry) = self
            .module
            .entry()
            .ok_or_else(|| MiError::NoEntry(self.module.name.clone()))?;
        let addr = self.proc_addr(&entry.name.clone())?;
        self.call_void(addr);
        Ok(())
    }

    /// Call a procedure that takes no parameters and returns nothing.
    pub fn call_void(&mut self, addr: usize) {
        let env = self.env();
        // SAFETY: `addr` is the entry of a procedure the loader placed
        // in executable memory it still holds; the code generator gave
        // it the signature `(env, argc, opdesc)` for a procedure without
        // parameters or a return value, and `env` points to this
        // program's live environment, job and static area.
        let f: Entry = unsafe { std::mem::transmute::<usize, Entry>(addr) };
        f(env, 0, 0);
    }
}
