// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hybrid image: a protective MBR and a GPT in the ISO's system
//! area, the backup GPT in a tail after the payload. One table of byte
//! regions gives both the 2048-byte ISO sectors and the 512-byte LBAs.
//! Partition 1 (EFI System) is exactly the El Torito boot image;
//! partition 2 (Quartz media) is the ISO 9660 payload before it.

use std::fmt::Write as _;

use crate::crc32::crc32;
use crate::error::{Error, Result};
use crate::iso::SECTOR;
use crate::source::ReadAt;

/// The partition table's block size.
pub const LBA: u64 = 512;
/// GPT entries per array.
pub const ENTRIES: u32 = 128;
const LBA_USIZE: usize = 512;
/// Bytes per GPT entry.
pub const ENTRY_SIZE: u32 = 128;
/// Bytes of an entry array.
pub const ARRAY_BYTES: u64 = (ENTRIES * ENTRY_SIZE) as u64;
/// ISO sectors kept after the payload for the backup GPT.
pub const TAIL_SECTORS: u32 = 9;
/// First LBA a partition may use.
pub const FIRST_USABLE: u64 = 34;
/// The EFI System partition type.
pub const EFI_SYSTEM: [u8; 16] = [
    0xC1, 0x2A, 0x73, 0x28, 0xF8, 0x1F, 0x11, 0xD2, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];
/// The Quartz media partition type, `c8a1e5d2-7f34-4b6e-a2d9-515334304d44`.
pub const QUARTZ_MEDIA: [u8; 16] = [
    0xC8, 0xA1, 0xE5, 0xD2, 0x7F, 0x34, 0x4B, 0x6E, 0xA2, 0xD9, 0x51, 0x53, 0x34, 0x30, 0x4D, 0x44,
];
/// Partition 1's name.
pub const ESP_NAME: &str = "QS400 EFI BOOT";
/// Partition 2's name.
pub const MEDIA_NAME: &str = "QS400 MEDIA";

/// One region of the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// What it holds.
    pub name: &'static str,
    /// Byte offset.
    pub offset: u64,
    /// Byte length.
    pub len: u64,
}

impl Region {
    /// First 512-byte LBA.
    #[must_use]
    pub fn lba(&self) -> u64 {
        self.offset / LBA
    }
    /// Last 512-byte LBA.
    #[must_use]
    pub fn last_lba(&self) -> u64 {
        (self.offset + self.len).div_ceil(LBA) - 1
    }
    /// First 2048-byte ISO sector.
    #[must_use]
    pub fn sector(&self) -> u64 {
        self.offset / SECTOR as u64
    }
    /// One past the last byte.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.offset + self.len
    }
}

/// Region names, in table order.
pub mod names {
    /// LBA 0.
    pub const MBR: &str = "protective MBR";
    /// LBA 1.
    pub const HEADER: &str = "primary GPT header";
    /// LBA 2 to 33.
    pub const ARRAY: &str = "primary GPT entries";
    /// Partition 2.
    pub const PAYLOAD: &str = "ISO 9660 payload";
    /// Partition 1.
    pub const ESP: &str = "EFI boot image";
    /// Before the backup header.
    pub const BACKUP_ARRAY: &str = "backup GPT entries";
    /// The last LBA.
    pub const BACKUP_HEADER: &str = "backup GPT header";
}

/// The region table of one image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridLayout {
    /// Image bytes.
    pub total: u64,
    /// The regions, in the order of [`names`].
    pub regions: [Region; 7],
}

impl HybridLayout {
    /// The layout for an image of `total` bytes (tail included) whose
    /// boot image is `esp_len` bytes at ISO sector `esp_sector`.
    pub fn new(total: u64, esp_sector: u32, esp_len: u64) -> Result<Self> {
        let s = SECTOR as u64;
        let esp_off = u64::from(esp_sector) * s;
        let esp = Region {
            name: names::ESP,
            offset: esp_off,
            len: esp_len.div_ceil(LBA) * LBA,
        };
        let backup_header = total
            .checked_sub(LBA)
            .ok_or_else(|| Error::bad("tiny image"))?;
        let l = Self {
            total,
            regions: [
                Region {
                    name: names::MBR,
                    offset: 0,
                    len: LBA,
                },
                Region {
                    name: names::HEADER,
                    offset: LBA,
                    len: LBA,
                },
                Region {
                    name: names::ARRAY,
                    offset: 2 * LBA,
                    len: ARRAY_BYTES,
                },
                Region {
                    name: names::PAYLOAD,
                    offset: 16 * s,
                    len: esp_off.saturating_sub(16 * s),
                },
                esp,
                Region {
                    name: names::BACKUP_ARRAY,
                    offset: backup_header.saturating_sub(ARRAY_BYTES),
                    len: ARRAY_BYTES,
                },
                Region {
                    name: names::BACKUP_HEADER,
                    offset: backup_header,
                    len: LBA,
                },
            ],
        };
        l.check()?;
        Ok(l)
    }

