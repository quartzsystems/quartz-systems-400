// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Content-hash manifest of every artifact, signed with ed25519.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ed25519_dalek::{Signer, SigningKey, Verifier};
use serde::Serialize;
use tracing::{info, warn};

use crate::ctx::Ctx;
use crate::host::cmd::{Cmd, on_path};
use crate::host::hash::{sha256_bytes, sha256_file};
use crate::host::paths;
use crate::kernel::{self, CMDLINE, fetch};
use crate::userland::RUST_TARGET;

const KEY_ENV: &str = "QS400_SIGNING_KEY";

#[derive(Serialize)]
struct Manifest {
    schema: u32,
    product: &'static str,
    inputs: Inputs,
    artifacts: Vec<Artifact>,
}

#[derive(Serialize)]
struct Inputs {
    target: &'static str,
    kernel_version: String,
    kernel_sha256: String,
    kernel_config_sha256: String,
    cmdline: &'static str,
    source_date_epoch: u64,
    rustc: String,
    clang: String,
    git_rev: String,
}

#[derive(Serialize)]
struct Artifact {
    path: String,
    kind: &'static str,
    sha256: String,
    size: u64,
}

#[derive(Serialize)]
struct SignatureFile {
    algorithm: &'static str,
    public_key: String,
    manifest_sha256: String,
    signature: String,
}

/// `build manifest`.
pub fn manifest(ctx: &Ctx) -> Result<PathBuf> {
    let pin = fetch::read_pin(&ctx.paths)?;
    let mut artifacts = Vec::new();
    let mut add = |kind: &'static str, path: &Path| -> Result<()> {
        if !path.is_file() {
            return Ok(());
        }
        artifacts.push(Artifact {
            path: ctx.paths.relative(path),
            kind,
            sha256: sha256_file(path)?,
            size: fs::metadata(path)?.len(),
        });
        Ok(())
    };
    if let Ok(p) = paths::read_pointer(&ctx.paths.uki_pointer()) {
        add("uki", &p)?;
    }
    if let Ok(p) = paths::read_pointer(&ctx.paths.disk_pointer()) {
        add("disk", &p)?;
    }
    add("initramfs", &ctx.paths.initramfs())?;
    add("slic", &ctx.paths.slic())?;
    let config = kernel::config_path(ctx);
    add("kernel-config", &config)?;
    ensure!(
        !artifacts.is_empty(),
        "nothing to put in a manifest; build something first"
    );

    let manifest = Manifest {
        schema: 1,
        product: "QS/400 phase 0",
        inputs: Inputs {
            target: RUST_TARGET,
            kernel_version: pin.version,
            kernel_sha256: pin.sha256.unwrap_or_else(|| "unpinned".into()),
            kernel_config_sha256: if config.is_file() {
                sha256_file(&config)?
            } else {
                "absent".into()
            },
            cmdline: CMDLINE,
            source_date_epoch: kernel::SOURCE_DATE_EPOCH.parse()?,
            rustc: tool_version("rustc", &["-vV"]),
            clang: tool_version("clang", &["--version"]),
            git_rev: git_rev(&ctx.paths.repo),
        },
        artifacts,
    };
    let mut json = serde_json::to_string_pretty(&manifest)?;
    json.push('\n');

    let out_dir = ctx.paths.images();
    fs::create_dir_all(&out_dir)?;
    let manifest_path = out_dir.join("manifest.json");
    fs::write(&manifest_path, &json)?;

    let key = signing_key(ctx)?;
    let signature = key.sign(json.as_bytes());
    key.verifying_key()
        .verify(json.as_bytes(), &signature)
        .context("self-check of signature")?;
    let sig = SignatureFile {
        algorithm: "ed25519",
        public_key: hex::encode(key.verifying_key().to_bytes()),
        manifest_sha256: sha256_bytes(json.as_bytes()),
        signature: hex::encode(signature.to_bytes()),
    };
    let sig_path = out_dir.join("manifest.sig");
    let mut sig_json = serde_json::to_string_pretty(&sig)?;
    sig_json.push('\n');
    fs::write(&sig_path, sig_json)?;

    info!(
        "manifest: {} ({} artifacts, sha256 {}), signature {}",
        manifest_path.display(),
        manifest.artifacts.len(),
        sig.manifest_sha256,
        sig_path.display()
    );
    Ok(manifest_path)
}

/// Load the signing key from `QS400_SIGNING_KEY`, else a dev key under
/// `target/keys/`, generating it when absent.
fn signing_key(ctx: &Ctx) -> Result<SigningKey> {
    let (path, from_env) = match env::var_os(KEY_ENV) {
        Some(p) => (PathBuf::from(p), true),
        None => (ctx.paths.keys().join("dev.ed25519"), false),
    };
    if path.is_file() {
        let text = fs::read_to_string(&path)?;
        let bytes = hex::decode(text.trim()).context("signing key is not hex")?;
        let seed: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .context("signing key must be 32 bytes")?;
        if !from_env {
            warn!(
                "signing with the DEV key {} (set {KEY_ENV} for a real key)",
                path.display()
            );
        }
        return Ok(SigningKey::from_bytes(&seed));
    }
    if from_env {
        bail!("{KEY_ENV}={} does not exist", path.display());
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).context("getrandom")?;
    fs::create_dir_all(path.parent().context("no parent")?)?;
    write_private(&path, format!("{}\n", hex::encode(seed)))?;
    warn!(
        "no signing key found: generated a DEV key at {} (not for release)",
        path.display()
    );
    Ok(SigningKey::from_bytes(&seed))
}

#[cfg(unix)]
fn write_private(path: &Path, contents: String) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: String) -> Result<()> {
    fs::write(path, contents)?;
    Ok(())
}

/// First line of a tool's version output, or `unavailable`.
fn tool_version(tool: &str, args: &[&str]) -> String {
    if !on_path(tool) {
        return "unavailable".into();
    }
    Cmd::new(tool, "version")
        .args(args)
        .capture()
        .ok()
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_else(|| "unavailable".into())
}

/// `HEAD` plus `-dirty`, or `unknown` outside a git checkout.
fn git_rev(repo: &Path) -> String {
    let Ok(rev) = Cmd::new("git", "git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .capture()
    else {
        return "unknown".into();
    };
    let dirty = Cmd::new("git", "git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain"])
        .capture()
        .is_ok_and(|s| !s.trim().is_empty());
    format!("{}{}", rev.trim(), if dirty { "-dirty" } else { "" })
}
