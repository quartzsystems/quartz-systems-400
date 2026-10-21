// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reaching the storage and the job generated code passes. The two
//! `unsafe` blocks of the runtime library live here.

use codegen::abi::{Env, TypeDesc};
use qir::Type;

use crate::job::Job;

/// The job behind an environment.
pub(crate) fn job<'a>(env: *mut Env) -> &'a mut Job {
    // SAFETY: generated code receives `env` from `Program`, which keeps
    // the `Env` and the `Job` it points to alive for as long as the code
    // can run, and `Job` is `repr(C)` with the ABI header first, so the
    // `job` pointer is a pointer to the whole record. The runtime is
    // single-threaded per job, so no other reference is live.
    unsafe { &mut *(*env).job.cast::<Job>() }
}

/// The environment itself, for the fields the call stack switches.
pub(crate) fn env<'a>(env: *mut Env) -> &'a mut Env {
    // SAFETY: `env` is the job's environment, alive for as long as
    // generated code can run, and the runtime is single-threaded per
    // job, so no other reference to it is live.
    unsafe { &mut *env }
}

/// `len` bytes of storage at a native address.
pub(crate) fn bytes<'a>(addr: i64, len: usize) -> &'a mut [u8] {
    if len == 0 {
        return &mut [];
    }
    // SAFETY: generated code passes the address of storage it owns
    // (a frame slot, the static area or a deref the space table
    // validated) together with the type that fixes its length; the
    // verifier and the code generator keep the two consistent. Callers
    // read every source operand before writing a destination, so two
    // slices over overlapping storage never coexist.
    let p = usize::try_from(addr).expect("a native address") as *mut u8;
    unsafe { std::slice::from_raw_parts_mut(p, len) }
}

/// The type a descriptor word names.
pub(crate) fn ty(desc: i64) -> Type {
    TypeDesc::from_word(desc)
        .decode()
        .expect("generated code passes descriptors it encoded")
}

/// The bytes of a typed operand.
pub(crate) fn typed<'a>(addr: i64, desc: i64) -> (&'a mut [u8], Type) {
    let t = ty(desc);
    let len = usize::try_from(t.size()).expect("fits in memory");
    (bytes(addr, len), t)
}
