// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Change counters. One [`Clock`] per backend hands out values; each
//! change to an object sets its record's `change` to the next one, so a
//! value is never seen twice, not even by an object deleted and made
//! again at the same path. The clock reserves values in blocks of
//! [`BLOCK`] and keeps the top of its reservation in [`XATTR`] on the
//! backend's top: after a restart it starts above it, a crash skipping
//! at most one block. A node without a valid record has no counter
//! (it reads 0) until an attribute change or `RCLLNK` writes one.

use crate::backend::{BFile, Backend};
use crate::errno::{Errno, Result};

use super::codec;

/// The extended attribute on the backend's top holding the highest
/// value reserved, u64 little-endian.
pub const XATTR: &str = "trusted.qs400.change";

/// Values reserved at a time.
pub const BLOCK: u64 = 1024;

/// The file system's counter.
#[derive(Debug, Default)]
pub struct Clock {
    next: u64,
    reserved: u64,
}

impl Clock {
    /// The next value (the first ever is 1).
    pub fn take<B: Backend>(&mut self, b: &B) -> Result<u64> {
        if self.next == 0 || self.next > self.reserved {
            // Above whatever was reserved before, by anyone.
            let base = mark(b)?.max(self.reserved);
            let top = base.checked_add(BLOCK).ok_or(Errno::EOVERFLOW)?;
            b.xattr_set("", XATTR, &top.to_le_bytes())?;
            self.next = self.next.max(base + 1);
            self.reserved = top;
        }
        let v = self.next;
        self.next += 1;
        Ok(v)
    }
}

/// The highest value reserved so far (0: none).
pub fn mark<B: Backend>(b: &B) -> Result<u64> {
    match b.xattr_get("", XATTR)? {
        None => Ok(0),
        Some(v) => v
            .try_into()
            .map(u64::from_le_bytes)
            .map_err(|_| Errno::EDAMAGE),
    }
}

/// Set the stored record of the node at `p` to `value`; a node without
/// a valid record is left alone. Returns whether it was set.
pub fn stamp<B: Backend>(b: &B, p: &str, value: u64) -> Result<bool> {
    let Some(raw) = b.xattr_get(p, super::XATTR)? else {
        return Ok(false);
    };
    let Ok(mut m) = codec::decode(&raw) else {
        return Ok(false);
    };
    m.change = value;
    b.xattr_set(p, super::XATTR, &codec::encode(&m))?;
    Ok(true)
}

/// [`stamp`] through an open file (it may have been renamed or
/// unlinked since it was opened).
pub fn stamp_file<F: BFile>(f: &F, value: u64) -> Result<bool> {
    let Some(raw) = f.xattr_get(super::XATTR)? else {
        return Ok(false);
    };
    let Ok(mut m) = codec::decode(&raw) else {
        return Ok(false);
    };
    m.change = value;
    f.xattr_set(super::XATTR, &codec::encode(&m))?;
    Ok(true)
}

/// The counter of an open file's record (0 without one).
pub fn of_file<F: BFile>(f: &F) -> u64 {
    f.xattr_get(super::XATTR)
        .ok()
        .flatten()
        .and_then(|raw| codec::decode(&raw).ok())
        .map_or(0, |m| m.change)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mem::MemBackend;

    #[test]
    fn values_rise_and_survive_a_restart() {
        let b = MemBackend::new();
        let mut c = Clock::default();
        assert_eq!(c.take(&b).unwrap(), 1);
        assert_eq!(c.take(&b).unwrap(), 2);
        assert_eq!(mark(&b).unwrap(), BLOCK);
        // A new clock (a restart) starts above the reservation.
        let mut d = Clock::default();
        assert_eq!(d.take(&b).unwrap(), BLOCK + 1);
        // The first clock's next block starts above the second's.
        for _ in 3..=BLOCK {
            c.take(&b).unwrap();
        }
        assert_eq!(c.take(&b).unwrap(), 2 * BLOCK + 1);
    }

    #[test]
    fn a_damaged_mark_is_an_error() {
        let b = MemBackend::new();
        b.xattr_set("", XATTR, b"xy").unwrap();
        assert_eq!(Clock::default().take(&b), Err(Errno::EDAMAGE));
    }
}
