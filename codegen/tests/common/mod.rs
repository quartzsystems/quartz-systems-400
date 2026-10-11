// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A stand-in runtime for running generated code in tests: the call
//! table, an environment, and a resolver for the relocations.

#![allow(dead_code, clippy::cast_possible_truncation)]

use std::sync::Mutex;

use codegen::abi::{Env, Job};
use codegen::{Loaded, ModuleCode, RelocTarget, RtFn};

/// Conditions raised, in order.
pub static RAISED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// Current call depth as `enter`/`leave` see it, and the deepest seen.
pub static DEPTH: Mutex<(i64, i64)> = Mutex::new((0, 0));

extern "sysv64" fn raise(env: *mut Env, cond: u32) {
    // SAFETY: generated code passes the environment the test built.
    unsafe {
        (*(*env).job).pending = u64::from(cond);
    }
    RAISED.lock().unwrap().push(cond);
}

extern "sysv64" fn enter(_env: *mut Env, _code: u64) {
    let mut d = DEPTH.lock().unwrap();
    d.0 += 1;
    d.1 = d.1.max(d.0);
}

extern "sysv64" fn leave(_env: *mut Env) {
    DEPTH.lock().unwrap().0 -= 1;
}

extern "sysv64" fn deref(_env: *mut Env, lo: u64, _hi: u64) -> u64 {
    lo
}

extern "sysv64" fn copy(dst: *mut u8, src: *const u8, len: u64) {
    // SAFETY: generated code passes storage it owns, of at least `len` bytes.
    unsafe { std::ptr::copy_nonoverlapping(src, dst, len as usize) }
}

extern "sysv64" fn mov_char(dst: *mut u8, dlen: u64, src: *const u8, slen: u64) {
    // SAFETY: generated code passes storage it owns, of the lengths given.
    unsafe {
        let d = std::slice::from_raw_parts_mut(dst, dlen as usize);
        let s = std::slice::from_raw_parts(src, slen as usize);
        let n = d.len().min(s.len());
        d[..n].copy_from_slice(&s[..n]);
        d[n..].fill(0x40);
    }
}

/// The runtime call table.
pub fn table() -> Vec<usize> {
    let mut t = vec![0usize; codegen::RT_FNS.len()];
    t[RtFn::Raise.index() as usize] = raise as *const () as usize;
    t[RtFn::Enter.index() as usize] = enter as *const () as usize;
    t[RtFn::Leave.index() as usize] = leave as *const () as usize;
    t[RtFn::Deref.index() as usize] = deref as *const () as usize;
    t[RtFn::Copy.index() as usize] = copy as *const () as usize;
    t[RtFn::MovChar.index() as usize] = mov_char as *const () as usize;
    t
}

/// A loaded module with its static area and job.
pub struct Running {
    pub module: qir::Module,
    pub code: ModuleCode,
    pub loaded: Loaded,
    pub statics: Vec<u8>,
    pub job: Box<Job>,
    pub env: Box<Env>,
}

impl Running {
    /// Compile, load and prepare a module; `imports` resolves imported
    /// procedures by name to native addresses.
    pub fn new(text: &str, imports: &[(&str, usize)]) -> Self {
        let module = qir::parse(text).unwrap_or_else(|e| panic!("{e}"));
        let code = codegen::compile(&module).unwrap_or_else(|e| panic!("{e}"));
        let table = table();
        let loaded = Loaded::new(&code, &|t: &RelocTarget| match t {
            RelocTarget::Runtime(f) => Ok(table[f.index() as usize] as u64),
            RelocTarget::StaticFrame => Ok(0),
            RelocTarget::ImportProc(name) => imports
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, a)| *a as u64)
                .ok_or_else(|| codegen::CodegenError::Load(format!("import {name}"))),
            RelocTarget::ImportData(name) => {
                Err(codegen::CodegenError::Load(format!("data {name}")))
            }
        })
        .unwrap_or_else(|e| panic!("{e}"));
        let mut statics = code.static_init.clone();
        statics.resize(statics.len().max(16), 0);
        let mut job = Box::new(Job::default());
        let env = Box::new(Env {
            statics: statics.as_mut_ptr(),
            job: std::ptr::from_mut(&mut *job),
            entry: std::ptr::null_mut(),
        });
        RAISED.lock().unwrap().clear();
        *DEPTH.lock().unwrap() = (0, 0);
        Self {
            module,
            code,
            loaded,
            statics,
            job,
            env,
        }
    }

    pub fn env(&mut self) -> *mut Env {
        std::ptr::from_mut(&mut *self.env)
    }

    pub fn addr(&self, name: &str) -> usize {
        self.loaded
            .proc_addr(name)
            .unwrap_or_else(|| panic!("no proc {name}"))
    }

    pub fn pending(&self) -> u64 {
        self.job.pending
    }

    pub fn clear_pending(&mut self) {
        self.job.pending = 0;
    }

    /// The bytes of a module-scope static.
    pub fn static_bytes(&self, var: &str) -> &[u8] {
        let i = self
            .module
            .vars
            .iter()
            .position(|v| v.name == var)
            .unwrap_or_else(|| panic!("no static {var}"));
        let slot = self
            .code
            .statics
            .module_var(u32::try_from(i).unwrap())
            .expect("static");
        &self.statics[slot.offset as usize..(slot.offset + slot.size) as usize]
    }
}

pub fn raised() -> Vec<u32> {
    RAISED.lock().unwrap().clone()
}

pub fn max_depth() -> i64 {
    DEPTH.lock().unwrap().1
}
