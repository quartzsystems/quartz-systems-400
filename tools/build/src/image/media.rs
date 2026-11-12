// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build iso`: UEFI-bootable installation media. The El Torito boot
//! image holds the media UKI; the same UKI is also a plain file at
//! `EFI/BOOT/BOOTX64.EFI` and `QS400/LIC/qs400.efi`, so a copy of the
//! tree onto a FAT stick boots too. The media UKI's initramfs holds
//! only the loader, to fit the boot image whatever the system grows to;
//! `slic` and the job program are `QS400/LIC/SLIC` and `QS400/LIC/QSJOB`,
//! in the signed manifest. The image is a hybrid: its GPT
//! makes the boot image an EFI System partition when the ISO is
//! written raw to a disk (docs/MEDIA.md, "Hybrid install image").
//! The product loads `build lp` makes go under `QS400/LP/`, a response
//! file named by `QS400_INSTALL_RSP` goes to `QS400/INSTALL.RSP`, and
//! the manifest is signed with the build key (docs/INSTALL.md).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use optical::IsoBuilder;
use optical::manifest::{self, Manifest, ManifestFile};
use tracing::info;

use buildkit::host::hash::{sha256_bytes, sha256_file, short, write_sum};
use buildkit::host::paths;
use buildkit::kernel::SOURCE_DATE_EPOCH;
use buildkit::{Ctx, UkiKind};
use diskimg::fat;

/// The media UKI's path inside the boot image and in the ISO tree.
pub const BOOT_FILE: &str = manifest::BOOT_UKI;
/// The El Torito boot image.
pub const BOOT_IMAGE: &str = "EFI.IMG";
/// The media UKI under the LIC directory.
pub const LIC_FILE: &str = "QS400/LIC/qs400.efi";
/// Slot A's UKI, which the installer copies onto the disk.
pub const SLOT_A_FILE: &str = manifest::SLOT_A;
/// Slot B's UKI.
pub const SLOT_B_FILE: &str = manifest::SLOT_B;
/// `slic`, which the media UKI's loader runs.
pub const SLIC_FILE: &str = instmedia::licfile::SLIC;
/// The job program the loader puts beside it.
pub const JOB_FILE: &str = instmedia::licfile::QSJOB;
/// The text file at the root.
pub const README_FILE: &str = "QS400.TXT";
/// The db key's files, also in the boot image under `EFI/keys/`.
pub const KEY_FILES: [&str; 2] = ["qs400-db.cer", "qs400-db.esl"];

/// What goes on the media.
pub struct Parts<'a> {
    /// Volume identifier.
    pub volume_id: &'a str,
    /// Release, `V1R0M0`.
    pub release: &'a str,
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
    /// `slic`, which the media UKI's loader runs.
    pub slic: Vec<u8>,
    /// The job program.
    pub qsjob: Vec<u8>,
    /// The readme.
    pub readme: String,
    /// Product loads: ID and SAVLICPGM stream.
    pub lp: Vec<(String, Vec<u8>)>,
    /// A response file for an automatic install.
    pub rsp: Option<String>,
    /// The key the manifest is signed with.
    pub key: Option<ed25519_dalek::SigningKey>,
    /// Other files: the Knowledge Center (`QS400/KC/`), the PTF
    /// baseline (`QS400/PTF/`). Not in the install manifest.
    pub extra: Vec<(String, Vec<u8>)>,
}

/// The image's tree and options.
pub fn compose(p: Parts<'_>) -> Result<IsoBuilder> {
    let mut b = IsoBuilder::new(p.volume_id, p.epoch);
    // The marker the media locator looks for, with the payload.
    let mut files: Vec<ManifestFile> = [
        (SLOT_A_FILE, &p.slot_a),
        (SLOT_B_FILE, &p.slot_b),
        (BOOT_FILE, &p.uki),
        (SLIC_FILE, &p.slic),
        (JOB_FILE, &p.qsjob),
    ]
    .map(|(path, data)| ManifestFile {
        path: path.to_string(),
        size: data.len() as u64,
        sha256: sha256_bytes(data),
    })
    .to_vec();
    for (id, data) in &p.lp {
        files.push(ManifestFile {
            path: instmedia::Payload::Product(id.clone()).path(),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
        });
    }
    let mut m = Manifest {
        release: p.release.to_string(),
        volume: p.volume_id.to_string(),
        kind: manifest::KIND_INSTALL.into(),
        files,
        signature: None,
    };
    if let Some(k) = &p.key {
        instmedia::sign::sign(&mut m, k);
    }
    b.add_file(manifest::PATH, m.to_text().into_bytes())?;
    for (id, data) in p.lp {
        b.add_file(&instmedia::Payload::Product(id).path(), data)?;
    }
    if let Some(r) = p.rsp {
        b.add_file(manifest::RESPONSE, r.into_bytes())?;
    }
    b.add_file(BOOT_IMAGE, p.efi)?;
    b.add_file(BOOT_FILE, p.uki)?;
    b.add_link(LIC_FILE, BOOT_FILE)?;
    b.add_file(SLOT_A_FILE, p.slot_a)?;
    b.add_file(SLOT_B_FILE, p.slot_b)?;
    b.add_file(SLIC_FILE, p.slic)?;
    b.add_file(JOB_FILE, p.qsjob)?;
    b.add_file(README_FILE, p.readme.into_bytes())?;
    for (path, data) in p.extra {
        b.add_file(&path, data)?;
    }
    b.boot_image(BOOT_IMAGE)?;
    // Protective MBR and GPT: a raw copy onto a disk boots under UEFI.
    b.hybrid(true);
    Ok(b)
}

