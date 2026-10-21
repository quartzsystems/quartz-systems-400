// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The initramfs: `/init` (slic, which is also the storage service),
//! `/qsjob` (the job process), mount points and a console node.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::hash::sha256_bytes;
use crate::image::cpio::{Entry, write_newc};

/// `build initramfs`.
pub fn initramfs(ctx: &Ctx) -> Result<PathBuf> {
    let slic = ctx.paths.slic();
    ensure!(
        slic.is_file(),
        "missing {}; run `userland` first",
        slic.display()
    );
    let init = fs::read(&slic).with_context(|| format!("cannot read {}", slic.display()))?;
    let qsjob = ctx.paths.qsjob();
    let job = fs::read(&qsjob).with_context(|| format!("cannot read {}", qsjob.display()))?;

    // The kernel does not auto-mount devtmpfs for an initramfs, so a static
    // console node lets it open the initial console before slic runs.
    let entries = vec![
        Entry::dir("dev"),
        Entry::chardev("dev/console", 5, 1),
        Entry::file("init", init),
        Entry::dir("proc"),
        Entry::file("qsjob", job),
        Entry::dir("run"),
        Entry::dir("sys"),
    ];
    let bytes = write_newc(entries);

    let out = ctx.paths.initramfs();
    fs::create_dir_all(out.parent().context("no parent")?)?;
    fs::write(&out, &bytes).with_context(|| format!("cannot write {}", out.display()))?;
    info!(
        "initramfs: {} ({} bytes, sha256 {})",
        out.display(),
        bytes.len(),
        sha256_bytes(&bytes)
    );
    Ok(out)
}
