// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `alloc`, `realloc` and `dealloc` on the current group's default heap.

use codegen::abi::Env;

use super::core::raise_on;
use super::mem::{bytes, job};
use crate::space::FatPtr;

fn write_ptr(out: i64, p: FatPtr) {
    bytes(out, 16).copy_from_slice(&p.to_bytes());
}

fn read_ptr(addr: i64) -> FatPtr {
    let b: [u8; 16] = bytes(addr, 16).try_into().expect("16 bytes");
    FatPtr::from_bytes(&b)
}

/// `alloc(env, out, n)`.
pub extern "sysv64" fn alloc(env: *mut Env, out: i64, n: i64) {
    let j = job(env);
    match j.alloc(n.cast_unsigned()) {
        Ok(p) => write_ptr(out, p),
        Err(c) => {
            raise_on(j, c);
            write_ptr(out, FatPtr::null());
        }
    }
}

/// `realloc(env, out, ptr, n)`.
pub extern "sysv64" fn realloc(env: *mut Env, out: i64, ptr: i64, n: i64) {
    let j = job(env);
    match j.realloc(read_ptr(ptr), n.cast_unsigned()) {
        Ok(p) => write_ptr(out, p),
        Err(c) => {
            raise_on(j, c);
            write_ptr(out, FatPtr::null());
        }
    }
}

/// `dealloc(env, ptr)`.
pub extern "sysv64" fn dealloc(env: *mut Env, ptr: i64) {
    let j = job(env);
    if let Err(c) = j.dealloc(read_ptr(ptr)) {
        raise_on(j, c);
    }
}
