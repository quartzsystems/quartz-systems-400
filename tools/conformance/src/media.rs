// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The install ISO's other two lives: written raw onto a disk
//! (`--media iso-raw`), and its file tree copied onto a fresh FAT32
//! stick the way Rufus extracts it (`--media iso-fat`).

use std::fs;
use std::time::Duration;

use optical::IsoImage;

use crate::error::HarnessError;
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, StepReport, Suite};

/// Where the FAT32 partition starts, in 512-byte sectors.
const FAT_START: u64 = 2048;
/// MBR partition type: FAT32 with LBA.
const FAT32_LBA: u8 = 0x0C;

/// The ISO's files, read with our own reader: `(path, bytes)`.
pub fn iso_files(iso: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let img = IsoImage::open(iso).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (path, e) in img.walk().map_err(|e| e.to_string())? {
        if !e.is_dir() {
            out.push((path, img.read_file(&e).map_err(|e| e.to_string())?));
        }
    }
    Ok(out)
}

/// A disk image: an MBR with one FAT32 partition holding every file of
/// the ISO's tree.
pub fn fat_stick(iso: &[u8]) -> Result<Vec<u8>, String> {
    let files = iso_files(iso)?;
    let payload: u64 = files.iter().map(|(_, d)| d.len() as u64).sum();
    let spec = diskimg::fat::fat32_for(payload);
    let refs: Vec<(&str, &[u8])> = files.iter().map(|(p, d)| (p.as_str(), d.as_slice())).collect();
    let vol = diskimg::fat::build(&spec, &refs).map_err(|e| e.to_string())?;
    let start = usize::try_from(FAT_START * 512).expect("1 MiB");
    let mut disk = vec![0u8; start];
    let sectors = u32::try_from(vol.len() / 512).map_err(|_| "volume too large".to_string())?;
    let mbr = &mut disk[..512];
    mbr[440..444].copy_from_slice(&0x5153_3430u32.to_le_bytes());
    let p = &mut mbr[446..462];
    p[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
    p[4] = FAT32_LBA;
    p[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
    p[8..12].copy_from_slice(&u32::try_from(FAT_START).expect("small").to_le_bytes());
    p[12..16].copy_from_slice(&sectors.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    disk.extend_from_slice(&vol);
    Ok(disk)
}

impl Suite {
    /// The ISO as a disk: `fat` copies its tree onto a FAT32 stick,
    /// else the image is written raw. The medium boots alone into the
    /// D IPL and reaches sign-on; then installs onto a blank disk beside
    /// it, which boots on its own.
    pub(crate) fn run_media_disk(&self, fat: bool) -> Result<Vec<StepReport>, HarnessError> {
        let iso = self.iso.clone().expect("checked in new");
        let medium = self
            .out_dir
            .join(if fat { "media-fat.img" } else { "media-raw.img" });
        if fat {
            let bytes = fs::read(&iso)?;
            let disk = fat_stick(&bytes).map_err(|reason| HarnessError::Failed {
                step: "0 FAT32 stick",
                reason,
                log: String::new(),
            })?;
            fs::write(&medium, disk)?;
        } else {
            fs::copy(&iso, &medium)?;
        }
        let mut reports = Vec::new();
        let alone = Launch {
            image: Some(medium.clone()),
            cdrom: None,
            cdrom_first: false,
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&alone)?;
        let name = if fat {
            "1 media IPL from FAT32"
        } else {
            "1 media IPL from raw disk"
        };
        reports.push(self.step(&mut s, name, Self::media_ipl)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);

        // 2. With a blank disk beside it: option 4 must find the payload
        // on the medium and install onto the blank disk, not the medium.
        let blank = self.out_dir.join("media-target.img");
        let f = fs::File::create(&blank)?;
        f.set_len(4 * 1024 * 1024 * 1024)?;
        drop(f);
        let install = Launch {
            extra_disks: vec![blank.clone()],
            ..alone
        };
        let mut s = Session::spawn(&install)?;
        reports.push(self.step(&mut s, "2 automatic install", Self::automatic_install)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        drop(s);

        // 3. The installed disk on its own.
        let disk = Launch {
            image: Some(blank),
            extra_disks: Vec::new(),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&disk)?;
        reports.push(self.step(&mut s, "3 boot installed disk", Self::installed_boot)?);
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stick_holds_the_tree() {
        let mut b = optical::IsoBuilder::new("QS400_T", 0);
        b.add_file("EFI/BOOT/BOOTX64.EFI", b"uki".to_vec()).unwrap();
        b.add_file("QS400/MEDIA.MANIFEST", b"m".to_vec()).unwrap();
        let iso = b.build().unwrap();
        let disk = fat_stick(&iso).unwrap();
        assert_eq!(&disk[510..512], &[0x55, 0xAA]);
        assert_eq!(disk[446 + 4], FAT32_LBA);
        let vol = &disk[usize::try_from(FAT_START * 512).unwrap()..];
        assert_eq!(&vol[82..87], b"FAT32");
        assert_eq!(
            diskimg::fat::read_file(vol, "QS400/MEDIA.MANIFEST").unwrap(),
            b"m"
        );
        assert_eq!(
            diskimg::fat::read_file(vol, "EFI/BOOT/BOOTX64.EFI").unwrap(),
            b"uki"
        );
    }
}