    /// The region named `name`.
    #[must_use]
    pub fn region(&self, name: &str) -> Region {
        *self
            .regions
            .iter()
            .find(|r| r.name == name)
            .expect("every name is in the table")
    }

    /// Regions are in order, apart, and aligned to what they need.
    pub fn check(&self) -> Result<()> {
        if !self.total.is_multiple_of(SECTOR as u64) {
            return Err(Error::bad("hybrid image not whole sectors"));
        }
        for w in self.regions.windows(2) {
            if w[0].end() > w[1].offset {
                return Err(Error::bad(format!("{} overlaps {}", w[0].name, w[1].name)));
            }
        }
        let esp = self.region(names::ESP);
        let payload = self.region(names::PAYLOAD);
        if !esp.offset.is_multiple_of(SECTOR as u64) || payload.len == 0 || esp.len == 0 {
            return Err(Error::bad("boot image misplaced for a hybrid image"));
        }
        if esp.last_lba() > self.last_usable() || payload.lba() < FIRST_USABLE {
            return Err(Error::bad("partitions outside the usable LBAs"));
        }
        if self.region(names::ARRAY).end() > 16 * SECTOR as u64 {
            return Err(Error::bad("GPT does not fit the system area"));
        }
        Ok(())
    }

    /// Last LBA a partition may use.
    #[must_use]
    pub fn last_usable(&self) -> u64 {
        self.total / LBA - FIRST_USABLE
    }

    /// The partitions: `(index, type, region)`.
    #[must_use]
    pub fn partitions(&self) -> [(u32, [u8; 16], Region); 2] {
        [
            (1, EFI_SYSTEM, self.region(names::ESP)),
            (2, QUARTZ_MEDIA, self.region(names::PAYLOAD)),
        ]
    }

    fn array(&self, g: &Guids) -> Vec<u8> {
        let mut a = vec![0u8; usize::try_from(ARRAY_BYTES).expect("16 KiB")];
        for ((idx, ty, r), (uniq, name)) in self
            .partitions()
            .into_iter()
            .zip([(g.esp, ESP_NAME), (g.payload, MEDIA_NAME)])
        {
            let e = &mut a[(idx as usize - 1) * ENTRY_SIZE as usize..][..ENTRY_SIZE as usize];
            e[0..16].copy_from_slice(&mixed(ty));
            e[16..32].copy_from_slice(&mixed(uniq));
            e[32..40].copy_from_slice(&r.lba().to_le_bytes());
            e[40..48].copy_from_slice(&r.last_lba().to_le_bytes());
            for (i, u) in name.encode_utf16().enumerate() {
                e[56 + 2 * i..58 + 2 * i].copy_from_slice(&u.to_le_bytes());
            }
        }
        a
    }

