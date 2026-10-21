// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Heaps: the default heap of every activation group (`%ALLOC`) and
//! the user heaps `CEECRHP` makes. A block is its own space, so a
//! pointer to freed storage fails with `MCH3402` instead of reaching
//! whatever took its place, and a block's address never moves until
//! `%REALLOC` replaces it.

use crate::space::{FatPtr, Spaces};

/// A heap number within an activation group; 0 is the default heap.
pub type HeapId = u32;

/// One allocation.
pub struct Block {
    /// The storage.
    pub data: Vec<u8>,
    /// The space the block is.
    pub space: u32,
}

/// A heap.
pub struct Heap {
    /// Number.
    pub id: HeapId,
    /// Live blocks.
    pub blocks: Vec<Block>,
}

impl Heap {
    /// An empty heap.
    #[must_use]
    pub fn new(id: HeapId) -> Self {
        Self {
            id,
            blocks: Vec::new(),
        }
    }

    /// Allocate `n` zeroed bytes.
    pub fn alloc(&mut self, n: u64, spaces: &mut Spaces) -> FatPtr {
        let len = usize::try_from(n).expect("fits in memory").max(1);
        let data = vec![0u8; len];
        let p = spaces.add_region(data.as_ptr() as u64, data.len() as u64);
        self.blocks.push(Block {
            data,
            space: p.space,
        });
        p
    }

    /// Resize the block that is `space`, keeping its contents; the old
    /// space dies. `None` when the block is not in this heap.
    pub fn realloc(&mut self, space: u32, n: u64, spaces: &mut Spaces) -> Option<FatPtr> {
        let i = self.blocks.iter().position(|b| b.space == space)?;
        let old = self.blocks.remove(i);
        spaces.kill(old.space);
        let len = usize::try_from(n).expect("fits in memory").max(1);
        let mut data = vec![0u8; len];
        let keep = old.data.len().min(len);
        data[..keep].copy_from_slice(&old.data[..keep]);
        let p = spaces.add_region(data.as_ptr() as u64, data.len() as u64);
        self.blocks.push(Block {
            data,
            space: p.space,
        });
        Some(p)
    }

    /// Free the block that is `space`; `false` when it is not here.
    pub fn free(&mut self, space: u32, spaces: &mut Spaces) -> bool {
        let Some(i) = self.blocks.iter().position(|b| b.space == space) else {
            return false;
        };
        let b = self.blocks.remove(i);
        spaces.kill(b.space);
        true
    }

    /// Bytes allocated.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.blocks.iter().map(|b| b.data.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codegen::Cond;

    #[test]
    fn blocks_are_spaces_that_die_when_freed() {
        let mut sp = Spaces::default();
        let mut h = Heap::new(0);
        let p = h.alloc(16, &mut sp);
        let a = sp.deref(p).unwrap();
        assert_eq!(sp.add(p, 16).unwrap().offset, 16);
        assert_eq!(sp.add(p, 17), Err(Cond::Mch0601));
        let q = h.realloc(p.space, 32, &mut sp).unwrap();
        assert_eq!(sp.deref(p), Err(Cond::Mch3402));
        assert_eq!(sp.add(q, 32).unwrap().offset, 32);
        assert!(h.free(q.space, &mut sp));
        assert!(!h.free(q.space, &mut sp));
        assert_eq!(sp.deref(q), Err(Cond::Mch3402));
        assert_eq!(h.bytes(), 0);
        let _ = a;
    }
}
