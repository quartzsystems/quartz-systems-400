// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File handles (docs/NFS.md section 5): 32 bytes, the same for NFS
//! versions 3 and 4, MOUNT version 1 and NLM.
//!
//! | Bytes | What |
//! |---|---|
//! | 0 | the format, [`FORMAT`] |
//! | 1-4 | the export's ID |
//! | 5-8 | the object's file system |
//! | 9-16 | the object's number |
//! | 17-20 | the object's generation |
//! | 21-28 | the number of the export's root |
//! | 29-31 | a check: the low 24 bits of FNV-1a over bytes 0-28 |
//!
//! Every number is big-endian. A handle carries no secret: what it
//! names is checked on every call (the export, its root, that the
//! object lies under it) and reached as the caller.

use ifs::ops::audit::FileId;

use super::error::VfsError;

/// The handle's length.
pub const LEN: usize = 32;

/// The format byte of this layout.
pub const FORMAT: u8 = 1;

/// What a handle names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileHandle {
    /// The export's ID.
    pub export: u32,
    /// The object.
    pub fid: FileId,
    /// The number of the export's root when the handle was made.
    pub root_ino: u64,
}

fn check(b: &[u8]) -> u32 {
    let h = b.iter().fold(0x811c_9dc5_u32, |h, x| {
        (h ^ u32::from(*x)).wrapping_mul(0x0100_0193)
    });
    h & 0x00ff_ffff
}

impl FileHandle {
    /// The handle's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; LEN] {
        let mut b = [0u8; LEN];
        b[0] = FORMAT;
        b[1..5].copy_from_slice(&self.export.to_be_bytes());
        b[5..21].copy_from_slice(&self.fid.to_bytes());
        b[21..29].copy_from_slice(&self.root_ino.to_be_bytes());
        let c = check(&b[..29]).to_be_bytes();
        b[29..].copy_from_slice(&c[1..]);
        b
    }

    /// A handle from its bytes.
    ///
    /// # Errors
    ///
    /// [`VfsError::BadHandle`] for another length, format or check.
    pub fn from_bytes(b: &[u8]) -> Result<Self, VfsError> {
        if b.len() != LEN || b[0] != FORMAT {
            return Err(VfsError::BadHandle);
        }
        let c = check(&b[..29]).to_be_bytes();
        if b[29..] != c[1..] {
            return Err(VfsError::BadHandle);
        }
        let n32 = |r: std::ops::Range<usize>| {
            u32::from_be_bytes(b[r].try_into().unwrap_or_default())
        };
        let n64 = |r: std::ops::Range<usize>| {
            u64::from_be_bytes(b[r].try_into().unwrap_or_default())
        };
        Ok(Self {
            export: n32(1..5),
            fid: FileId {
                fs: n32(5..9),
                ino: n64(9..17),
                generation: n32(17..21),
            },
            root_ino: n64(21..29),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip_and_are_checked() {
        let h = FileHandle {
            export: 7,
            fid: FileId {
                fs: 1,
                ino: 0x1234_5678_9abc,
                generation: 99,
            },
            root_ino: 42,
        };
        let b = h.to_bytes();
        assert_eq!(b[0], FORMAT);
        assert_eq!(FileHandle::from_bytes(&b), Ok(h));
        for i in 0..LEN {
            let mut bad = b;
            bad[i] ^= 1;
            assert_eq!(FileHandle::from_bytes(&bad), Err(VfsError::BadHandle), "{i}");
        }
        assert_eq!(FileHandle::from_bytes(&b[..31]), Err(VfsError::BadHandle));
    }
}
