// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build lp`: every product of `licpgm/products/` packaged into a
//! signed product load, `target/images/lp/<ID>.savf` (the bytes a save
//! file holds for `RSTLICPGM DEV(*SAVF)`). The media layout puts them
//! on the ISO later.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::sha256_bytes;
use crate::kernel::SOURCE_DATE_EPOCH;

/// `build lp`.
pub fn lp(ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let key = crate::image::manifest::signing_key(ctx)?;
    let now: u64 = SOURCE_DATE_EPOCH.parse()?;
    let src = ctx.paths.repo.join("licpgm/products");
    let mut manifests: Vec<PathBuf> = fs::read_dir(&src)
        .with_context(|| format!("cannot read {}", src.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    manifests.sort();
    let out = ctx.paths.images().join("lp");
    fs::create_dir_all(&out)?;
    let mut made = Vec::new();
    for m in manifests {
        let text = fs::read_to_string(&m).with_context(|| format!("cannot read {}", m.display()))?;
        let b = lpbuild::build(&text, &key, now).with_context(|| format!("packaging {}", m.display()))?;
        let path = out.join(format!("{}.savf", b.product));
        fs::write(&path, &b.stream).with_context(|| format!("cannot write {}", path.display()))?;
        info!(
            "lp: {} ({} bytes, {} objects signed, sha256 {})",
            path.display(),
            b.stream.len(),
            b.signed,
            sha256_bytes(&b.stream)
        );
        made.push(path);
    }
    Ok(made)
}
