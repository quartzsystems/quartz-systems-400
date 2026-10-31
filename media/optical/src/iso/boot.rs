// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! El Torito: the boot record descriptor and a catalog holding one EFI
//! no-emulation entry.

use crate::iso::SECTOR;

/// El Torito validation entry id.
const CATALOG_ID: &[u8] = b"QUARTZ SYSTEMS";
/// Platform id for EFI.
pub const PLATFORM_EFI: u8 = 0xEF;

/// The boot entry an image carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootEntry {
    /// Platform of the validation entry (`0xEF` for EFI).
    pub platform: u8,
    /// Whether the default entry is bootable.
    pub bootable: bool,
    /// Emulation type (0: none).
    pub media: u8,
    /// Sector count in 512-byte units, as recorded.
    pub sectors512: u16,
    /// The image's first 2048-byte sector.
    pub lba: u32,
}

/// The boot record volume descriptor pointing at the catalog.
#[must_use]
pub fn record(catalog: u32) -> Vec<u8> {
    let mut br = vec![0u8; SECTOR];
    br[1..6].copy_from_slice(b"CD001");
    br[6] = 1;
    br[7..7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
    br[71..75].copy_from_slice(&catalog.to_le_bytes());
    br
}

fn checksum(v: &[u8]) -> u16 {
    v.as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_le_bytes(*w))
        .fold(0u16, u16::wrapping_add)
}

/// The catalog sector: validation entry plus the default EFI
/// no-emulation entry for an image of `len` bytes at `lba`.
#[must_use]
pub fn catalog(lba: u32, len: u64) -> Vec<u8> {
    let mut v = [0u8; 32];
    v[0] = 0x01;
    v[1] = PLATFORM_EFI;
    v[4..4 + CATALOG_ID.len()].copy_from_slice(CATALOG_ID);
    v[30] = 0x55;
    v[31] = 0xAA;
    let sum = checksum(&v);
    v[28..30].copy_from_slice(&(0u16.wrapping_sub(sum)).to_le_bytes());

    let mut e = [0u8; 32];
    e[0] = 0x88;
    // Past sixteen bits the count is 1: firmware then takes the FAT
    // volume's own size, where a capped count would cut it short.
    let sectors512 = u16::try_from(len.div_ceil(512)).unwrap_or(1);
    e[6..8].copy_from_slice(&sectors512.to_le_bytes());
    e[8..12].copy_from_slice(&lba.to_le_bytes());

    let mut cat = vec![0u8; SECTOR];
    cat[..32].copy_from_slice(&v);
    cat[32..64].copy_from_slice(&e);
    cat
}

/// The catalog sector at the record's pointer, if the descriptor is a
/// boot record.
#[must_use]
pub fn catalog_lba(desc: &[u8]) -> Option<u32> {
    (desc.len() >= 75 && desc[0] == 0 && desc.get(7..30) == Some(b"EL TORITO SPECIFICATION"))
        .then(|| u32::from_le_bytes(desc[71..75].try_into().expect("4")))
}

/// Parse a catalog sector: the default entry, if the validation entry
/// checks out.
#[must_use]
pub fn parse(cat: &[u8]) -> Option<BootEntry> {
    if cat.len() < 64 || cat[0] != 1 || cat[30] != 0x55 || cat[31] != 0xAA {
        return None;
    }
    if checksum(&cat[..32]) != 0 {
        return None;
    }
    let e = &cat[32..64];
    Some(BootEntry {
        platform: cat[1],
        bootable: e[0] == 0x88,
        media: e[1],
        sectors512: u16::from_le_bytes([e[6], e[7]]),
        lba: u32::from_le_bytes(e[8..12].try_into().expect("4")),
    })
}
