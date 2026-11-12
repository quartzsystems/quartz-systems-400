// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UEFI-bootable installation media: an ISO 9660 image whose El Torito
//! boot image is a FAT volume holding the media UKI, with the same UKI
//! as a plain file at `EFI/BOOT/BOOTX64.EFI`, so a copy of the tree onto
//! a FAT stick boots too. The image is a hybrid: its GPT makes the boot
//! image an EFI System partition when the ISO is written raw to a disk.
//! Everything else on the media is the product's (`Product::media`).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use diskimg::fat;
use optical::IsoBuilder;
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::{sha256_bytes, sha256_file, short, write_sum};
use crate::host::paths;
use crate::kernel::{SOURCE_DATE_EPOCH, UkiKind};

/// The media UKI's path inside the boot image and in the ISO tree.
pub const BOOT_FILE: &str = "EFI/BOOT/BOOTX64.EFI";
/// The El Torito boot image.
pub const BOOT_IMAGE: &str = "EFI.IMG";
/// What El Torito's sixteen-bit count of 512-byte sectors can name.
pub const BOOT_IMAGE_MAX: u64 = 65_535 * 512;

/// One entry of the media's tree, written in the order given.
#[derive(Debug, Clone)]
pub enum Item {
    /// A file: its path and bytes.
    File(String, Vec<u8>),
    /// A second name for a file already in the tree: path, target.
    Link(String, String),
    /// The boot image ([`BOOT_IMAGE`]) and the loose UKI ([`BOOT_FILE`]).
    Boot,
}

/// What the product puts on its media.
#[derive(Debug, Clone)]
pub struct Media {
    /// The ISO volume identifier.
    pub volume_id: String,
    /// The tree, with one [`Item::Boot`].
    pub tree: Vec<Item>,
    /// Files beside the UKI in the boot image (a Secure Boot key to
    /// enrol from), by path.
    pub boot_files: Vec<(String, Vec<u8>)>,
    /// Paths of the tree that must hold the media UKI's bytes too.
    pub uki_copies: Vec<String>,
}

/// The El Torito boot image: the media UKI and `files` beside it.
pub fn boot_image(uki: &[u8], files: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let spec = fat::media_esp(uki.len() as u64);
    // El Torito names the boot image's size in 512-byte sectors, 16 bits:
    // a larger image boots only on firmware that ignores the field.
    ensure!(
        spec.bytes <= BOOT_IMAGE_MAX,
        "the media UKI ({} bytes) needs a {} MiB boot image; El Torito holds at most 32 MiB",
        uki.len(),
        spec.bytes / (1024 * 1024)
    );
    info!(
        "iso: building {} MiB FAT16 boot image",
        spec.bytes / (1024 * 1024)
    );
    let mut fat_files: Vec<(&str, &[u8])> = vec![(BOOT_FILE, uki)];
    fat_files.extend(files.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
    let efi = fat::build(&spec, &fat_files)?;
    let listing = fat::list(&efi)?;
    let mut want: Vec<String> = Vec::new();
    for (path, _) in &fat_files {
        let mut dir = String::new();
        for part in path.split('/') {
            dir.push_str(part);
            if dir.len() < path.len() {
                dir.push('/');
            }
            if !want.contains(&dir) {
                want.push(dir.clone());
            }
        }
    }
    want.sort();
    ensure!(
        listing == want,
        "boot image contents unexpected: {listing:?}"
    );
    Ok(efi)
}

/// The image's tree and options: `tree` in order, [`Item::Boot`] as the
/// boot image `efi` and the loose `uki`.
pub fn compose(
    volume_id: &str,
    epoch: u64,
    efi: Vec<u8>,
    uki: Vec<u8>,
    tree: Vec<Item>,
) -> Result<IsoBuilder> {
    let mut b = IsoBuilder::new(volume_id, epoch);
    let mut boot = Some((efi, uki));
    for item in tree {
        match item {
            Item::File(path, data) => {
                b.add_file(&path, data)?;
            }
            Item::Link(path, target) => {
                b.add_link(&path, &target)?;
            }
            Item::Boot => {
                let (efi, uki) = boot.take().context("the media tree has two boot items")?;
                b.add_file(BOOT_IMAGE, efi)?;
                b.add_file(BOOT_FILE, uki)?;
            }
        }
    }
    ensure!(boot.is_none(), "the media tree has no boot item");
    b.boot_image(BOOT_IMAGE)?;
    // Protective MBR and GPT: a raw copy onto a disk boots under UEFI.
    b.hybrid(true);
    Ok(b)
}

/// `build iso`: the media around the media UKI.
pub fn iso(ctx: &Ctx) -> Result<PathBuf> {
    let media = ctx
        .product
        .media
        .context("the product has no installation media")?;
    let uki = paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::Media))
        .context("no media UKI; run `uki` first")?;
    let uki_bytes = fs::read(&uki)?;
    let hash = sha256_file(&uki)?;
    let m = media(ctx, &uki_bytes)?;
    let efi = boot_image(&uki_bytes, &m.boot_files)?;
    let epoch: u64 = SOURCE_DATE_EPOCH.parse()?;
    let copies: Vec<&str> = m.uki_copies.iter().map(String::as_str).collect();
    let image = compose(&m.volume_id, epoch, efi, uki_bytes.clone(), m.tree)?.build()?;
    let checked = validate(&image, &uki_bytes, &copies)?;
    info!("iso: {checked}");

    let out = ctx.paths.images().join(format!(
        "{}-{}-{}.iso",
        ctx.product.vendor,
        ctx.product.release,
        short(&hash)
    ));
    fs::write(&out, &image).with_context(|| format!("cannot write {}", out.display()))?;
    paths::write_pointer(&ctx.paths.iso_pointer(), &out)?;
    let sum = sha256_bytes(&image);
    let sum_file = write_sum(&out, &sum)?;
    info!(
        "iso: {} ({} bytes, sha256 {sum}, {})",
        out.display(),
        image.len(),
        sum_file.display()
    );
    Ok(out)
}

