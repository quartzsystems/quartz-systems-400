// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage views for native programs: a system API receives the
//! addresses of its parameters and reads and writes them through here,
//! the same way the runtime library does.

/// `len` bytes at `addr`, a parameter address generated code passed
/// to a native program.
#[must_use]
pub fn view(addr: u64, len: usize) -> &'static mut [u8] {
    crate::rtlib::mem::bytes(addr.cast_signed(), len)
}

/// The job behind an environment record, for bound procedures the
/// loader resolves `*SYSTEM` imports to.
#[must_use]
pub fn job_of(env: *mut codegen::abi::Env) -> &'static mut crate::job::Job {
    crate::rtlib::mem::job(env)
}
