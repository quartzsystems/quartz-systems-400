// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job-owned `errno` (and `h_errno`): an `int` in storage, big-endian
//! as every integer is, at an address that never moves, so `__errno()`
//! can hand out a pointer to it once and `*__errno()` stays readable
//! for the rest of the job.

use crate::space::{FatPtr, Spaces};

/// The cell.
#[derive(Debug)]
pub struct ErrnoCell {
    cell: Box<[u8; 4]>,
    ptr: Option<FatPtr>,
}

impl Default for ErrnoCell {
    fn default() -> Self {
        Self {
            cell: Box::new([0; 4]),
            ptr: None,
        }
    }
}

impl ErrnoCell {
    /// The value.
    #[must_use]
    pub fn get(&self) -> i32 {
        i32::from_be_bytes(*self.cell)
    }

    /// Set the value.
    pub fn set(&mut self, v: i32) {
        *self.cell = v.to_be_bytes();
    }

    /// A space pointer to the cell, made on first use.
    pub fn pointer(&mut self, spaces: &mut Spaces) -> FatPtr {
        if let Some(p) = self.ptr
            && spaces.deref(p).is_ok()
        {
            return p;
        }
        let p = spaces.add_region(self.cell.as_ptr() as u64, 4);
        self.ptr = Some(p);
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pointer_reaches_the_cell() {
        let mut spaces = Spaces::default();
        let mut e = ErrnoCell::default();
        e.set(3425);
        let p = e.pointer(&mut spaces);
        assert_eq!(e.pointer(&mut spaces), p, "made once");
        let at = spaces.deref(p).unwrap();
        assert_eq!(at, e.cell.as_ptr() as u64);
        assert_eq!(e.get(), 3425);
        assert_eq!(*e.cell, 3425i32.to_be_bytes());
    }
}