/// Read the image back with our own readers and check that it boots
/// all three ways: the El Torito entry is EFI and points at the boot
/// image, the boot image holds the UKI, the tree holds the same bytes
/// (at [`BOOT_FILE`] and each of `copies`), partition 1 is exactly the
/// boot image, and the MBR and both GPTs check out. Returns one line
/// for the log.
pub fn validate(image: &[u8], uki: &[u8], copies: &[&str]) -> Result<String> {
    use optical::iso::{boot::PLATFORM_EFI, hybrid};
    let img = optical::IsoImage::open(image).context("ISO does not read back")?;
    let entry = img.info().boot.context("no El Torito boot entry")?;
    ensure!(
        entry.platform == PLATFORM_EFI && entry.bootable && entry.media == 0,
        "El Torito entry is not a bootable EFI no-emulation entry: {entry:?}"
    );
    let efi = img.lookup(BOOT_IMAGE)?;
    ensure!(
        efi.extents.len() == 1 && efi.extents[0].0 == entry.lba,
        "El Torito entry does not point at {BOOT_IMAGE}"
    );
    let efi_bytes = img.read_file(&efi)?;
    let inner = fat::read_file(&efi_bytes, BOOT_FILE)
        .with_context(|| format!("{BOOT_IMAGE} has no {BOOT_FILE}"))?;
    ensure!(
        inner == uki,
        "{BOOT_IMAGE}'s {BOOT_FILE} is not the media UKI"
    );
    for path in std::iter::once(BOOT_FILE).chain(copies.iter().copied()) {
        let loose = img.read_file(&img.lookup(path)?)?;
        ensure!(
            loose == inner,
            "{path} in the ISO tree differs from the boot image's"
        );
    }
    let gpt = hybrid::verify(&image).context("partition table")?;
    let p1 = gpt
        .partitions
        .iter()
        .find(|p| p.index == 1)
        .context("no partition 1")?;
    let first = u64::from(entry.lba) * 4;
    let last = first + efi.size.div_ceil(512) - 1;
    ensure!(
        p1.type_guid == hybrid::EFI_SYSTEM && p1.first_lba == first && p1.last_lba == last,
        "partition 1 is LBAs {}..={}, the boot image {first}..={last}",
        p1.first_lba,
        p1.last_lba
    );
    let p2 = gpt
        .partitions
        .iter()
        .find(|p| p.index == 2)
        .context("no partition 2")?;
    ensure!(
        p2.type_guid == hybrid::QUARTZ_MEDIA && p2.first_lba == 64 && p2.last_lba + 1 == first,
        "partition 2 does not cover the ISO 9660 payload"
    );
    Ok(format!(
        "El Torito EFI at sector {}, ESP LBAs {first}..={last}, payload LBAs 64..={}, GPT and backup valid",
        entry.lba, p2.last_lba
    ))
}
