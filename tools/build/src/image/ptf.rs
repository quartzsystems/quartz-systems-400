// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build ptf` and `build ptf-catalog` (`docs/PTF.md` §8). `build ptf
//! MANIFEST` packages a PTF from a manifest in the tree
//! (`ptf/fixes/QSnnnnn.toml`) into `target/images/ptf/QSnnnnn.savf`
//! (its cover letter beside it, `.txt`), signed with the build key; `lic = "*BUILD"` takes this build's slot
//! UKIs, signed for Secure Boot first. `build ptf-catalog DIR` publishes
//! every PTF under `DIR/ptf/` as the update service's directory: the
//! signed catalog one sequence past the last, and a cumulative package
//! listing them all.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use tracing::{info, warn};

use buildkit::Ctx;
use buildkit::UkiKind;
use buildkit::host::hash::sha256_bytes;
use buildkit::host::paths;

/// The manifest's `lic` value that takes this build's UKIs.
pub const FROM_BUILD: &str = "*BUILD";

/// The PTF's creation time: `SOURCE_DATE_EPOCH` when set (a rebuild of
/// the same commit gives the same bytes), else now.
fn now() -> Result<u64> {
    if let Ok(v) = std::env::var("SOURCE_DATE_EPOCH") {
        return v
            .trim()
            .parse()
            .context("SOURCE_DATE_EPOCH is not a number");
    }
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

/// A path in the manifest, relative to the manifest's directory.
fn beside(manifest: &Path, p: &str) -> PathBuf {
    let p = Path::new(p);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        manifest.parent().unwrap_or(Path::new(".")).join(p)
    }
}

/// This build's slot UKIs, signed with the Secure Boot db key.
fn build_ukis(ctx: &Ctx) -> Result<(Vec<u8>, Vec<u8>)> {
    let key = pesign::DbKey::from_env()?;
    if key.development {
        warn!(
            "ptf: signing the UKIs with the DEVELOPMENT Secure Boot key (set {} and {})",
            pesign::keys::KEY_ENV,
            pesign::keys::CERT_ENV
        );
    }
    let mut out = Vec::new();
    for kind in [UkiKind::SlotA, UkiKind::SlotB] {
        let p = paths::read_pointer(&ctx.paths.uki_pointer(kind))
            .context("no UKI from this build; run `build all --release` first")?;
        let signed = pesign::sign(&fs::read(&p)?, &key)?;
        info!("ptf: UKI {kind:?} {} ({} bytes)", p.display(), signed.len());
        out.push(signed);
    }
    let b = out.pop().context("slot B")?;
    let a = out.pop().context("slot A")?;
    Ok((a, b))
}

/// `build ptf MANIFEST`: the PTF's save file. Its path.
pub fn ptf(ctx: &Ctx, manifest: &Path) -> Result<PathBuf> {
    let text = fs::read_to_string(manifest)
        .with_context(|| format!("cannot read {}", manifest.display()))?;
    let m = ptf::manifest::Manifest::parse(&text).map_err(|e| anyhow::anyhow!("{e}"))?;
    let cover = if m.cover_file.is_empty() {
        m.cover.clone()
    } else {
        fs::read_to_string(beside(manifest, &m.cover_file))
            .with_context(|| format!("cover_file {}", m.cover_file))?
    };
    if cover.trim().is_empty() {
        bail!("{}: a PTF needs a cover letter", manifest.display());
    }
    let (lic, lic_b) = if m.lic == FROM_BUILD {
        if !m.lic_b.is_empty() {
            bail!("lic = \"{FROM_BUILD}\" takes slot B's UKI too; drop lic_b");
        }
        let (a, b) = build_ukis(ctx)?;
        (Some(a), Some(b))
    } else {
        let read = |p: &str| -> Result<Option<Vec<u8>>> {
            if p.is_empty() {
                return Ok(None);
            }
            let path = beside(manifest, p);
            Ok(Some(fs::read(&path).with_context(|| {
                format!("cannot read {}", path.display())
            })?))
        };
        (read(&m.lic)?, read(&m.lic_b)?)
    };
    if lic.is_some() && m.kind != "delayed" {
        warn!("ptf: a LIC PTF takes effect at the next IPL whatever its kind says");
    }
    let key = buildkit::image::manifest::signing_key(ctx)?;
    let built = ptfbuild::build(&ptfbuild::Input {
        manifest: &m,
        cover: cover.clone(),
        lic: lic.as_deref(),
        lic_b: lic_b.as_deref(),
        key: &key,
        now: now()?,
    })?;
    // What a system trusting this key will load.
    ptf::package::open(&built.stream, &[key.verifying_key().to_bytes()])
        .map_err(|e| anyhow::anyhow!("the PTF does not open: {e}"))?;
    let out = ctx.paths.images().join("ptf");
    fs::create_dir_all(&out)?;
    let path = out.join(format!("{}.savf", built.ptf));
    fs::write(&path, &built.stream)?;
    fs::write(out.join(format!("{}.txt", built.ptf)), cover.trim_start())?;
    info!(
        "ptf: {} ({} bytes{}, sha256 {})",
        path.display(),
        built.stream.len(),
        if lic.is_some() { ", LIC" } else { "" },
        sha256_bytes(&built.stream)
    );
    Ok(path)
}