    fn header(&self, g: &Guids, primary: bool, array_crc: u32) -> Vec<u8> {
        let last = self.total / LBA - 1;
        let (me, alt, array_lba) = if primary {
            (1, last, 2)
        } else {
            (last, 1, self.region(names::BACKUP_ARRAY).lba())
        };
        let mut h = vec![0u8; LBA_USIZE];
        h[0..8].copy_from_slice(b"EFI PART");
        h[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        h[12..16].copy_from_slice(&92u32.to_le_bytes());
        h[24..32].copy_from_slice(&me.to_le_bytes());
        h[32..40].copy_from_slice(&alt.to_le_bytes());
        h[40..48].copy_from_slice(&FIRST_USABLE.to_le_bytes());
        h[48..56].copy_from_slice(&self.last_usable().to_le_bytes());
        h[56..72].copy_from_slice(&mixed(g.disk));
        h[72..80].copy_from_slice(&array_lba.to_le_bytes());
        h[80..84].copy_from_slice(&ENTRIES.to_le_bytes());
        h[84..88].copy_from_slice(&ENTRY_SIZE.to_le_bytes());
        h[88..92].copy_from_slice(&array_crc.to_le_bytes());
        let crc = crc32(&h[..92]);
        h[16..20].copy_from_slice(&crc.to_le_bytes());
        h
    }

    /// The first 32 KiB: protective MBR, primary header and entries.
    #[must_use]
    pub fn system_area(&self, g: &Guids) -> Vec<u8> {
        let mut out = vec![0u8; 16 * SECTOR];
        let mbr = &mut out[..512];
        let p = &mut mbr[446..462];
        p[1..4].copy_from_slice(&[0x00, 0x02, 0x00]);
        p[4] = 0xEE;
        p[5..8].copy_from_slice(&[0xFF, 0xFF, 0xFF]);
        p[8..12].copy_from_slice(&1u32.to_le_bytes());
        let size = u32::try_from(self.total / LBA - 1).unwrap_or(u32::MAX);
        p[12..16].copy_from_slice(&size.to_le_bytes());
        mbr[510] = 0x55;
        mbr[511] = 0xAA;
        let array = self.array(g);
        let crc = crc32(&array);
        out[512..1024].copy_from_slice(&self.header(g, true, crc));
        out[1024..1024 + array.len()].copy_from_slice(&array);
        out
    }

    /// The tail: the backup entries and header, from the end of the
    /// boot image's last sector to the end of the image.
    #[must_use]
    pub fn tail(&self, g: &Guids) -> Vec<u8> {
        let start = self.tail_offset();
        let mut out = vec![0u8; usize::try_from(self.total - start).expect("small tail")];
        let array = self.array(g);
        let crc = crc32(&array);
        let a = usize::try_from(self.region(names::BACKUP_ARRAY).offset - start).expect("tail");
        out[a..a + array.len()].copy_from_slice(&array);
        let h = usize::try_from(self.region(names::BACKUP_HEADER).offset - start).expect("tail");
        out[h..h + LBA_USIZE].copy_from_slice(&self.header(g, false, crc));
        out
    }

    /// Where the tail starts.
    #[must_use]
    pub fn tail_offset(&self) -> u64 {
        self.total - u64::from(TAIL_SECTORS) * SECTOR as u64
    }
}

/// The GUIDs of one image, in RFC 4122 byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guids {
    /// The disk.
    pub disk: [u8; 16],
    /// Partition 1.
    pub esp: [u8; 16],
    /// Partition 2.
    pub payload: [u8; 16],
}

impl Guids {
    /// GUIDs derived from `seed` (the volume id and date): the same
    /// image always carries the same ones.
    #[must_use]
    pub fn derived(seed: &[u8]) -> Self {
        let g = |n: u8| {
            let mut out = [0u8; 16];
            for (half, chunk) in out.chunks_mut(8).enumerate() {
                let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ (u64::from(n) << 8 | half as u64);
                for &b in seed {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0100_0000_01b3);
                }
                chunk.copy_from_slice(&h.to_be_bytes());
            }
            out[6] = (out[6] & 0x0F) | 0x40;
            out[8] = (out[8] & 0x3F) | 0x80;
            out
        };
        Self {
            disk: g(0),
            esp: g(1),
            payload: g(2),
        }
    }
}

/// RFC byte order to GPT's mixed-endian order (and back).
#[must_use]
pub fn mixed(g: [u8; 16]) -> [u8; 16] {
    let mut o = g;
    o[0..4].reverse();
    o[4..6].reverse();
    o[6..8].reverse();
    o
}

/// The text form of a GUID.
#[must_use]
pub fn guid_text(g: [u8; 16]) -> String {
    let h = |r: std::ops::Range<usize>| {
        g[r].iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    };
    format!(
        "{}-{}-{}-{}-{}",
        h(0..4),
        h(4..6),
        h(6..8),
        h(8..10),
        h(10..16)
    )
}

/// One partition read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptPart {
    /// 1-based index.
    pub index: u32,
    /// Type, RFC order.
    pub type_guid: [u8; 16],
    /// First LBA.
    pub first_lba: u64,
    /// Last LBA.
    pub last_lba: u64,
    /// Name.
    pub name: String,
}

/// A verified partition table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptInfo {
    /// Disk GUID, RFC order.
    pub disk: [u8; 16],
    /// Used entries.
    pub partitions: Vec<GptPart>,
    /// The backup header's LBA.
    pub backup_lba: u64,
}

fn rd(src: &dyn ReadAt, off: u64, len: u64) -> Result<Vec<u8>> {
    let mut b = vec![0u8; usize::try_from(len).map_err(|_| Error::bad("length"))?];
    src.read_exact_at(off, &mut b)?;
    Ok(b)
}

fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().expect("8"))
}

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4"))
}

