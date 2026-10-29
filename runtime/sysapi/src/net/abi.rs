// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C-style bound procedures: arguments by value as C passes them (an
//! `int` in a register, a pointer as the 16-byte fat pointer in a
//! register pair, `ABI.md` §3) and a value returned. Integers arrive
//! as `i64` and are narrowed here, so whatever generated code leaves in
//! a register's upper bits never matters; integer results go back as
//! `i64`, which a caller expecting any narrower integer reads the low
//! bits of.

use mi::{FatPtr, Job};
use sockets::Errno;

/// A bound procedure with by-value arguments that returns a value: the
/// `sysv64` entry the loader resolves the import to, which calls `$f`
/// with the job and the arguments.
macro_rules! bound_c {
    ($name:ident($($a:ident: $t:ty),*) -> $r:ty = $f:path) => {
        #[allow(clippy::too_many_arguments)]
        extern "sysv64" fn $name(
            env: *mut codegen::abi::Env,
            _argc: i64,
            _od: i64,
            $($a: $t),*
        ) -> $r {
            $f(mi::storage::job_of(env), $($a),*)
        }
    };
}
pub(crate) use bound_c;

/// A pointer argument.
#[must_use]
pub fn fat(v: u128) -> FatPtr {
    FatPtr::from_bytes(&v.to_le_bytes())
}

/// A pointer result.
#[must_use]
pub fn raw(p: FatPtr) -> u128 {
    u128::from_le_bytes(p.to_bytes())
}

/// The low 32 bits of an integer argument.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn int(v: i64) -> i32 {
    v as i32
}

/// The low 32 bits, unsigned.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn uint(v: i64) -> u32 {
    v as u32
}

/// The low 16 bits, unsigned.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn ushort(v: i64) -> u16 {
    v as u16
}

/// Fail a call: `errno` set, -1 returned.
pub fn fail(j: &mut Job, e: Errno) -> i64 {
    j.errno.set(e.code());
    -1
}

/// The result of a call that answers a count or fails.
pub fn ret(j: &mut Job, r: Result<i64, Errno>) -> i64 {
    match r {
        Ok(v) => v,
        Err(e) => fail(j, e),
    }
}

/// A mi `errno` number as the `Errno` it names.
#[must_use]
pub fn errno(code: i32) -> Errno {
    Errno::from_code(code).unwrap_or(Errno::EUNKNOWN)
}

/// `len` bytes at a pointer; `EFAULT` for a null or dead pointer or
/// one with fewer bytes behind it.
pub fn mem(j: &Job, p: u128, len: usize) -> Result<&'static mut [u8], Errno> {
    let (at, avail) = j.spaces.extent(fat(p)).map_err(|_| Errno::EFAULT)?;
    if (len as u64) > avail {
        return Err(Errno::EFAULT);
    }
    Ok(mi::storage::view(at, len))
}

/// As [`mem`], `None` for a null pointer.
pub fn opt_mem(j: &Job, p: u128, len: usize) -> Result<Option<&'static mut [u8]>, Errno> {
    if fat(p).is_null() {
        return Ok(None);
    }
    mem(j, p, len).map(Some)
}

/// Whether a pointer argument is null.
#[must_use]
pub fn is_null(p: u128) -> bool {
    fat(p).is_null()
}

/// A NUL-terminated EBCDIC string at a pointer, as UTF-8; at most `max`
/// bytes are read.
pub fn cstr(j: &Job, p: u128, max: usize) -> Result<String, Errno> {
    let (at, avail) = j.spaces.extent(fat(p)).map_err(|_| Errno::EFAULT)?;
    let n = usize::try_from(avail).unwrap_or(usize::MAX).min(max);
    let b = mi::storage::view(at, n);
    let end = b.iter().position(|&c| c == 0).unwrap_or(n);
    Ok(ccsid::from_ebcdic(&b[..end]))
}

/// As [`cstr`], `None` for a null pointer.
pub fn opt_cstr(j: &Job, p: u128, max: usize) -> Result<Option<String>, Errno> {
    if is_null(p) {
        return Ok(None);
    }
    cstr(j, p, max).map(Some)
}

/// A big-endian `int` behind a pointer.
pub fn get_int(j: &Job, p: u128) -> Result<i32, Errno> {
    let b = mem(j, p, 4)?;
    Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// Store a big-endian `int` behind a pointer.
pub fn put_int(j: &Job, p: u128, v: i32) -> Result<(), Errno> {
    mem(j, p, 4)?.copy_from_slice(&v.to_be_bytes());
    Ok(())
}

/// A pointer stored in job storage.
#[must_use]
pub fn ptr_at(b: &[u8], off: usize) -> u128 {
    b.get(off..off + 16)
        .map_or(0, |s| u128::from_le_bytes(s.try_into().expect("16 bytes")))
}

/// Store a pointer in job storage.
pub fn put_ptr(b: &mut [u8], off: usize, p: FatPtr) {
    if let Some(s) = b.get_mut(off..off + 16) {
        s.copy_from_slice(&p.to_bytes());
    }
}