/// `build ptf-catalog DIR`: the update service's directory over the
/// PTFs in `DIR/ptf/`. `hiper` names the high-impact ones.
pub fn catalog(ctx: &Ctx, dir: &Path, hiper: &[String]) -> Result<()> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir.join("ptf"))
        .with_context(|| format!("cannot read {}", dir.join("ptf").display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "savf"))
        .collect();
    files.sort();
    let key = buildkit::image::manifest::signing_key(ctx)?;
    let trusted = [key.verifying_key().to_bytes()];
    let mut streams = Vec::new();
    let mut ids = Vec::new();
    for f in &files {
        let bytes = fs::read(f)?;
        let p = ptf::package::open(&bytes, &trusted)
            .map_err(|e| anyhow::anyhow!("{}: {e}", f.display()))?;
        if p.header.release != release::RELEASE.to_string() {
            bail!(
                "{}: release {}, this build is {}",
                f.display(),
                p.header.release,
                release::RELEASE
            );
        }
        ids.push(p.header.ptf.clone());
        streams.push(bytes);
    }
    if streams.is_empty() {
        bail!("no PTFs under {}", dir.join("ptf").display());
    }
    // One past the catalog already there (the release's baseline is 1);
    // its HIPER marks stay, and a package republished the same day goes
    // up a level so systems that have it see the change.
    let prev = match fs::read_to_string(dir.join("catalog.toml")) {
        Ok(t) => Some(
            ptf_update::catalog::Catalog::parse(&t, &trusted)
                .map_err(|e| anyhow::anyhow!("the catalog there: {e}"))?,
        ),
        Err(_) => None,
    };
    let sequence = prev.as_ref().map_or(2, |c| c.sequence + 1);
    let mut hiper = hiper.to_vec();
    if let Some(c) = &prev {
        hiper.extend(c.ptfs.iter().filter(|p| p.hiper).map(|p| p.id.clone()));
    }
    // The real time: the package is named for the day it is published.
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let (year, day) = crate::image::release::year_day(now);
    let name = release::Cumulative { year, day }.to_string();
    let level = prev
        .as_ref()
        .and_then(|c| c.group(&name))
        .map_or(1, |g| g.level + 1);
    let groups = [ptf_update::publish::GroupDef {
        name: name.clone(),
        kind: ptf_update::catalog::GroupKind::Cumulative,
        level,
        release: release::RELEASE.to_string(),
        text: format!(
            "Cumulative PTF package of {}: {}",
            release::RELEASE,
            ids.join(", ")
        ),
        ptfs: ids.clone(),
    }];
    let cat = ptf_update::publish::publish(
        dir,
        &ptf_update::publish::Publication {
            ptfs: &streams,
            hiper: &hiper,
            groups: &groups,
            sequence,
            now,
        },
        &key,
    )?;
    info!(
        "ptf-catalog: {} sequence {}, {} PTF(s), cumulative package {name} level {level}",
        dir.join("catalog.toml").display(),
        cat.sequence,
        ids.len()
    );
    Ok(())
}
