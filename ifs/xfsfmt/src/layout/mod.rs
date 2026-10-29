// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The on-disk structures, one builder per kind. Every multi-byte
//! field is big-endian except the CRCs. Field offsets are named
//! constants so the tests can hold them against the XFS format.

pub mod ag;
pub mod btree;
pub mod inode;
pub mod log;
pub mod sb;

/// "No block" in an AG.
pub const NULLAGBLOCK: u32 = u32::MAX;
/// "No inode" in an AG.
pub const NULLAGINO: u32 = u32::MAX;

/// A zeroed buffer written field by field.
pub struct Buf(pub Vec<u8>);

impl Buf {
    /// `len` zero bytes.
    pub fn new(len: usize) -> Self {
        Self(vec![0; len])
    }

    /// One byte.
    pub fn u8(&mut self, off: usize, v: u8) {
        self.0[off] = v;
    }

    /// Big-endian u16.
    pub fn u16(&mut self, off: usize, v: u16) {
        self.0[off..off + 2].copy_from_slice(&v.to_be_bytes());
    }

    /// Big-endian u32.
    pub fn u32(&mut self, off: usize, v: u32) {
        self.0[off..off + 4].copy_from_slice(&v.to_be_bytes());
    }

    /// Big-endian u64.
    pub fn u64(&mut self, off: usize, v: u64) {
        self.0[off..off + 8].copy_from_slice(&v.to_be_bytes());
    }

    /// Raw bytes.
    pub fn bytes(&mut self, off: usize, v: &[u8]) {
        self.0[off..off + v.len()].copy_from_slice(v);
    }

    /// Seal the CRC at `off` and hand the bytes over.
    pub fn sealed(mut self, off: usize) -> Vec<u8> {
        crate::crc::seal(&mut self.0, off);
        self.0
    }
}

/// Read a big-endian u32.
pub fn be32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(b[off..off + 4].try_into().expect("4 bytes"))
}

/// Read a big-endian u64.
pub fn be64(b: &[u8], off: usize) -> u64 {
    u64::from_be_bytes(b[off..off + 8].try_into().expect("8 bytes"))
}

#[cfg(test)]
mod tests;
