// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The initramfs cpios: one for the slot UKIs and, when the product
//! builds a media UKI, one for it. What goes in is the product's
//! (`Product::initramfs`); [`plain`] is the least a system boots with.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::sha256_bytes;
use crate::image::cpio::{Entry, write_newc};
use crate::kernel::UkiKind;

/// `build initramfs`: every cpio the product's UKIs embed. Returns the
/// slot UKIs'.
pub fn initramfs(ctx: &Ctx) -> Result<PathBuf> {
    let out = ctx.paths.initramfs();
    fs::create_dir_all(out.parent().context("no parent")?)?;
    let mut written: Vec<PathBuf> = Vec::new();
    for &kind in ctx.product.kinds {
        let path = ctx.paths.initramfs_of(kind);
        if written.contains(&path) {
            continue;
        }
        let bytes = write_newc((ctx.product.initramfs)(ctx, kind)?);
        fs::write(&path, &bytes).with_context(|| format!("cannot write {}", path.display()))?;
        info!(
            "initramfs: {} ({} bytes, sha256 {})",
            path.display(),
            bytes.len(),
            sha256_bytes(&bytes)
        );
        written.push(path);
    }
    Ok(out)
}

/// The built binary of userland package `name`.
pub fn binary(ctx: &Ctx, name: &str) -> Result<Vec<u8>> {
    let path = ctx.paths.bin(name);
    ensure!(
        path.is_file(),
        "missing {}; run `userland` first",
        path.display()
    );
    fs::read(&path).with_context(|| format!("cannot read {}", path.display()))
}

/// A plain initramfs: the product's init package as `/init`, the mount
/// points and a console node. The kernel does not auto-mount devtmpfs
/// for an initramfs, so a static console node lets it open the initial
/// console before `/init` runs.
pub fn plain(ctx: &Ctx, _kind: UkiKind) -> Result<Vec<Entry>> {
    Ok(vec![
        Entry::dir("dev"),
        Entry::chardev("dev/console", 5, 1),
        Entry::file("init", binary(ctx, ctx.product.init)?),
        Entry::dir("proc"),
        Entry::dir("run"),
        Entry::dir("sys"),
    ])
}
