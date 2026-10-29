// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading back what a device holds: the primary superblock.

use std::io::{Read, Seek, SeekFrom};

use uuid::Uuid;

use crate::Error;
use crate::crc;
use crate::layout::sb::{self, off};
use crate::layout::{be32, be64};

/// What the primary superblock says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// Filesystem UUID.
    pub uuid: Uuid,
    /// Volume label.
    pub label: String,
    /// Block size.
    pub block_size: u32,
    /// Data blocks.
    pub dblocks: u64,
    /// Allocation groups.
    pub ag_count: u32,
    /// Free blocks as last recorded.
    pub fdblocks: u64,
}

/// The XFS v5 filesystem on `dev`, or `None` when its first sector is
/// not a valid v5 superblock (no magic, a bad CRC, another version).
pub fn probe<D: Read + Seek>(dev: &mut D) -> Result<Option<Info>, Error> {
    let mut b = [0u8; 512];
    dev.seek(SeekFrom::Start(0)).map_err(Error::io("seek"))?;
    dev.read_exact(&mut b)
        .map_err(Error::io("read superblock"))?;
    if be32(&b, off::MAGIC) != sb::MAGIC
        || u16::from_be_bytes([b[off::VERSIONNUM], b[off::VERSIONNUM + 1]]) & 0xf != 5
    {
        return Ok(None);
    }
    let sect = usize::from(u16::from_be_bytes([b[off::SECTSIZE], b[off::SECTSIZE + 1]]));
    let mut full = vec![0u8; sect.max(512)];
    full[..512].copy_from_slice(&b);
    if sect > 512 {
        dev.read_exact(&mut full[512..])
            .map_err(Error::io("read superblock"))?;
    }
    if !crc::verify(&full, off::CRC) {
        return Ok(None);
    }
    let label = &b[off::FNAME..off::FNAME + 12];
    let label =
        String::from_utf8_lossy(label.split(|&c| c == 0).next().unwrap_or(&[])).into_owned();
    Ok(Some(Info {
        uuid: Uuid::from_slice(&b[off::UUID..off::UUID + 16]).expect("16 bytes"),
        label,
        block_size: be32(&b, off::BLOCKSIZE),
        dblocks: be64(&b, off::DBLOCKS),
        ag_count: be32(&b, off::AGCOUNT),
        fdblocks: be64(&b, off::FDBLOCKS),
    }))
}
