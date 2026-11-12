// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build kc` and `build release`. The release is the whole pipeline
//! without the service console, the Knowledge Center and the PTF baseline
//! (a cumulative package `Cyyddd110` with no PTFs) on the media, and
//! `target/release/<VRM>/`: the ISO, the slot UKIs (signed for Secure
//! Boot), the baseline, `NOTICES.txt`, `SOURCE-OFFER.txt`, `MANIFEST.json`,
//! the db certificate, `SHA256SUMS` and its detached ed25519 signature
//! `SHA256SUMS.sig`. The media also carry the notices, the release notes,
//! the kernel's source offer (`QS400/SRC/`) and the db key
//! (`QS400/EFI/keys/`).

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ed25519_dalek::Signer;
use tracing::{info, warn};

use buildkit::Ctx;
use buildkit::UkiKind;
use buildkit::host::paths;
use buildkit::kernel::SOURCE_DATE_EPOCH;

/// `build kc`: the Knowledge Center into `images/kc` (the ISO takes it).
pub fn kc(ctx: &Ctx) -> Result<PathBuf> {
    let out = ctx.paths.images().join("kc");
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("cannot clear {}", out.display()))?;
    }
    let s = kcbuild::build_checked(&ctx.paths.repo, &out, kcbuild::notes::legacy_from_env())?;
    info!(
        "kc: {} pages ({} commands, {} messages, {} documents) in {}",
        s.pages,
        s.commands,
        s.messages,
        s.docs,
        out.display()
    );
    Ok(out)
}

