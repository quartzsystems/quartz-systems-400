// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Generation numbers: with the inode number, a new object's file ID.
//! One [`GenClock`] per backend hands them out. It reserves values in
//! blocks of [`BLOCK`] and keeps the top of its reservation in [`XATTR`]
//! on the backend's top, as the change counter does (`super::change`),
//! so after a restart (an IPL) it starts above every value handed out
//! before: an inode number xfs reuses never meets its old generation,
//! and a file ID kept by a client across an IPL names the same object
//! or none. Values are never 0 (0 is an object made before generations,
//! or behind the IFS's back); after 4294967295 they start again at 1.

use crate::backend::Backend;
use crate::errno::{Errno, Result};

/// The extended attribute on the backend's top holding the highest
/// generation reserved, u32 little-endian.
pub const XATTR: &str = "trusted.qs400.gen";

/// Values reserved at a time.
pub const BLOCK: u32 = 1024;

/// The file system's generation counter.
#[derive(Debug, Default)]
pub struct GenClock {
    next: u32,
    reserved: u32,
}

impl GenClock {
    /// The next generation.
    ///
    /// # Errors
    ///
    /// The mark could not be read (`EDAMAGE`) or written.
    pub fn take<B: Backend>(&mut self, b: &B) -> Result<u32> {
        if self.next == 0 || self.next > self.reserved {
            let mut base = mark(b)?.max(self.reserved);
            if base > u32::MAX - BLOCK {
                // Round again from the start.
                base = 0;
                self.next = 0;
            }
            let top = base + BLOCK;
            b.xattr_set("", XATTR, &top.to_le_bytes())?;
            self.next = self.next.max(base + 1);
            self.reserved = top;
        }
        let v = self.next;
        self.next += 1;
        Ok(v)
    }

    /// The next generation without the backend (its mark failed): above
    /// what this clock gave, not kept.
    pub fn take_unkept(&mut self) -> u32 {
        let v = self.next.max(1);
        self.next = v.wrapping_add(1).max(1);
        v
    }
}

/// The highest generation reserved so far (0: none).
///
/// # Errors
///
/// `EDAMAGE` for a mark of the wrong size.
pub fn mark<B: Backend>(b: &B) -> Result<u32> {
    match b.xattr_get("", XATTR)? {
        None => Ok(0),
        Some(v) => v
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| Errno::EDAMAGE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::mem::MemBackend;

    #[test]
    fn a_restart_starts_above_every_value_given() {
        let b = MemBackend::new();
        let mut c = GenClock::default();
        assert_eq!(c.take(&b).unwrap(), 1);
        assert_eq!(c.take(&b).unwrap(), 2);
        assert_eq!(mark(&b).unwrap(), BLOCK);
        let mut d = GenClock::default();
        assert_eq!(d.take(&b).unwrap(), BLOCK + 1);
        for _ in 3..=BLOCK {
            c.take(&b).unwrap();
        }
        assert_eq!(c.take(&b).unwrap(), 2 * BLOCK + 1);
    }

    #[test]
    fn values_wrap_past_zero() {
        let b = MemBackend::new();
        b.xattr_set("", XATTR, &(u32::MAX - 10).to_le_bytes())
            .unwrap();
        let mut c = GenClock::default();
        assert_eq!(c.take(&b).unwrap(), 1);
        b.xattr_set("", XATTR, b"x").unwrap();
        assert_eq!(GenClock::default().take(&b), Err(Errno::EDAMAGE));
    }
}
