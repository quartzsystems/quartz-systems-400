// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A minimal ISO 9660 reader: the root directory of a volume, enough
//! for the installer to pull the UKIs off the installation media.

use std::io::{Read, Seek, SeekFrom};

use crate::Error;

/// Logical sector size.
pub const SECTOR: u64 = 2048;
const SECTOR_USIZE: usize = 2048;
const PVD_SECTOR: u64 = 16;

/// A file in the root directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsoFile {
    /// Identifier without the `;1` version.
    pub name: String,
    /// Byte offset of the data.
    pub offset: u64,
    /// Data length.
    pub len: u64,
}

fn le32(b: &[u8], at: usize) -> u64 {
    u64::from(u32::from_le_bytes(b[at..at + 4].try_into().expect("4")))
}

/// The volume identifier and the root directory files.
pub fn read_root<D: Read + Seek>(dev: &mut D) -> Result<(String, Vec<IsoFile>), Error> {
    let mut pvd = vec![0u8; SECTOR_USIZE];
    dev.seek(SeekFrom::Start(PVD_SECTOR * SECTOR))
        .map_err(Error::io("seek to PVD"))?;
    dev.read_exact(&mut pvd).map_err(Error::io("read PVD"))?;
    if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
        return Err(Error::Gpt {
            what: "ISO 9660",
            detail: "no primary volume descriptor".into(),
        });
    }
    let volume_id = String::from_utf8_lossy(&pvd[40..72]).trim().to_string();
    let root = &pvd[156..190];
    let root_lba = le32(root, 2);
    let root_len = le32(root, 10);
    let mut dir = vec![0u8; usize::try_from(root_len).expect("directory fits")];
    dev.seek(SeekFrom::Start(root_lba * SECTOR))
        .map_err(Error::io("seek to root directory"))?;
    dev.read_exact(&mut dir)
        .map_err(Error::io("read root directory"))?;
    let mut files = Vec::new();
    let mut at = 0;
    while at < dir.len() {
        let len = usize::from(dir[at]);
        if len == 0 {
            // Records never cross a sector: skip to the next one.
            at = (at / SECTOR_USIZE + 1) * SECTOR_USIZE;
            continue;
        }
        let rec = &dir[at..at + len];
        let flags = rec[25];
        let name_len = usize::from(rec[32]);
        let raw = &rec[33..33 + name_len];
        at += len;
        if flags & 2 != 0 || raw == [0] || raw == [1] {
            continue; // directories, `.` and `..`
        }
        let name = String::from_utf8_lossy(raw);
        let name = name.split(';').next().unwrap_or("").to_string();
        files.push(IsoFile {
            name,
            offset: le32(rec, 2) * SECTOR,
            len: le32(rec, 10),
        });
    }
    Ok((volume_id, files))
}

/// Read one root-directory file by name.
pub fn read_file<D: Read + Seek>(dev: &mut D, name: &str) -> Result<Vec<u8>, Error> {
    let (_, files) = read_root(dev)?;
    let f = files
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| Error::Gpt {
            what: "ISO 9660",
            detail: format!("no file {name} on the volume"),
        })?;
    let mut data = vec![0u8; usize::try_from(f.len).expect("file fits")];
    dev.seek(SeekFrom::Start(f.offset))
        .map_err(Error::io("seek to file"))?;
    dev.read_exact(&mut data).map_err(Error::io("read file"))?;
    Ok(data)
}