/// Where the Knowledge Center (`build kc`) and the PTF baseline (`build
/// release`) are written, and the directories they take on the media.
pub const EXTRA_TREES: &[(&str, &str)] = &[
    ("kc", "QS400/KC"),
    ("ptf-baseline", "QS400/PTF"),
    ("media-root", ""),
];

/// Every file of the extra trees that exist, with its path on the media.
fn extra_files(ctx: &Ctx) -> Result<Vec<(String, Vec<u8>)>> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, Vec<u8>)>) -> Result<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for p in entries {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let r = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            if p.is_dir() {
                walk(&p, &r, out)?;
            } else {
                out.push((r, fs::read(&p)?));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for (sub, on_media) in EXTRA_TREES {
        let dir = ctx.paths.images().join(sub);
        if dir.is_dir() {
            walk(&dir, on_media, &mut out)?;
            info!(
                "iso: {} from {}",
                if on_media.is_empty() {
                    "the root"
                } else {
                    on_media
                },
                dir.display()
            );
        }
    }
    Ok(out)
}

/// The El Torito boot image: the media UKI, and a release's db key
/// beside it where the firmware's enrol-from-file finds it (`build
/// release` puts the key in `images/media-root`).
fn boot_image(ctx: &Ctx, uki_bytes: &[u8]) -> Result<Vec<u8>> {
    let spec = fat::media_esp(uki_bytes.len() as u64);
    // El Torito names the boot image's size in 512-byte sectors, 16 bits:
    // a larger image boots only on firmware that ignores the field.
    ensure!(
        spec.bytes <= 65_535 * 512,
        "the media UKI ({} bytes) needs a {} MiB boot image; El Torito holds at most 32 MiB",
        uki_bytes.len(),
        spec.bytes / (1024 * 1024)
    );
    info!(
        "iso: building {} MiB FAT16 boot image",
        spec.bytes / (1024 * 1024)
    );
    let keys_dir = ctx
        .paths
        .images()
        .join("media-root")
        .join(crate::image::release::KEYS);
    let mut keys = Vec::new();
    for k in KEY_FILES {
        if let Ok(b) = fs::read(keys_dir.join(k)) {
            keys.push((format!("EFI/keys/{k}"), b));
        }
    }
    let mut fat_files: Vec<(&str, &[u8])> = vec![(BOOT_FILE, uki_bytes)];
    fat_files.extend(keys.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
    let efi = fat::build(&spec, &fat_files)?;
    let listing = fat::list(&efi)?;
    let mut want = vec!["EFI/".to_string(), "EFI/BOOT/".into(), BOOT_FILE.into()];
    if !keys.is_empty() {
        want.push("EFI/keys/".into());
        want.extend(keys.iter().map(|(p, _)| p.clone()));
    }
    want.sort();
    ensure!(
        listing == want,
        "boot image contents unexpected: {listing:?}"
    );
    Ok(efi)
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

    // The media UKI's loader takes both from the media.
    let slic = fs::read(ctx.paths.bin("slic")).context("no slic; run `userland` first")?;
    let qsjob = fs::read(ctx.paths.bin("qsjob")).context("no qsjob; run `userland` first")?;

    let efi = boot_image(ctx, &uki_bytes)?;

    let release = release::RELEASE.to_string();
    let readme = format!(
        "QS/400 {}\nUEFI installation media. Boots the system from this volume\n(IPL type D). Write to a USB stick with dd; see docs/PHASE1.md.\n",
        release::full()
    );
    let epoch: u64 = SOURCE_DATE_EPOCH.parse()?;
    let uki_check = uki_bytes.clone();
    // The product loads, built now so they carry this build's release.
    let lp_dir = ctx.paths.images().join("lp");
    crate::image::lp::lp(ctx)?;
    let mut lp = Vec::new();
    let mut names: Vec<_> = fs::read_dir(&lp_dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "savf"))
        .collect();
    names.sort();
    for path in names {
        let id = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_uppercase())
            .unwrap_or_default();
        lp.push((id, fs::read(&path)?));
    }
    let rsp = match std::env::var_os("QS400_INSTALL_RSP") {
        Some(p) => {
            let text = fs::read_to_string(&p)
                .with_context(|| format!("QS400_INSTALL_RSP={}", PathBuf::from(&p).display()))?;
            instmedia::Response::parse(&text).context("the response file")?;
            info!("iso: response file {}", PathBuf::from(&p).display());
            Some(text)
        }
        None => None,
    };
    let key = buildkit::image::manifest::signing_key(ctx)?;
    let volume_id = format!("QS400_{release}");
    let b = compose(Parts {
        volume_id: &volume_id,
        release: &release,
        epoch,
        efi,
        uki: uki_bytes,
        slot_a,
        slot_b,
        slic,
        qsjob,
        readme,
        lp,
        rsp,
        key: Some(key),
        extra: extra_files(ctx)?,
    })?;
    let image = b.build()?;
    let checked = validate(&image, &uki_check)?;
    info!("iso: {checked}");

    let out = ctx
        .paths
        .images()
        .join(format!("QS400-{release}-{}.iso", short(&hash)));
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
/// image, the boot image holds the UKI, the tree holds the same bytes,
/// partition 1 is exactly the boot image, and the MBR and both GPTs
/// check out. Returns one line for the log.
pub fn validate(image: &[u8], uki: &[u8]) -> Result<String> {
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
    for path in [BOOT_FILE, LIC_FILE] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use optical::IsoImage;

    fn parts() -> Parts<'static> {
        Parts {
            volume_id: "QS400_V1R0M0",
            release: "V1R0M0",
            epoch: 1_767_225_600,
            efi: fat::build(&fat::MEDIA_ESP, &[(BOOT_FILE, b"uki")]).unwrap(),
            uki: b"uki".to_vec(),
            slot_a: b"slot a".to_vec(),
            slot_b: b"slot b".to_vec(),
            slic: b"slic".to_vec(),
            qsjob: b"job program".to_vec(),
            readme: "readme\n".into(),
            lp: vec![("QS00SS1".into(), b"os".to_vec())],
            rsp: Some("language = 2924\n".into()),
            key: Some(ed25519_dalek::SigningKey::from_bytes(&[5; 32])),
            extra: vec![("QS400/KC/index.html".into(), b"<html>".to_vec())],
        }
    }

    #[test]
    fn the_tree_carries_the_loose_efi_files() {
        let iso = compose(parts()).unwrap().build().unwrap();
        validate(&iso, b"uki").unwrap();
        assert!(validate(&iso, b"other").is_err());
        let mut bad = iso.clone();
        bad[1024 + 40] ^= 1;
        assert!(validate(&bad, b"uki").is_err());
        assert_eq!(compose(parts()).unwrap().build().unwrap(), iso);
        let img = IsoImage::open(iso.as_slice()).unwrap();
        assert!(img.info().joliet && img.info().rock_ridge);
        for (path, want) in [
            (BOOT_FILE, &b"uki"[..]),
            (LIC_FILE, b"uki"),
            (SLOT_A_FILE, b"slot a"),
            (SLOT_B_FILE, b"slot b"),
            (SLIC_FILE, b"slic"),
            (JOB_FILE, b"job program"),
        ] {
            let e = img.lookup(path).unwrap();
            assert_eq!(img.read_file(&e).unwrap(), want, "{path}");
        }
        let m = img.read_file(&img.lookup(manifest::PATH).unwrap()).unwrap();
        let m = Manifest::parse(std::str::from_utf8(&m).unwrap()).unwrap();
        assert_eq!(m.file(SLOT_A_FILE).unwrap().size, 6);
        assert_eq!(m.release, "V1R0M0");
        let public = ed25519_dalek::SigningKey::from_bytes(&[5; 32])
            .verifying_key()
            .to_bytes();
        instmedia::sign::verify(&m, &public).unwrap();
        instmedia::licfile::check_manifest(&m, &public, "V1R0M0").unwrap();
        instmedia::licfile::check_file(&m, SLIC_FILE, b"slic").unwrap();
        instmedia::licfile::check_file(&m, JOB_FILE, b"job program").unwrap();
        let c = instmedia::Contents::of(m.clone()).unwrap();
        assert_eq!(c.products(), vec!["QS00SS1".to_string()]);
        let lp = img.lookup("QS400/LP/QS00SS1.SAVF").unwrap();
        assert_eq!(img.read_file(&lp).unwrap(), b"os");
        let r = img
            .read_file(&img.lookup(manifest::RESPONSE).unwrap())
            .unwrap();
        assert_eq!(r, b"language = 2924\n");
        let boot = img.info().boot.unwrap();
        assert_eq!(img.lookup(BOOT_IMAGE).unwrap().extents[0].0, boot.lba);
        let gpt = optical::iso::hybrid::verify(&img.source().to_vec()).unwrap();
        let esp = img.lookup(BOOT_IMAGE).unwrap();
        assert_eq!(gpt.partitions[0].first_lba, u64::from(boot.lba) * 4);
        assert_eq!(
            gpt.partitions[0].last_lba,
            u64::from(boot.lba) * 4 + esp.size / 512 - 1
        );
        assert_eq!(
            gpt.partitions[1].type_guid,
            optical::iso::hybrid::QUARTZ_MEDIA
        );
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
