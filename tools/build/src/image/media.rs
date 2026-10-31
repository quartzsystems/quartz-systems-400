// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build iso`: UEFI-bootable installation media. The El Torito boot
//! image holds the media UKI; the same UKI is also a plain file at
//! `EFI/BOOT/BOOTX64.EFI` and `QS400/LIC/qs400.efi`, so a copy of the
//! tree onto a FAT stick boots too. The image is a hybrid: its GPT
//! makes the boot image an EFI System partition when the ISO is
//! written raw to a disk (docs/MEDIA.md, "Hybrid install image").

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use optical::IsoBuilder;
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::{sha256_bytes, sha256_file, short};
use crate::host::paths;
use crate::kernel::{SOURCE_DATE_EPOCH, UkiKind};
use diskimg::fat;

/// The media UKI's path inside the boot image and in the ISO tree.
pub const BOOT_FILE: &str = "EFI/BOOT/BOOTX64.EFI";
/// The El Torito boot image.
pub const BOOT_IMAGE: &str = "EFI.IMG";
/// The media UKI under the LIC directory.
pub const LIC_FILE: &str = "QS400/LIC/qs400.efi";
/// Slot A's UKI, which the installer copies onto the disk.
pub const SLOT_A_FILE: &str = "QS400A.EFI";
/// Slot B's UKI.
pub const SLOT_B_FILE: &str = "QS400B.EFI";
/// The text file at the root.
pub const README_FILE: &str = "QS400.TXT";

/// What goes on the media.
pub struct Parts<'a> {
    /// Volume identifier.
    pub volume_id: &'a str,
    /// Unix time of every timestamp.
    pub epoch: u64,
    /// The FAT boot image holding [`BOOT_FILE`].
    pub efi: Vec<u8>,
    /// The media UKI.
    pub uki: Vec<u8>,
    /// Slot A's UKI.
    pub slot_a: Vec<u8>,
    /// Slot B's UKI.
    pub slot_b: Vec<u8>,
    /// The readme.
    pub readme: String,
}

/// The image's tree and options.
pub fn compose(p: Parts<'_>) -> Result<IsoBuilder> {
    let mut b = IsoBuilder::new(p.volume_id, p.epoch);
    b.add_file(BOOT_IMAGE, p.efi)?;
    b.add_file(BOOT_FILE, p.uki)?;
    b.add_link(LIC_FILE, BOOT_FILE)?;
    b.add_file(SLOT_A_FILE, p.slot_a)?;
    b.add_file(SLOT_B_FILE, p.slot_b)?;
    b.add_file(README_FILE, p.readme.into_bytes())?;
    b.boot_image(BOOT_IMAGE)?;
    // Protective MBR and GPT: a raw copy onto a disk boots under UEFI.
    b.hybrid(true);
    Ok(b)
}

/// Build the ISO from the media UKI.
pub fn iso(ctx: &Ctx) -> Result<PathBuf> {
    let uki = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::Media))
        .context("no media UKI; run `uki` first")?;
    let uki_bytes = fs::read(&uki)?;
    let hash = sha256_file(&uki)?;
    // The installer copies the slot UKIs onto the disk's ESP.
    let slot_a = fs::read(paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))?)?;
    let slot_b = fs::read(paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotB))?)?;

    let spec = fat::media_esp(uki_bytes.len() as u64);
    info!(
        "iso: building {} MiB FAT16 boot image",
        spec.bytes / (1024 * 1024)
    );
    let efi = fat::build(&spec, &[(BOOT_FILE, &uki_bytes)])?;
    let listing = fat::list(&efi)?;
    ensure!(
        listing == ["EFI/", "EFI/BOOT/", BOOT_FILE],
        "boot image contents unexpected: {listing:?}"
    );

    let release = release::RELEASE.to_string();
    let readme = format!(
        "QS/400 {}\nUEFI installation media. Boots the system from this volume\n(IPL type D). Write to a USB stick with dd; see docs/PHASE1.md.\n",
        release::full()
    );
    let epoch: u64 = SOURCE_DATE_EPOCH.parse()?;
    let volume_id = format!("QS400_{release}");
    let b = compose(Parts {
        volume_id: &volume_id,
        epoch,
        efi,
        uki: uki_bytes,
        slot_a,
        slot_b,
        readme,
    })?;
    let image = b.build()?;

    let out = ctx
        .paths
        .images()
        .join(format!("qs400-{release}-{}.iso", short(&hash)));
    fs::write(&out, &image).with_context(|| format!("cannot write {}", out.display()))?;
    paths::write_pointer(&ctx.paths.iso_pointer(), &out)?;
    info!(
        "iso: {} ({} bytes, sha256 {})",
        out.display(),
        image.len(),
        sha256_bytes(&image)
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use optical::IsoImage;

    fn parts() -> Parts<'static> {
        Parts {
            volume_id: "QS400_V1R0M0",
            epoch: 1_767_225_600,
            efi: fat::build(&fat::MEDIA_ESP, &[(BOOT_FILE, b"uki")]).unwrap(),
            uki: b"uki".to_vec(),
            slot_a: b"slot a".to_vec(),
            slot_b: b"slot b".to_vec(),
            readme: "readme\n".into(),
        }
    }

    #[test]
    fn the_tree_carries_the_loose_efi_files() {
        let iso = compose(parts()).unwrap().build().unwrap();
        assert_eq!(compose(parts()).unwrap().build().unwrap(), iso);
        let img = IsoImage::open(iso.as_slice()).unwrap();
        assert!(img.info().joliet && img.info().rock_ridge);
        for (path, want) in [
            (BOOT_FILE, &b"uki"[..]),
            (LIC_FILE, b"uki"),
            (SLOT_A_FILE, b"slot a"),
            (SLOT_B_FILE, b"slot b"),
        ] {
            let e = img.lookup(path).unwrap();
            assert_eq!(img.read_file(&e).unwrap(), want, "{path}");
        }
        let boot = img.info().boot.unwrap();
        assert_eq!(img.lookup(BOOT_IMAGE).unwrap().extents[0].0, boot.lba);
        let gpt = optical::iso::hybrid::verify(&img.source().to_vec()).unwrap();
        let esp = img.lookup(BOOT_IMAGE).unwrap();
        assert_eq!(gpt.partitions[0].first_lba, u64::from(boot.lba) * 4);
        assert_eq!(gpt.partitions[0].last_lba, u64::from(boot.lba) * 4 + esp.size / 512 - 1);
        assert_eq!(gpt.partitions[1].type_guid, optical::iso::hybrid::QUARTZ_MEDIA);
        // The installer's reader still finds the slot UKIs.
        let mut cur = std::io::Cursor::new(iso);
        let (id, _) = diskimg::iso9660::read_root(&mut cur).unwrap();
        assert_eq!(id, "QS400_V1R0M0");
        assert_eq!(
            diskimg::iso9660::read_file(&mut cur, SLOT_A_FILE).unwrap(),
            b"slot a"
        );
    }
}
