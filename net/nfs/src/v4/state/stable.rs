// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stable records RFC 7530 section 9.6.3 and RFC 8881 section 8.4.3
//! ask for: which clients held state, so that after a restart only they
//! reclaim, and whether the last grace period ended. The job keeps them
//! in a file under `/QIBM/UserData/OS400/NFS/` ([`file_name`]) through
//! [`StableStore`]; [`encode`] and [`decode`] are its format, a format
//! of this release (big-endian):
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | 0 | 4 | `QSN4` |
//! | 4 | 2 | version, 1 |
//! | 6 | 2 | flags: 1 the grace period of the start that wrote it ended |
//! | 8 | 4 | records, n |
//! | 12 | | n records: owner length (2, at most 1024), the client's owner bytes, minor version (1), flags (1: 1 holds state in that start, 2 may still reclaim from the start before) |

use std::io;
use std::sync::{Mutex, PoisonError};

/// The magic.
pub const MAGIC: &[u8; 4] = b"QSN4";

/// The format's version.
pub const VERSION: u16 = 1;

/// The longest owner (`NFS4_OPAQUE_LIMIT`).
pub const MAX_OWNER: usize = 1024;

/// Where job `n` (1 for the first `QNFSNFSD`) keeps its records.
#[must_use]
pub fn file_name(n: u32) -> String {
    format!("/QIBM/UserData/OS400/NFS/QNFSV4CL.{n:03}")
}

/// One client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableClient {
    /// `nfs_client_id4.id` (4.0) or `co_ownerid` (4.1).
    pub owner: Vec<u8>,
    /// Its minor version.
    pub minor: u8,
    /// It holds state in the start that wrote the records.
    pub holds: bool,
    /// It may still reclaim state of the start before.
    pub may_reclaim: bool,
}

/// The records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StableImage {
    /// The writing start's grace period ended.
    pub grace_done: bool,
    /// The clients.
    pub clients: Vec<StableClient>,
}

/// Why records do not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// Not this format.
    #[error("not a record file of this format")]
    Magic,
    /// Another version.
    #[error("format version {0}")]
    Version(u16),
    /// Cut short or too long.
    #[error("the records are damaged")]
    Damaged,
}

/// The records as bytes.
#[must_use]
pub fn encode(img: &StableImage) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&VERSION.to_be_bytes());
    b.extend_from_slice(&u16::from(img.grace_done).to_be_bytes());
    let n = img
        .clients
        .iter()
        .filter(|c| c.owner.len() <= MAX_OWNER)
        .count();
    b.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_be_bytes());
    for c in img.clients.iter().filter(|c| c.owner.len() <= MAX_OWNER) {
        b.extend_from_slice(&u16::try_from(c.owner.len()).unwrap_or(0).to_be_bytes());
        b.extend_from_slice(&c.owner);
        b.push(c.minor);
        b.push(u8::from(c.holds) | (u8::from(c.may_reclaim) << 1));
    }
    b
}

/// Records from bytes.
///
/// # Errors
///
/// [`FormatError`].
pub fn decode(b: &[u8]) -> Result<StableImage, FormatError> {
    if b.get(..4) != Some(&MAGIC[..]) {
        return Err(FormatError::Magic);
    }
    let u16_at = |i: usize| -> Result<u16, FormatError> {
        b.get(i..i + 2)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
            .ok_or(FormatError::Damaged)
    };
    let v = u16_at(4)?;
    if v != VERSION {
        return Err(FormatError::Version(v));
    }
    let flags = u16_at(6)?;
    let n = b
        .get(8..12)
        .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
        .ok_or(FormatError::Damaged)?;
    let mut at = 12;
    let mut clients = Vec::new();
    for _ in 0..n {
        let len = usize::from(u16_at(at)?);
        if len > MAX_OWNER {
            return Err(FormatError::Damaged);
        }
        let owner = b.get(at + 2..at + 2 + len).ok_or(FormatError::Damaged)?;
        let tail = b
            .get(at + 2 + len..at + 4 + len)
            .ok_or(FormatError::Damaged)?;
        clients.push(StableClient {
            owner: owner.to_vec(),
            minor: tail[0],
            holds: tail[1] & 1 != 0,
            may_reclaim: tail[1] & 2 != 0,
        });
        at += 4 + len;
    }
    if at != b.len() {
        return Err(FormatError::Damaged);
    }
    Ok(StableImage {
        grace_done: flags & 1 != 0,
        clients,
    })
}

/// Where the records live.
pub trait StableStore: Send + Sync {
    /// The records written last (none: an empty image).
    ///
    /// # Errors
    ///
    /// The file system's; damaged records are the job's to report, and
    /// count as none.
    fn load(&self) -> io::Result<StableImage>;
    /// Writes the records, replacing the old.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn store(&self, img: &StableImage) -> io::Result<()>;
}

/// Records in memory, kept as their bytes (tests, and a restart within
/// one process).
#[derive(Debug, Default)]
pub struct MemStable {
    bytes: Mutex<Option<Vec<u8>>>,
}

impl MemStable {
    /// None yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The bytes written last.
    #[must_use]
    pub fn bytes(&self) -> Option<Vec<u8>> {
        self.bytes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl StableStore for MemStable {
    fn load(&self) -> io::Result<StableImage> {
        match self.bytes() {
            None => Ok(StableImage::default()),
            Some(b) => decode(&b).map_err(io::Error::other),
        }
    }
    fn store(&self, img: &StableImage) -> io::Result<()> {
        *self.bytes.lock().unwrap_or_else(PoisonError::into_inner) = Some(encode(img));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip_and_damage_is_found() {
        let img = StableImage {
            grace_done: true,
            clients: vec![
                StableClient {
                    owner: b"linux-client".to_vec(),
                    minor: 1,
                    holds: true,
                    may_reclaim: false,
                },
                StableClient {
                    owner: vec![0; 3],
                    minor: 0,
                    holds: false,
                    may_reclaim: true,
                },
            ],
        };
        let b = encode(&img);
        assert_eq!(&b[..4], b"QSN4");
        assert_eq!(decode(&b), Ok(img));
        assert_eq!(decode(&b[..b.len() - 1]), Err(FormatError::Damaged));
        let mut longer = b.clone();
        longer.push(0);
        assert_eq!(decode(&longer), Err(FormatError::Damaged));
        let mut v2 = b.clone();
        v2[5] = 2;
        assert_eq!(decode(&v2), Err(FormatError::Version(2)));
        assert_eq!(decode(b"XXXX"), Err(FormatError::Magic));
    }
}
