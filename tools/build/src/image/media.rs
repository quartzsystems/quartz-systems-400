// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What QS/400 puts on its installation media (`build iso`; the image
//! itself is `buildkit::image::iso`'s). The media UKI is also at
//! `QS400/LIC/qs400.efi`. Its initramfs holds only the loader, to fit
//! the boot image whatever the system grows to; `slic` and the job
//! program are `QS400/LIC/SLIC` and `QS400/LIC/QSJOB`, in the signed
//! manifest. The product loads `build lp` makes go under `QS400/LP/`, a
//! response file named by `QS400_INSTALL_RSP` goes to
//! `QS400/INSTALL.RSP`, and the manifest is signed with the build key
//! (docs/INSTALL.md).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use buildkit::host::hash::sha256_bytes;
use buildkit::host::paths;
use buildkit::image::iso::{Item, Media};
use buildkit::{Ctx, UkiKind};
use optical::manifest::{self, Manifest, ManifestFile};
use tracing::info;

/// The media UKI's path in the ISO tree.
pub const BOOT_FILE: &str = manifest::BOOT_UKI;
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
    /// The media UKI.
    pub uki: &'a [u8],
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

/// The media's tree, in the order it is written.
pub fn tree(p: Parts<'_>) -> Vec<Item> {
    // The marker the media locator looks for, with the payload.
    let mut files: Vec<ManifestFile> = [
        (SLOT_A_FILE, p.slot_a.as_slice()),
        (SLOT_B_FILE, p.slot_b.as_slice()),
        (BOOT_FILE, p.uki),
        (SLIC_FILE, p.slic.as_slice()),
        (JOB_FILE, p.qsjob.as_slice()),
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
    let mut t = vec![Item::File(manifest::PATH.into(), m.to_text().into_bytes())];
    for (id, data) in p.lp {
        t.push(Item::File(instmedia::Payload::Product(id).path(), data));
    }
    if let Some(r) = p.rsp {
        t.push(Item::File(manifest::RESPONSE.into(), r.into_bytes()));
    }
    t.push(Item::Boot);
    t.push(Item::Link(LIC_FILE.into(), BOOT_FILE.into()));
    t.push(Item::File(SLOT_A_FILE.into(), p.slot_a));
    t.push(Item::File(SLOT_B_FILE.into(), p.slot_b));
    t.push(Item::File(SLIC_FILE.into(), p.slic));
    t.push(Item::File(JOB_FILE.into(), p.qsjob));
    t.push(Item::File(README_FILE.into(), p.readme.into_bytes()));
    for (path, data) in p.extra {
        t.push(Item::File(path, data));
    }
    t
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

/// A release's db key, which goes in the boot image where the
/// firmware's enrol-from-file finds it (`build release` puts the key in
/// `images/media-root`).
fn boot_keys(ctx: &Ctx) -> Vec<(String, Vec<u8>)> {
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
    keys
}

/// The media around the media UKI `uki`.
pub fn media(ctx: &Ctx, uki: &[u8]) -> Result<Media> {
    // The installer copies the slot UKIs onto the disk's ESP.
    let slot_a = fs::read(paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotA))?)?;
    let slot_b = fs::read(paths::read_pointer(&ctx.paths.uki_pointer(UkiKind::SlotB))?)?;
    // The media UKI's loader takes both from the media.
    let slic = fs::read(ctx.paths.bin("slic")).context("no slic; run `userland` first")?;
    let qsjob = fs::read(ctx.paths.bin("qsjob")).context("no qsjob; run `userland` first")?;

    let release = release::RELEASE.to_string();
    let readme = format!(
        "QS/400 {}\nUEFI installation media. Boots the system from this volume\n(IPL type D). Write to a USB stick with dd; see docs/PHASE1.md.\n",
        release::full()
    );
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
    let tree = tree(Parts {
        volume_id: &volume_id,
        release: &release,
        uki,
        slot_a,
        slot_b,
        slic,
        qsjob,
        readme,
        lp,
        rsp,
        key: Some(key),
        extra: extra_files(ctx)?,
    });
    Ok(Media {
        volume_id,
        tree,
        boot_files: boot_keys(ctx),
        uki_copies: vec![LIC_FILE.into()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use buildkit::image::iso::{BOOT_IMAGE, compose, validate};
    use diskimg::fat;
    use optical::IsoImage;

    fn build() -> Vec<u8> {
        let tree = tree(Parts {
            volume_id: "QS400_V1R0M0",
            release: "V1R0M0",
            uki: b"uki",
            slot_a: b"slot a".to_vec(),
            slot_b: b"slot b".to_vec(),
            slic: b"slic".to_vec(),
            qsjob: b"job program".to_vec(),
            readme: "readme\n".into(),
            lp: vec![("QS00SS1".into(), b"os".to_vec())],
            rsp: Some("language = 2924\n".into()),
            key: Some(ed25519_dalek::SigningKey::from_bytes(&[5; 32])),
            extra: vec![("QS400/KC/index.html".into(), b"<html>".to_vec())],
        });
        let efi = fat::build(&fat::MEDIA_ESP, &[(BOOT_FILE, b"uki")]).unwrap();
        compose("QS400_V1R0M0", 1_767_225_600, efi, b"uki".to_vec(), tree)
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn the_tree_carries_the_loose_efi_files() {
        let iso = build();
        validate(&iso, b"uki", &[LIC_FILE]).unwrap();
        assert!(validate(&iso, b"other", &[LIC_FILE]).is_err());
        let mut bad = iso.clone();
        bad[1024 + 40] ^= 1;
        assert!(validate(&bad, b"uki", &[LIC_FILE]).is_err());
        assert_eq!(build(), iso);
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