/// Year (two digits) and day of the year of Unix time `t`.
pub fn year_day(t: u64) -> (u8, u16) {
    let mut days = t / 86_400;
    let mut year = 1970u64;
    loop {
        let leap =
            (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
        let n = if leap { 366 } else { 365 };
        if days < n {
            break;
        }
        days -= n;
        year += 1;
    }
    (
        u8::try_from(year % 100).unwrap_or(0),
        u16::try_from(days + 1).unwrap_or(1),
    )
}

/// The PTF baseline into `images/ptf-baseline`: a published catalog with
/// one cumulative package of no PTFs at this release. Its name.
pub fn ptf_baseline(ctx: &Ctx) -> Result<String> {
    let epoch: u64 = SOURCE_DATE_EPOCH.parse()?;
    let (year, day) = year_day(epoch);
    let name = release::Cumulative { year, day }.to_string();
    let dir = ctx.paths.images().join("ptf-baseline");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    let groups = [ptf_update::publish::GroupDef {
        name: name.clone(),
        kind: ptf_update::catalog::GroupKind::Cumulative,
        level: 1,
        release: release::RELEASE.to_string(),
        text: format!(
            "Cumulative PTF package of {}: the baseline, no PTFs",
            release::RELEASE
        ),
        ptfs: Vec::new(),
    }];
    let key = buildkit::image::manifest::signing_key(ctx)?;
    ptf_update::publish::publish(
        &dir,
        &ptf_update::publish::Publication {
            ptfs: &[],
            hiper: &[],
            groups: &groups,
            sequence: 1,
            now: epoch,
        },
        &key,
    )?;
    info!("release: PTF baseline {name} in {}", dir.display());
    Ok(name)
}

fn copy(from: &Path, to_dir: &Path) -> Result<String> {
    let name = from
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    fs::copy(from, to_dir.join(&name))
        .with_context(|| format!("cannot copy {}", from.display()))?;
    Ok(name)
}

/// Sign the three UKIs in place with the Secure Boot db key
/// (`QS400_SB_KEY`/`QS400_SB_CERT`, else the development pair).
pub fn sign_ukis(ctx: &Ctx) -> Result<pesign::DbKey> {
    let key = pesign::DbKey::from_env()?;
    if key.development {
        warn!(
            "release: signing the UKIs with the DEVELOPMENT Secure Boot key (set {} and {})",
            pesign::keys::KEY_ENV,
            pesign::keys::CERT_ENV
        );
    }
    for kind in UkiKind::ALL {
        let p = paths::read_pointer(&ctx.paths.uki_pointer(kind))?;
        let signed = pesign::sign(&fs::read(&p)?, &key)?;
        fs::write(&p, &signed)?;
        info!(
            "release: UKI {kind:?} signed, db key SHA-256 {}",
            key.fingerprint()
        );
    }
    Ok(key)
}

/// The files `build release` adds at the media's root and under
/// `QS400/`, into `images/media-root` (the ISO takes the tree): the
/// notices, the release notes, the kernel's source offer and the db
/// key's certificate. The text of `SOURCE-OFFER.txt`.
fn media_root(ctx: &Ctx, key: &pesign::DbKey) -> Result<String> {
    let dir = ctx.paths.images().join("media-root");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    let lf = |p: &Path| -> Result<Vec<u8>> {
        Ok(fs::read_to_string(p)?.replace("\r\n", "\n").into_bytes())
    };
    let mut files = vec![
        (
            "NOTICES.txt".to_string(),
            lf(&ctx.paths.repo.join(crate::product::NOTICES_FILE))?,
        ),
        (
            NOTES.to_string(),
            lf(&ctx.paths.repo.join("docs").join(NOTES))?,
        ),
        (format!("{KEYS}/qs400-db.cer"), key.cert.clone()),
        (
            format!("{KEYS}/qs400-db.esl"),
            pesign::keys::esl::list(&key.cert),
        ),
    ];
    let (src, offer) = crate::image::srcoffer::files(ctx)?;
    files.extend(src);
    for (rel, data) in files {
        let p = dir.join(&rel);
        fs::create_dir_all(p.parent().context("no parent")?)?;
        fs::write(&p, data)?;
    }
    info!(
        "release: notices, release notes, kernel source offer and db certificate in {}",
        dir.display()
    );
    Ok(offer)
}

/// The release notes, at the media's root by this name.
pub const NOTES: &str = "RELEASE_NOTES_V1R1M0.md";
/// The db key's certificate and signature list on the media.
pub const KEYS: &str = "QS400/EFI/keys";

/// `build release`.
pub fn release(ctx: &Ctx) -> Result<PathBuf> {
    let vrm = release::RELEASE.to_string();
    info!("release: {}", release::full());
    // Stale notices or release notes stop the release before it builds.
    buildkit::notices::notices(ctx, true)?;
    let problems = kcbuild::notes::problems(&ctx.paths.repo, kcbuild::notes::legacy_from_env())?;
    if !problems.is_empty() {
        bail!(
            "the release notes are not current (cargo run -p kcbuild -- notes --update):\n  {}",
            problems.join("\n  ")
        );
    }
    buildkit::kernel::fetch::fetch(ctx)?;
    buildkit::kernel::kernel(ctx)?;
    buildkit::userland::userland(ctx)?;
    buildkit::image::initramfs::initramfs(ctx)?;
    buildkit::kernel::uki(ctx, false)?;
    let key = sign_ukis(ctx)?;
    buildkit::image::disk::disk(ctx, None)?;
    kc(ctx)?;
    let baseline = ptf_baseline(ctx)?;
    let offer = media_root(ctx, &key)?;
    let iso = buildkit::image::iso::iso(ctx)?;
    let manifest = buildkit::image::manifest::manifest(ctx)?;

    let out = ctx.paths.target.join("release").join(&vrm);
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;
    let mut files = vec![copy(&iso, &out)?];
    for kind in [UkiKind::SlotA, UkiKind::SlotB, UkiKind::Media] {
        files.push(copy(
            &paths::read_pointer(&ctx.paths.uki_pointer(kind))?,
            &out,
        )?);
    }
    fs::write(out.join("SOURCE-OFFER.txt"), offer)?;
    fs::copy(
        ctx.paths.repo.join(crate::product::NOTICES_FILE),
        out.join("NOTICES.txt"),
    )?;
    fs::copy(&manifest, out.join("MANIFEST.json"))?;
    fs::copy(
        manifest.with_file_name("manifest.sig"),
        out.join("MANIFEST.sig"),
    )?;
    fs::write(out.join("qs400-db.cer"), &key.cert)?;
    files.extend(
        [
            "SOURCE-OFFER.txt",
            "NOTICES.txt",
            "MANIFEST.json",
            "qs400-db.cer",
        ]
        .map(str::to_string),
    );
    let base_dir = out.join("ptf-baseline");
    fs::create_dir_all(base_dir.join("group"))?;
    let src = ctx.paths.images().join("ptf-baseline");
    fs::copy(src.join("catalog.toml"), base_dir.join("catalog.toml"))?;
    fs::copy(
        src.join("group").join(format!("{baseline}.toml")),
        base_dir.join("group").join(format!("{baseline}.toml")),
    )?;
    files.push("ptf-baseline/catalog.toml".into());
    files.push(format!("ptf-baseline/group/{baseline}.toml"));
    files.sort();
    let mut sums = String::new();
    for f in &files {
        let sum = buildkit::host::hash::sha256_file(&out.join(f))?;
        let _ = writeln!(sums, "{sum}  {f}");
    }
    fs::write(out.join("SHA256SUMS"), &sums)?;
    let key = buildkit::image::manifest::signing_key(ctx)?;
    let sig = key.sign(sums.as_bytes());
    fs::write(
        out.join("SHA256SUMS.sig"),
        format!(
            "ed25519 {}\nkey {}\n",
            hex::encode(sig.to_bytes()),
            hex::encode(key.verifying_key().to_bytes())
        ),
    )?;
    info!(
        "release: {} with SHA256SUMS and its signature",
        out.display()
    );
    for line in sums.lines() {
        info!("release: {line}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::year_day;

    #[test]
    fn day_of_year() {
        assert_eq!(year_day(0), (70, 1));
        // 2026-11-04 is day 308.
        assert_eq!(year_day(1_793_750_400), (26, 308));
        // 2024-12-31 is day 366 of a leap year.
        assert_eq!(year_day(1_735_603_200), (24, 366));
    }
}
