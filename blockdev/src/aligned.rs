// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A 4 KiB-aligned byte buffer. `O_DIRECT` requires the address, the
//! length and the file offset to be block aligned; this is the only
//! allocator in the workspace that guarantees the first two.

#![allow(unsafe_code)]

use std::ops::{Deref, DerefMut};

use crate::BLOCK_SIZE;

/// One block, aligned to its own size. A `Vec` of these is a buffer whose
/// start is block aligned.
#[repr(C, align(4096))]
#[derive(Clone, Copy)]
struct Block([u8; BLOCK_SIZE as usize]);

/// A zeroed buffer of whole blocks, 4 KiB aligned. Derefs to `[u8]`.
#[derive(Clone)]
pub struct AlignedBuf {
    blocks: Vec<Block>,
}

impl AlignedBuf {
    /// A zeroed buffer of `count` blocks. `count` must be at least 1.
    #[must_use]
    pub fn new(count: usize) -> Self {
        assert!(count > 0, "AlignedBuf needs at least one block");
        Self {
            blocks: vec![Block([0; BLOCK_SIZE as usize]); count],
        }
    }

    /// A buffer holding a copy of `bytes`, which must be a whole number
    /// of blocks.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let bs = BLOCK_SIZE as usize;
        assert!(
            !bytes.is_empty() && bytes.len().is_multiple_of(bs),
            "AlignedBuf::from_bytes needs whole blocks"
        );
        let mut buf = Self::new(bytes.len() / bs);
        buf.copy_from_slice(bytes);
        buf
    }

    /// Number of blocks.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Whether `ptr` is block aligned.
    #[must_use]
    pub fn is_aligned(ptr: *const u8) -> bool {
        ptr.addr().is_multiple_of(BLOCK_SIZE as usize)
    }
}

impl Deref for AlignedBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        let len = self.blocks.len() * BLOCK_SIZE as usize;
        // SAFETY: `Block` is `repr(C)` around a byte array with no padding,
        // so `blocks.len()` contiguous blocks are exactly `len` initialized
        // bytes, alive for as long as `self`.
        unsafe { std::slice::from_raw_parts(self.blocks.as_ptr().cast::<u8>(), len) }
    }
}

impl DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        let len = self.blocks.len() * BLOCK_SIZE as usize;
        // SAFETY: as in `deref`; `&mut self` makes the view unique, and any
        // byte pattern is a valid `Block`.
        unsafe { std::slice::from_raw_parts_mut(self.blocks.as_mut_ptr().cast::<u8>(), len) }
    }
}

impl std::fmt::Debug for AlignedBuf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AlignedBuf({} blocks)", self.blocks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_and_sized() {
        let mut b = AlignedBuf::new(3);
        assert_eq!(b.len(), 3 * 4096);
        assert!(AlignedBuf::is_aligned(b.as_ptr()));
        assert!(b.iter().all(|&x| x == 0));
        b[4096] = 7;
        assert_eq!(b[4096], 7);
        let c = AlignedBuf::from_bytes(&b);
        assert_eq!(&*c, &*b);
        assert_eq!(c.block_count(), 3);
    }

    #[test]
    #[should_panic(expected = "whole blocks")]
    fn from_bytes_rejects_partial_blocks() {
        let _ = AlignedBuf::from_bytes(&[0u8; 100]);
    }
}
