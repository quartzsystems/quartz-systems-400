// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A minimal ISO 9660 (ECMA-119) level 2 writer with an El Torito boot
//! catalog holding one EFI no-emulation entry. No Joliet, no Rock Ridge.
//! Deterministic: timestamps come from the caller, records are sorted.

use std::fmt::Write as _;

use anyhow::{Result, ensure};

/// Logical sector size.
pub const SECTOR: usize = 2048;
const SECTOR_U32: u32 = 2048;
const SECTOR_U16: u16 = 2048;
const PVD_SECTOR: u32 = 16;
const BOOT_RECORD_SECTOR: u32 = 17;
const TERMINATOR_SECTOR: u32 = 18;
const CATALOG_SECTOR: u32 = 19;
const PATH_TABLE_L_SECTOR: u32 = 20;
const PATH_TABLE_M_SECTOR: u32 = 21;
const ROOT_DIR_SECTOR: u32 = 22;
const FIRST_FILE_SECTOR: u32 = 23;

/// Strings Quartz puts in the volume descriptor.
pub const PUBLISHER: &str = "QUARTZ SYSTEMS";
/// Data preparer.
pub const PREPARER: &str = "QS/400 BUILD";
/// Application identifier.
pub const APPLICATION: &str = "QS/400 INSTALLATION MEDIA";
/// El Torito validation entry id.
const CATALOG_ID: &[u8] = b"QUARTZ SYSTEMS";

/// One file in the root directory.
struct File {
    /// ISO 9660 identifier without the `;1`.
    name: String,
    data: Vec<u8>,
}

/// Builds one image. Files live in the root directory only.
pub struct IsoBuilder {
    volume_id: String,
    files: Vec<File>,
    boot: Option<String>,
    epoch: u64,
}

impl IsoBuilder {
    /// A new image. `volume_id`: up to 32 d-characters (`A-Z 0-9 _`).
    /// `epoch`: Unix time used for every timestamp.
    pub fn new(volume_id: &str, epoch: u64) -> Self {
        Self {
            volume_id: volume_id.to_string(),
            files: Vec::new(),
            boot: None,
            epoch,
        }
    }

    /// Add a root-directory file. The name must be an ISO 9660 level 2
    /// identifier (`A-Z 0-9 _`, one dot, at most 30 characters).
    pub fn add_file(&mut self, name: &str, data: Vec<u8>) -> Result<&mut Self> {
        ensure!(valid_name(name), "not an ISO 9660 identifier: {name}");
        ensure!(
            !self.files.iter().any(|f| f.name == name),
            "duplicate {name}"
        );
        self.files.push(File {
            name: name.to_string(),
            data,
        });
        Ok(self)
    }

    /// Mark a file as the El Torito EFI boot image (a FAT volume image).
    pub fn boot_image(&mut self, name: &str) -> Result<&mut Self> {
        ensure!(
            self.files.iter().any(|f| f.name == name),
            "no such file {name}"
        );
        self.boot = Some(name.to_string());
        Ok(self)
    }

