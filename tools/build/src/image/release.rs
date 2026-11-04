// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build kc` and `build release`. The release is the whole pipeline
//! without the service console, the Knowledge Center and the PTF baseline
//! (a cumulative package `Cyyddd110` with no PTFs) on the media, and
//! `target/release/<VRM>/`: the ISO, the slot UKIs, the baseline,
//! `SHA256SUMS` and its detached ed25519 signature `SHA256SUMS.sig`.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ed25519_dalek::Signer;
use tracing::info;

use crate::ctx::Ctx;
use crate::host::paths;
use crate::kernel::{SOURCE_DATE_EPOCH, UkiKind};

/// `build kc`: the Knowledge Center into `images/kc` (the ISO takes it).
pub fn kc(ctx: &Ctx) -> Result<PathBuf> {
    let out = ctx.paths.images().join("kc");
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("cannot clear {}", out.display()))?;
    }
    let s = kcbuild::build(&ctx.paths.repo, &out)?;
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
fn year_day(t: u64) -> (u8, u16) {
    let mut days = t / 86_400;
    let mut year = 1970u64;
    loop {
        let leap = (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
        let n = if leap { 366 } else { 365 };
        if days < n {
            break;
        }
        days -= n;
        year += 1;
    }
    (u8::try_from(year % 100).unwrap_or(0), u16::try_from(days + 1).unwrap_or(1))
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
        text: format!("Cumulative PTF package of {}: the baseline, no PTFs", release::RELEASE),
        ptfs: Vec::new(),
    }];
    let key = crate::image::manifest::signing_key(ctx)?;
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
    let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    fs::copy(from, to_dir.join(&name)).with_context(|| format!("cannot copy {}", from.display()))?;
    Ok(name)
}

/// `build release`.
pub fn release(ctx: &Ctx) -> Result<PathBuf> {
    let vrm = release::RELEASE.to_string();
    info!("release: {}", release::full());
    crate::kernel::fetch::fetch(ctx)?;
    crate::kernel::kernel(ctx)?;
    crate::userland::userland(ctx)?;
    crate::image::initramfs::initramfs(ctx)?;
    crate::kernel::uki(ctx, true)?;
    crate::image::disk::disk(ctx, None)?;
    kc(ctx)?;
    let baseline = ptf_baseline(ctx)?;
    let iso = crate::image::media::iso(ctx)?;
    crate::image::manifest::manifest(ctx)?;

    let out = ctx.paths.target.join("release").join(&vrm);
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    fs::create_dir_all(&out)?;
    let mut files = vec![copy(&iso, &out)?];
    for kind in [UkiKind::SlotA, UkiKind::SlotB, UkiKind::Media] {
        files.push(copy(&paths::read_pointer(&ctx.paths.uki_pointer(kind))?, &out)?);
    }
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
        let sum = crate::host::hash::sha256_file(&out.join(f))?;
        let _ = writeln!(sums, "{sum}  {f}");
    }
    fs::write(out.join("SHA256SUMS"), &sums)?;
    let key = crate::image::manifest::signing_key(ctx)?;
    let sig = key.sign(sums.as_bytes());
    fs::write(
        out.join("SHA256SUMS.sig"),
        format!(
            "ed25519 {}\nkey {}\n",
            hex::encode(sig.to_bytes()),
            hex::encode(key.verifying_key().to_bytes())
        ),
    )?;
    info!("release: {} with SHA256SUMS and its signature", out.display());
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
