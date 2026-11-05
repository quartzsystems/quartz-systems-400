// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the CLI needs of the job: the program's storage at the
//! pointers it passed (checked at each use, so storage that went away
//! is never written), and its SQL for connections not in server mode.

use sql::Value;
use sql::embed::{Reply, Request};

use crate::consts::len;
use crate::data::ebcdic;
use crate::model::handles::Ptr;

/// The job behind a call.
pub trait Host {
    /// `n` bytes at `p`, or `None` when fewer are there.
    fn read(&self, p: u128, n: usize) -> Option<Vec<u8>>;
    /// How many bytes there are at `p`.
    fn avail(&self, p: u128) -> usize;
    /// Write `b` at `p`; false when it does not fit.
    fn write(&mut self, p: u128, b: &[u8]) -> bool;
    /// Run a statement in the job; a `CALL` it plans is made.
    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>);
}

/// Store a 4-byte integer, when the pointer is not null.
pub fn put_i32(h: &mut dyn Host, p: Ptr, v: i32) {
    if let Some(p) = p {
        h.write(p, &v.to_be_bytes());
    }
}

/// Store a 2-byte integer, when the pointer is not null.
pub fn put_i16(h: &mut dyn Host, p: Ptr, v: i16) {
    if let Some(p) = p {
        h.write(p, &v.to_be_bytes());
    }
}

/// A 4-byte integer at a pointer.
pub fn get_i32(h: &dyn Host, p: Ptr) -> Option<i32> {
    let b = h.read(p?, 4)?;
    Some(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// A string argument: `n` bytes, or up to the first NUL for `SQL_NTS`
/// (or any negative length); `None` for a null pointer.
pub fn get_str(h: &dyn Host, p: Ptr, n: i32) -> Option<String> {
    let p = p?;
    let b = if n == len::NTS || n < 0 {
        let avail = h.avail(p).min(32_768);
        let b = h.read(p, avail)?;
        let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        b[..end].to_vec()
    } else {
        h.read(p, usize::try_from(n).unwrap_or(0))?
    };
    Some(ccsid::from_ebcdic(&b))
}

/// Write `text` with a NUL into `max` bytes at `p`; the full length
/// is the result. True when it was cut short.
pub fn put_str(h: &mut dyn Host, p: Ptr, max: i32, text: &str) -> (usize, bool) {
    let b = ebcdic(text);
    let n = b.len();
    let Some(p) = p else { return (n, false) };
    let cap = usize::try_from(max).unwrap_or(0);
    if cap == 0 {
        return (n, n > 0);
    }
    let keep = n.min(cap - 1);
    let mut out = b[..keep].to_vec();
    out.push(0);
    h.write(p, &out);
    (n, keep < n)
}