fn read_header(src: &dyn ReadAt, lba: u64, what: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let h = rd(src, lba * LBA, LBA)?;
    if &h[0..8] != b"EFI PART" || le32(&h, 12) != 92 {
        return Err(Error::bad(format!("{what}: no GPT header")));
    }
    let mut z = h[..92].to_vec();
    z[16..20].fill(0);
    if crc32(&z) != le32(&h, 16) {
        return Err(Error::bad(format!("{what}: header checksum")));
    }
    if le64(&h, 24) != lba {
        return Err(Error::bad(format!("{what}: header not where it says")));
    }
    let n = u64::from(le32(&h, 80));
    let size = u64::from(le32(&h, 84));
    if n == 0 || n > 1024 || !(128..=1024).contains(&size) {
        return Err(Error::bad(format!("{what}: entry array shape")));
    }
    let array = rd(src, le64(&h, 72) * LBA, n * size)?;
    if crc32(&array) != le32(&h, 88) {
        return Err(Error::bad(format!("{what}: entry array checksum")));
    }
    Ok((h, array))
}

/// Check the protective MBR and both GPTs of a device or image and
/// list the partitions.
pub fn verify(src: &dyn ReadAt) -> Result<GptInfo> {
    let mbr = rd(src, 0, LBA)?;
    if mbr[510] != 0x55 || mbr[511] != 0xAA || mbr[446 + 4] != 0xEE {
        return Err(Error::bad("no protective MBR"));
    }
    let (h, array) = read_header(src, 1, "primary GPT")?;
    let backup_lba = le64(&h, 32);
    let total_lba = src.size()? / LBA;
    if backup_lba + 1 != total_lba {
        return Err(Error::bad("backup GPT header is not the last LBA"));
    }
    let (bh, barray) = read_header(src, backup_lba, "backup GPT")?;
    if barray != array || bh[56..72] != h[56..72] || le64(&bh, 32) != 1 {
        return Err(Error::bad("backup GPT differs from the primary"));
    }
    let size = le32(&h, 84) as usize;
    let mut parts = Vec::new();
    for (i, e) in array.chunks(size).enumerate() {
        if e[0..16].iter().all(|&b| b == 0) {
            continue;
        }
        let units: Vec<u16> = e[56..128]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes(*p))
            .take_while(|&u| u != 0)
            .collect();
        parts.push(GptPart {
            index: u32::try_from(i + 1).unwrap_or(u32::MAX),
            type_guid: mixed(e[0..16].try_into().expect("16")),
            first_lba: le64(e, 32),
            last_lba: le64(e, 40),
            name: String::from_utf16_lossy(&units),
        });
    }
    Ok(GptInfo {
        disk: mixed(h[56..72].try_into().expect("16")),
        partitions: parts,
        backup_lba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_table_gives_both_units() {
        // 100 sectors of payload, a 64 KiB boot image at sector 100.
        let total = (100 + 32 + u64::from(TAIL_SECTORS)) * 2048;
        let l = HybridLayout::new(total, 100, 65536).unwrap();
        let esp = l.region(names::ESP);
        assert_eq!(esp.sector(), 100);
        assert_eq!(esp.lba(), 400);
        assert_eq!(esp.last_lba(), 400 + 128 - 1);
        let p = l.region(names::PAYLOAD);
        assert_eq!((p.lba(), p.last_lba()), (64, 399));
        assert_eq!(l.region(names::BACKUP_HEADER).lba(), total / 512 - 1);
        assert_eq!(l.region(names::BACKUP_ARRAY).lba(), total / 512 - 33);
        assert!(esp.end() <= l.tail_offset());
        let g = Guids::derived(b"seed");
        let mut img = vec![0u8; usize::try_from(total).unwrap()];
        img[..32768].copy_from_slice(&l.system_area(&g));
        let t = l.tail(&g);
        let off = usize::try_from(l.tail_offset()).unwrap();
        img[off..].copy_from_slice(&t);
        let info = verify(&img).unwrap();
        assert_eq!(info.partitions.len(), 2);
        assert_eq!(info.partitions[0].type_guid, EFI_SYSTEM);
        assert_eq!(
            (info.partitions[0].first_lba, info.partitions[0].last_lba),
            (400, 527)
        );
        assert_eq!(info.partitions[1].type_guid, QUARTZ_MEDIA);
        assert_eq!(info.partitions[1].name, MEDIA_NAME);
        assert_eq!(info.disk, g.disk);
        img[600] ^= 1;
        assert!(verify(&img).is_err());
        assert_eq!(
            guid_text(QUARTZ_MEDIA),
            "c8a1e5d2-7f34-4b6e-a2d9-515334304d44"
        );
    }

    #[test]
    fn overlap_is_refused() {
        assert!(HybridLayout::new(40 * 2048, 30, 40 * 2048).is_err());
    }
}