    /// Serialize the image.
    pub fn build(mut self) -> Result<Vec<u8>> {
        ensure!(
            valid_name(&self.volume_id) || self.volume_id.bytes().all(is_d_char),
            "bad volume id"
        );
        ensure!(self.volume_id.len() <= 32, "volume id longer than 32");
        self.files.sort_by(|a, b| a.name.cmp(&b.name));

        // Lay out the data area.
        let mut extents = Vec::with_capacity(self.files.len());
        let mut next = FIRST_FILE_SECTOR;
        for f in &self.files {
            extents.push(next);
            next += sectors(f.data.len());
        }
        let total_sectors = next;

        let mut out = vec![0u8; total_sectors as usize * SECTOR];
        let date = self.long_date();
        let rec_date = self.record_date();

        // Root directory.
        let mut root = Vec::new();
        root.extend(dir_record(
            &[0],
            ROOT_DIR_SECTOR,
            SECTOR_U32,
            true,
            rec_date,
        ));
        root.extend(dir_record(
            &[1],
            ROOT_DIR_SECTOR,
            SECTOR_U32,
            true,
            rec_date,
        ));
        for (f, &extent) in self.files.iter().zip(&extents) {
            let id = format!("{};1", f.name);
            root.extend(dir_record(
                id.as_bytes(),
                extent,
                u32::try_from(f.data.len())?,
                false,
                rec_date,
            ));
        }
        ensure!(root.len() <= SECTOR, "root directory exceeds one sector");
        put(&mut out, ROOT_DIR_SECTOR, &root);

        // Path tables: one entry, the root.
        let mut ptl = vec![1, 0];
        ptl.extend_from_slice(&ROOT_DIR_SECTOR.to_le_bytes());
        ptl.extend_from_slice(&1u16.to_le_bytes());
        ptl.extend_from_slice(&[0, 0]);
        let mut ptm = vec![1, 0];
        ptm.extend_from_slice(&ROOT_DIR_SECTOR.to_be_bytes());
        ptm.extend_from_slice(&1u16.to_be_bytes());
        ptm.extend_from_slice(&[0, 0]);
        put(&mut out, PATH_TABLE_L_SECTOR, &ptl);
        put(&mut out, PATH_TABLE_M_SECTOR, &ptm);

        let pvd = self.primary_descriptor(total_sectors, &ptl, &date, rec_date)?;
        put(&mut out, PVD_SECTOR, &pvd);

        // Boot record volume descriptor and the catalog.
        if let Some(boot) = &self.boot {
            let i = self
                .files
                .iter()
                .position(|f| &f.name == boot)
                .expect("checked");
            let mut br = vec![0u8; SECTOR];
            br[1..6].copy_from_slice(b"CD001");
            br[6] = 1;
            br[7..7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
            br[71..75].copy_from_slice(&CATALOG_SECTOR.to_le_bytes());
            put(&mut out, BOOT_RECORD_SECTOR, &br);
            let catalog = boot_catalog(extents[i], self.files[i].data.len());
            put(&mut out, CATALOG_SECTOR, &catalog);
        }

        // Terminator.
        let mut term = vec![0u8; SECTOR];
        term[0] = 255;
        term[1..6].copy_from_slice(b"CD001");
        term[6] = 1;
        put(&mut out, TERMINATOR_SECTOR, &term);

        // File data.
        for (f, &extent) in self.files.iter().zip(&extents) {
            put(&mut out, extent, &f.data);
        }
        Ok(out)
    }

    /// The primary volume descriptor (sector 16).
    fn primary_descriptor(
        &self,
        total_sectors: u32,
        path_table: &[u8],
        date: &str,
        rec_date: [u8; 7],
    ) -> Result<Vec<u8>> {
        let mut pvd = vec![0u8; SECTOR];
        pvd[0] = 1;
        pvd[1..6].copy_from_slice(b"CD001");
        pvd[6] = 1;
        pad_str(&mut pvd[8..40], "QS/400");
        pad_str(&mut pvd[40..72], &self.volume_id);
        both_u32(&mut pvd[80..88], total_sectors);
        both_u16(&mut pvd[120..124], 1);
        both_u16(&mut pvd[124..128], 1);
        both_u16(&mut pvd[128..132], SECTOR_U16);
        both_u32(&mut pvd[132..140], u32::try_from(path_table.len())?);
        pvd[140..144].copy_from_slice(&PATH_TABLE_L_SECTOR.to_le_bytes());
        pvd[148..152].copy_from_slice(&PATH_TABLE_M_SECTOR.to_be_bytes());
        let root_rec = dir_record(&[0], ROOT_DIR_SECTOR, SECTOR_U32, true, rec_date);
        pvd[156..156 + root_rec.len()].copy_from_slice(&root_rec);
        pad_str(&mut pvd[190..318], &self.volume_id);
        pad_str(&mut pvd[318..446], PUBLISHER);
        pad_str(&mut pvd[446..574], PREPARER);
        pad_str(&mut pvd[574..702], APPLICATION);
        for off in [702, 739, 776] {
            pad_str(&mut pvd[off..off + 37], "");
        }
        for off in [813, 830, 847, 864] {
            pvd[off..off + 17].copy_from_slice(date.as_bytes());
        }
        pvd[881] = 1;
        Ok(pvd)
    }

    /// `YYYYMMDDHHMMSScc` plus a zero time zone byte.
    fn long_date(&self) -> String {
        let (year, month, day, hour, minute, second) = civil(self.epoch);
        let mut text = String::new();
        let _ = write!(
            text,
            "{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}00"
        );
        text.push('\0');
        text
    }

    /// The seven-byte directory record date.
    fn record_date(&self) -> [u8; 7] {
        let (year, month, day, hour, minute, second) = civil(self.epoch);
        [
            u8::try_from(year - 1900).unwrap_or(0),
            u8::try_from(month).unwrap_or(1),
            u8::try_from(day).unwrap_or(1),
            u8::try_from(hour).unwrap_or(0),
            u8::try_from(minute).unwrap_or(0),
            u8::try_from(second).unwrap_or(0),
            0,
        ]
    }
}

/// Validation entry plus the default EFI no-emulation entry.
fn boot_catalog(image_sector: u32, image_len: usize) -> Vec<u8> {
    let mut v = vec![0u8; 32];
    v[0] = 0x01; // header id
    v[1] = 0xEF; // platform: EFI
    v[4..4 + CATALOG_ID.len()].copy_from_slice(CATALOG_ID);
    v[30] = 0x55;
    v[31] = 0xAA;
    // Checksum: the sixteen little-endian words sum to zero.
    let sum: u16 = v
        .as_chunks::<2>()
        .0
        .iter()
        .map(|w| u16::from_le_bytes(*w))
        .fold(0u16, u16::wrapping_add);
    v[28..30].copy_from_slice(&(0u16.wrapping_sub(sum)).to_le_bytes());

    let mut e = vec![0u8; 32];
    e[0] = 0x88; // bootable
    e[1] = 0x00; // no emulation
    // load segment 0, system type 0, unused
    // Past sixteen bits the count is 1: firmware then takes the FAT
    // volume's own size, where a capped count would cut it short.
    let sectors512 = u16::try_from(image_len.div_ceil(512)).unwrap_or(1);
    e[6..8].copy_from_slice(&sectors512.to_le_bytes());
    e[8..12].copy_from_slice(&image_sector.to_le_bytes());

    let mut cat = vec![0u8; SECTOR];
    cat[..32].copy_from_slice(&v);
    cat[32..64].copy_from_slice(&e);
    cat
}

/// A directory record for `id` (already with `;1` for files).
fn dir_record(id: &[u8], extent: u32, len: u32, dir: bool, date: [u8; 7]) -> Vec<u8> {
    let mut r = vec![0u8; 33];
    both_u32(&mut r[2..10], extent);
    both_u32(&mut r[10..18], len);
    r[18..25].copy_from_slice(&date);
    r[25] = u8::from(dir) << 1;
    both_u16(&mut r[28..32], 1);
    r[32] = u8::try_from(id.len()).expect("short identifier");
    r.extend_from_slice(id);
    if r.len() % 2 == 1 {
        r.push(0);
    }
    r[0] = u8::try_from(r.len()).expect("record under 256 bytes");
    r
}

fn sectors(len: usize) -> u32 {
    u32::try_from(len.div_ceil(SECTOR)).expect("image under 8 TiB")
}

fn put(out: &mut [u8], sector: u32, data: &[u8]) {
    let off = sector as usize * SECTOR;
    out[off..off + data.len()].copy_from_slice(data);
}

fn both_u32(dst: &mut [u8], v: u32) {
    dst[..4].copy_from_slice(&v.to_le_bytes());
    dst[4..8].copy_from_slice(&v.to_be_bytes());
}

fn both_u16(dst: &mut [u8], v: u16) {
    dst[..2].copy_from_slice(&v.to_le_bytes());
    dst[2..4].copy_from_slice(&v.to_be_bytes());
}

/// Space-padded a-character field.
fn pad_str(dst: &mut [u8], s: &str) {
    dst.fill(b' ');
    for (d, b) in dst.iter_mut().zip(s.bytes()) {
        *d = b;
    }
}

fn is_d_char(b: u8) -> bool {
    b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'
}

/// Level 2 identifier: d-characters with at most one dot, 30 characters.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 30
        && name.matches('.').count() <= 1
        && name.bytes().all(|b| is_d_char(b) || b == b'.')
}

