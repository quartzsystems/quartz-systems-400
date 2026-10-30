// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build iso`: UEFI-bootable installation media.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::{sha256_bytes, sha256_file, short};
use crate::host::paths;
use crate::image::iso;
use crate::kernel::{SOURCE_DATE_EPOCH, UkiKind};
use diskimg::fat;

/// The media UKI's path inside the boot image.
pub const BOOT_FILE: &str = "EFI/BOOT/BOOTX64.EFI";

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
    let mut b = iso::IsoBuilder::new(&format!("QS400_{release}"), epoch);
    b.add_file("EFI.IMG", efi)?;
    b.add_file("QS400A.EFI", slot_a)?;
    b.add_file("QS400B.EFI", slot_b)?;
    b.add_file("QS400.TXT", readme.into_bytes())?;
    b.boot_image("EFI.IMG")?;
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
