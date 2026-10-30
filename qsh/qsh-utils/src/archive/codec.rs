// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The formats behind one interface: names for `-x`, recognizing an
//! archive, reading every member and writing a padded archive.

use super::cpio::{self, Cpio};
use super::member::Member;
use super::ustar;

/// An archive format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// POSIX ustar.
    Ustar,
    /// The tar format before ustar.
    Tar,
    /// A cpio format.
    Cpio(Cpio),
}

impl Format {
    /// The format of a `-x` name.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ustar" => Self::Ustar,
            "tar" => Self::Tar,
            "cpio" => Self::Cpio(Cpio::Odc),
            "sv4cpio" => Self::Cpio(Cpio::Newc),
            "sv4crc" => Self::Cpio(Cpio::Crc),
            _ => return None,
        })
    }

    /// The default blocking: 10240 bytes for tar formats, 5120 for cpio.
    pub fn default_block(self) -> usize {
        match self {
            Self::Ustar | Self::Tar => 10240,
            Self::Cpio(_) => 5120,
        }
    }
}

/// Recognize an archive by its first header.
pub fn detect(data: &[u8]) -> Option<Format> {
    if let Some(c) = cpio::magic(data) {
        return Some(Format::Cpio(c));
    }
    if ustar::is_header(data) {
        return Some(if ustar::is_ustar(data) {
            Format::Ustar
        } else {
            Format::Tar
        });
    }
    None
}

/// Why an archive could not be read.
pub const NOT_ARCHIVE: &str = "The input is not an archive in a known format.";

/// Read an archive: its format and members.
pub fn read(data: &[u8]) -> Result<(Format, Vec<Member>), String> {
    if data.iter().all(|&b| b == 0) {
        return Err("The archive is empty.".to_string());
    }
    let f = detect(data).ok_or(NOT_ARCHIVE)?;
    let ms = match f {
        Format::Ustar | Format::Tar => ustar::decode(data)?,
        Format::Cpio(_) => cpio::decode(data)?,
    };
    Ok((f, ms))
}

/// Encode one member; an error says why the format cannot hold it.
pub fn encode(m: &Member, f: Format) -> Result<Vec<u8>, String> {
    match f {
        Format::Ustar => ustar::encode(m, false),
        Format::Tar => ustar::encode(m, true),
        Format::Cpio(c) => Ok(cpio::encode(m, c)),
    }
}

/// Close an archive: the end marker, then zeros to a whole number of
/// blocks.
pub fn finish(mut bytes: Vec<u8>, f: Format, block: usize) -> Vec<u8> {
    match f {
        Format::Ustar | Format::Tar => bytes.extend_from_slice(&[0u8; 1024]),
        Format::Cpio(c) => bytes.extend(cpio::trailer(c)),
    }
    let block = block.max(1);
    bytes.resize(bytes.len().div_ceil(block) * block, 0);
    bytes
}
