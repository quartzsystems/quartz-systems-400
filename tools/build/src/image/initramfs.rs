// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The initramfs: `/init` (slic, which is also the storage service),
//! `/qsjob` (the job process), mount points, a console node and
//! `/qs400/keys/system.pub`: the public keys of the signers the system
//! trusts, the build key first, 32 bytes each; they check signed objects,
//! PTFs and license keys (`docs/LICPGM.md` §7, `docs/PTF.md` §6).

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

    let keys = trusted_keys(ctx)?;

    // The kernel does not auto-mount devtmpfs for an initramfs, so a static
    // console node lets it open the initial console before slic runs.
    let entries = vec![
        Entry::dir("dev"),
        Entry::chardev("dev/console", 5, 1),
        Entry::file("init", init),
        Entry::dir("proc"),
        Entry::dir("qs400"),
        Entry::dir("qs400/keys"),
        Entry::file("qs400/keys/system.pub", keys),
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

/// Third-party signers the image trusts: a file of ed25519 public keys,
/// 64 hex digits each, one per line (`#` comments).
const TRUSTED_ENV: &str = "QS400_TRUSTED_KEYS";

/// `/qs400/keys/system.pub` (docs/PTF.md section 6): the build key's
/// public key, then each key `QS400_TRUSTED_KEYS` lists; 32 bytes each.
fn trusted_keys(ctx: &Ctx) -> Result<Vec<u8>> {
    let mut out = crate::image::manifest::signing_key(ctx)?
        .verifying_key()
        .to_bytes()
        .to_vec();
    if let Some(path) = std::env::var_os(TRUSTED_ENV) {
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read {TRUSTED_ENV}={}", PathBuf::from(&path).display()))?;
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let k = hex::decode(line).with_context(|| format!("{TRUSTED_ENV}: {line:?} is not hex"))?;
            ensure!(k.len() == 32, "{TRUSTED_ENV}: {line:?} is not a 32-byte key");
            if !out.chunks(32).any(|c| c == k.as_slice()) {
                out.extend_from_slice(&k);
            }
        }
    }
    info!("initramfs: {} trusted signer(s)", out.len() / 32);
    Ok(out)
}