/// Unix seconds to civil UTC: (year, month, day, hour, minute, second).
fn civil(epoch: u64) -> (i64, u32, u32, u32, u32, u32) {
    let secs_total = i64::try_from(epoch).unwrap_or(0);
    let days = secs_total.div_euclid(86_400);
    let secs = secs_total.rem_euclid(86_400);
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let doe = shifted.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        month,
        day,
        u32::try_from(secs / 3600).unwrap_or(0),
        u32::try_from((secs % 3600) / 60).unwrap_or(0),
        u32::try_from(secs % 60).unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> Vec<u8> {
        let mut b = IsoBuilder::new("QS400_TEST", 1_767_225_600);
        b.add_file("EFI.IMG", vec![7u8; 5000]).unwrap();
        b.add_file("QS400.TXT", b"hello".to_vec()).unwrap();
        b.boot_image("EFI.IMG").unwrap();
        b.build().unwrap()
    }

    #[test]
    fn descriptors_are_where_they_should_be() {
        let iso = image();
        assert_eq!(iso.len() % SECTOR, 0);
        let pvd = &iso[16 * SECTOR..17 * SECTOR];
        assert_eq!(pvd[0], 1);
        assert_eq!(&pvd[1..6], b"CD001");
        assert_eq!(&pvd[40..50], b"QS400_TEST");
        assert_eq!(&pvd[813..829], b"2026010100000000");
        let total = u32::from_le_bytes(pvd[80..84].try_into().unwrap());
        assert_eq!(total as usize * SECTOR, iso.len());
        let br = &iso[17 * SECTOR..18 * SECTOR];
        assert_eq!(br[0], 0);
        assert_eq!(&br[7..30], b"EL TORITO SPECIFICATION");
        assert_eq!(u32::from_le_bytes(br[71..75].try_into().unwrap()), 19);
        assert_eq!(iso[18 * SECTOR], 255);
    }

    #[test]
    fn boot_catalog_checks_out() {
        let iso = image();
        let cat = &iso[19 * SECTOR..20 * SECTOR];
        assert_eq!((cat[0], cat[1], cat[30], cat[31]), (0x01, 0xEF, 0x55, 0xAA));
        let sum: u16 = cat[..32]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| u16::from_le_bytes(*w))
            .fold(0u16, u16::wrapping_add);
        assert_eq!(sum, 0);
        assert_eq!(cat[32], 0x88);
        assert_eq!(cat[33], 0);
        assert_eq!(u16::from_le_bytes([cat[38], cat[39]]), 10); // 5000 bytes = 10 sectors of 512
        let rba = u32::from_le_bytes(cat[40..44].try_into().unwrap());
        assert_eq!(
            &iso[rba as usize * SECTOR..rba as usize * SECTOR + 4],
            &[7, 7, 7, 7]
        );
    }

    #[test]
    fn root_directory_lists_files_sorted_and_is_deterministic() {
        let iso = image();
        let root = &iso[22 * SECTOR..23 * SECTOR];
        let text = String::from_utf8_lossy(root);
        let a = text.find("EFI.IMG;1").unwrap();
        let b = text.find("QS400.TXT;1").unwrap();
        assert!(a < b);
        assert_eq!(image(), iso);
        assert!(
            IsoBuilder::new("X", 0)
                .add_file("bad name", vec![])
                .is_err()
        );
    }
}
