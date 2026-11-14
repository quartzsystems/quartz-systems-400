// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The functions, each as `fn(&mut Job, args...)` with the arguments as
//! C passes them (an `int` as `i64`, a pointer as the fat pointer's
//! `u128`); [`crate::abi`] gives each its `sysv64` entry.

pub mod bind;
pub mod control;
pub mod dn;
pub mod entry;
pub mod err;
pub mod exop;
pub mod lber;
pub mod option;
pub mod refused;
pub mod result;
pub mod search;
pub mod session;
pub mod update;
pub mod url;
pub mod xlate;

use std::time::Duration;

use mi::{FatPtr, Job};

use crate::consts::*;
use crate::mem::{self, Codec, Image};
use crate::state::{self, Handle, State};

/// Runs `f` over the job's state.
pub fn with<T>(j: &mut Job, f: impl FnOnce(&mut State, &mut Job) -> T) -> T {
    let st = state::of(j);
    let mut s = st.borrow_mut();
    f(&mut s, j)
}

/// The key of the handle `ld` points to.
pub fn key(s: &State, j: &Job, ld: u128) -> Option<u64> {
    mem::native(j, ld).filter(|k| s.handles.contains_key(k))
}

/// Runs `f` over the handle `ld`; `LDAP_PARAM_ERROR` when it is not one.
pub fn on<T>(j: &mut Job, ld: u128, bad: T, f: impl FnOnce(&mut State, &mut Job, u64) -> T) -> T {
    with(j, |s, j| match key(s, j, ld) {
        Some(k) => f(s, j, k),
        None => {
            s.errno = LDAP_PARAM_ERROR;
            bad
        }
    })
}

/// The handle of a key.
pub fn h(s: &mut State, k: u64) -> &mut Handle {
    s.handles.get_mut(&k).expect("a handle")
}

/// An `int` result.
#[must_use]
pub fn ret(code: i32) -> i64 {
    i64::from(code)
}

/// A pointer result.
#[must_use]
pub fn ptr(p: FatPtr) -> u128 {
    mem::raw(p)
}

/// The null pointer result.
#[must_use]
pub fn null() -> u128 {
    mem::raw(FatPtr::null())
}

/// A string argument in a handle's codec.
pub fn text(s: &State, j: &Job, k: Option<u64>, p: u128) -> Option<String> {
    s.codec_of(k).arg(j, p)
}

/// A string placed for the program in `codec`; its pointer.
pub fn give_str(s: &mut State, j: &mut Job, codec: Codec, t: &str) -> FatPtr {
    let mut im = Image::default();
    im.cstr(&codec.bytes(t));
    s.heap.place(j, &im)
}

/// A NULL-terminated string array placed for the program.
pub fn give_strings(s: &mut State, j: &mut Job, codec: Codec, items: &[String]) -> FatPtr {
    let mut im = Image::default();
    let enc: Vec<Vec<u8>> = items.iter().map(|t| codec.bytes(t)).collect();
    im.strings(&enc);
    s.heap.place(j, &im)
}

/// A `berval` placed for the program.
pub fn give_berval(s: &mut State, j: &mut Job, data: &[u8]) -> FatPtr {
    s.heap.place(j, &mem::berval_image(data))
}

/// A NULL-terminated `berval **` placed for the program: one block for
/// the array and one for each value.
pub fn give_bervals(s: &mut State, j: &mut Job, items: &[Vec<u8>]) -> FatPtr {
    let ptrs: Vec<FatPtr> = items.iter().map(|v| give_berval(s, j, v)).collect();
    give_ptr_array(s, j, &ptrs)
}

/// A NULL-terminated pointer array placed for the program.
pub fn give_ptr_array(s: &mut State, j: &mut Job, ptrs: &[FatPtr]) -> FatPtr {
    let mut im = Image::new((ptrs.len() + 1) * mem::PTR);
    for (i, p) in ptrs.iter().enumerate() {
        im.bytes[i * 16..i * 16 + 16].copy_from_slice(&p.to_bytes());
    }
    s.heap.place(j, &im)
}

/// A NULL-terminated `LDAPControl **` placed for the program.
pub fn give_controls(
    s: &mut State,
    j: &mut Job,
    codec: Codec,
    ctrls: &[ldap::control::Control],
) -> FatPtr {
    let ptrs: Vec<FatPtr> = ctrls
        .iter()
        .map(|c| s.heap.place(j, &mem::control_image(c, codec)))
        .collect();
    give_ptr_array(s, j, &ptrs)
}

/// Frees a block the library gave out (ignored for any other storage).
pub fn free_block(s: &mut State, j: &mut Job, p: u128) {
    if let Some(n) = mem::native(j, p) {
        s.heap.free(j, n);
    }
}

/// Frees a `berval **` and its values.
pub fn free_bervals(s: &mut State, j: &mut Job, p: u128) {
    for v in mem::ptr_array(j, p) {
        free_block(s, j, v);
    }
    free_block(s, j, p);
}

/// Frees an `LDAPControl **` and its controls.
pub fn free_controls(s: &mut State, j: &mut Job, p: u128) {
    free_bervals(s, j, p);
}

/// A `struct timeval *` as a duration; none for a null pointer (wait
/// for ever).
pub fn timeval(j: &Job, p: u128) -> Option<Duration> {
    if mem::is_null(p) {
        return None;
    }
    let b = mem::bytes(j, p, 8)?;
    let sec = i32::from_be_bytes([b[0], b[1], b[2], b[3]]).max(0);
    let usec = i32::from_be_bytes([b[4], b[5], b[6], b[7]]).max(0);
    Some(
        Duration::from_secs(sec.unsigned_abs().into())
            + Duration::from_micros(usec.unsigned_abs().into()),
    )
}
