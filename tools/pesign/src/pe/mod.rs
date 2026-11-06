// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PE32+ image the kernel's EFI stub makes: where its checksum,
//! certificate table entry and sections are ([`Layout`]), the
//! Authenticode digest ([`digest`]) and the certificate table
//! ([`embed`]).

pub mod digest;
pub mod embed;

use crate::Error;

/// The PE32+ optional header magic.
const PE32_PLUS: u16 = 0x020B;
/// The certificate table is the fifth data directory.
const CERT_DIR: usize = 4;

/// A section's raw data in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    /// `PointerToRawData`.
    pub offset: usize,
    /// `SizeOfRawData`.
    pub size: usize,
}

/// The offsets Authenticode needs.
#[derive(Clone, Debug)]
pub struct Layout {
    /// The `CheckSum` field.
    pub checksum: usize,
    /// The certificate table's data directory entry (8 bytes).
    pub cert_entry: usize,
    /// `SizeOfHeaders`.
    pub headers: usize,
    /// The sections with raw data, by file offset.
    pub sections: Vec<Section>,
    /// The certificate table's offset and size; `(0, 0)` when unsigned.
    pub cert_table: (usize, usize),
}

fn u16_at(b: &[u8], o: usize) -> Result<u16, Error> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(Error::Pe("truncated header"))
}

fn u32_at(b: &[u8], o: usize) -> Result<u32, Error> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(Error::Pe("truncated header"))
}

fn usize_at(b: &[u8], o: usize) -> Result<usize, Error> {
    usize::try_from(u32_at(b, o)?).map_err(|_| Error::Pe("offset out of range"))
}

impl Layout {
    /// Read the headers of `image`.
    pub fn of(image: &[u8]) -> Result<Self, Error> {
        if image.get(..2) != Some(b"MZ") {
            return Err(Error::Pe("no MZ header"));
        }
        let pe = usize_at(image, 0x3C)?;
        if image.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(Error::Pe("no PE signature"));
        }
        let coff = pe + 4;
        let nsections = usize::from(u16_at(image, coff + 2)?);
        let opt_size = usize::from(u16_at(image, coff + 16)?);
        let opt = coff + 20;
        if u16_at(image, opt)? != PE32_PLUS {
            return Err(Error::Pe("not a PE32+ image"));
        }
        let ndirs = usize_at(image, opt + 108)?;
        if ndirs <= CERT_DIR {
            return Err(Error::Pe("no certificate table directory"));
        }
        let cert_entry = opt + 112 + CERT_DIR * 8;
        let cert_table = (usize_at(image, cert_entry)?, usize_at(image, cert_entry + 4)?);
        if cert_table.1 > 0 && cert_table.0.checked_add(cert_table.1) != Some(image.len()) {
            return Err(Error::Pe("the certificate table is not at the end of the file"));
        }
        let headers = usize_at(image, opt + 60)?;
        let mut sections = Vec::new();
        let table = opt + opt_size;
        for i in 0..nsections {
            let s = table + i * 40;
            let size = usize_at(image, s + 16)?;
            let offset = usize_at(image, s + 20)?;
            if size == 0 {
                continue;
            }
            if offset.checked_add(size).is_none_or(|e| e > image.len()) {
                return Err(Error::Pe("a section runs past the end of the file"));
            }
            sections.push(Section { offset, size });
        }
        sections.sort_by_key(|s| s.offset);
        if headers > image.len() || cert_entry + 8 > headers {
            return Err(Error::Pe("bad SizeOfHeaders"));
        }
        Ok(Self { checksum: opt + 64, cert_entry, headers, sections, cert_table })
    }
}
