// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kernel source: pin, download, verify, extract.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::ctx::Ctx;
use crate::host::cmd::{Cmd, on_path};
use crate::host::hash::sha256_file;
use crate::host::paths::Paths;

const BASE_URL: &str = "https://cdn.kernel.org/pub/linux/kernel/v6.x";
const SUMS_FILE: &str = "sha256sums.asc";
const AUTOSIGNER: &str = "autosigner@kernel.org";
/// Kernel.org checksum autosigner, fetched over WKD.
const AUTOSIGNER_FPR: &str = "B8868C80BA62A1FFFAF5FDA9632D3A06589DA6B1";

/// The committed kernel pin.
pub struct Pin {
    /// Contents of `kernel/VERSION`.
    pub version: String,
    /// First token of `kernel/SHA256`, if the file exists.
    pub sha256: Option<String>,
}

/// An extracted, verified kernel source tree.
pub struct Source {
    /// Kernel version, e.g. `6.18.55`.
    pub version: String,
    /// Extracted tree.
    pub dir: PathBuf,
}

/// Read `kernel/VERSION` and `kernel/SHA256`.
pub fn read_pin(paths: &Paths) -> Result<Pin> {
    let dir = paths.kernel_pin_dir();
    let version = fs::read_to_string(dir.join("VERSION"))
        .context("cannot read kernel/VERSION")?
        .trim()
        .to_string();
    ensure!(
        version.starts_with("6.18."),
        "kernel/VERSION must be a 6.18.y release, got {version}"
    );
    let sha256 = match fs::read_to_string(dir.join("SHA256")) {
        Ok(text) => {
            let tok = text
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            ensure!(
                tok.len() == 64 && tok.bytes().all(|b| b.is_ascii_hexdigit()),
                "kernel/SHA256 does not start with a sha256 hex digest"
            );
            Some(tok)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).context("cannot read kernel/SHA256"),
    };
    Ok(Pin { version, sha256 })
}

/// Download (if needed), verify against the pin and extract. Idempotent.
pub fn fetch(ctx: &Ctx) -> Result<Source> {
    let pin = read_pin(&ctx.paths)?;
    let src = ctx.paths.src();
    fs::create_dir_all(&src)?;
    let tarball_name = format!("linux-{}.tar.xz", pin.version);
    let tarball = src.join(&tarball_name);

    let expected = match pin.sha256 {
        Some(s) => s,
        None => record_pin(ctx, &tarball_name)?,
    };

    if tarball.is_file() {
        let got = sha256_file(&tarball)?;
        if got == expected {
            info!("{tarball_name}: present, sha256 verified");
        } else {
            warn!("{tarball_name}: sha256 mismatch, downloading again");
            fs::remove_file(&tarball)?;
        }
    }
    if !tarball.is_file() {
        let url = format!("{BASE_URL}/{tarball_name}");
        let got = download(&url, &tarball)?;
        ensure!(
            got == expected,
            "sha256 mismatch for {tarball_name}\n  expected {expected}\n  got      {got}"
        );
        info!("{tarball_name}: downloaded, sha256 verified");
    }

    let dir = src.join(format!("linux-{}", pin.version));
    let marker = dir.join(".qs400-extracted");
    if !marker.is_file() {
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        info!("extracting {tarball_name}");
        Cmd::new("tar", "fetch-extract")
            .arg("-xf")
            .arg(&tarball)
            .arg("-C")
            .arg(&src)
            .run(ctx)?;
        ensure!(
            dir.join("Makefile").is_file(),
            "extraction did not produce {}",
            dir.display()
        );
        fs::write(&marker, format!("{expected}\n"))?;
    }
    Ok(Source {
        version: pin.version,
        dir,
    })
}

/// First run: fetch `sha256sums.asc`, check its signature, record the pin.
fn record_pin(ctx: &Ctx, tarball_name: &str) -> Result<String> {
    let sums = ctx.paths.src().join(SUMS_FILE);
    info!("kernel/SHA256 missing; fetching {SUMS_FILE} to record the pin");
    download(&format!("{BASE_URL}/{SUMS_FILE}"), &sums)?;
    gpg_check(&sums)?;
    let text = fs::read_to_string(&sums)?;
    let line = text
        .lines()
        .find(|l| l.split_whitespace().nth(1) == Some(tarball_name))
        .with_context(|| format!("{SUMS_FILE} has no entry for {tarball_name}"))?;
    let sha = line
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    ensure!(sha.len() == 64, "malformed line in {SUMS_FILE}: {line}");
    let pin_file = ctx.paths.kernel_pin_dir().join("SHA256");
    fs::write(&pin_file, format!("{sha}  {tarball_name}\n"))?;
    info!("recorded {sha} in kernel/SHA256 (commit it)");
    Ok(sha)
}

/// Verify the clearsigned sums file with the autosigner key when gpg exists.
fn gpg_check(sums: &Path) -> Result<()> {
    if !on_path("gpg") {
        warn!("gpg not found; {SUMS_FILE} signature not checked");
        return Ok(());
    }
    let (ok, out) = Cmd::new("gpg", "gpg-locate")
        .args(["--batch", "--locate-keys", AUTOSIGNER])
        .capture_all()?;
    if !ok || !out.contains(AUTOSIGNER_FPR) {
        warn!("could not fetch {AUTOSIGNER} ({AUTOSIGNER_FPR}) over WKD; signature not checked");
        return Ok(());
    }
    let (ok, out) = Cmd::new("gpg", "gpg-verify")
        .args(["--batch", "--status-fd", "1", "--verify"])
        .arg(sums)
        .capture_all()?;
    let valid = out
        .lines()
        .any(|l| l.starts_with("[GNUPG:] VALIDSIG") && l.contains(AUTOSIGNER_FPR));
    if ok && valid {
        info!("{SUMS_FILE}: good signature from {AUTOSIGNER}");
        Ok(())
    } else {
        bail!("{SUMS_FILE}: signature check FAILED\n{out}");
    }
}

/// A blocking HTTPS client over rustls with the pure-Rust graviola provider.
fn client() -> Result<reqwest::blocking::Client> {
    // Installing twice is harmless; the second call just reports it.
    let _ = rustls_graviola::default_provider().install_default();
    reqwest::blocking::Client::builder()
        .user_agent("qs400-build")
        .connect_timeout(Duration::from_secs(30))
        .timeout(None)
        .build()
        .context("cannot build HTTP client")
}

/// Download `url` to `dest` atomically, returning the hex sha256.
fn download(url: &str, dest: &Path) -> Result<String> {
    info!("downloading {url}");
    let mut resp = client()?
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let total = resp.content_length();
    let tmp = dest.with_extension("part");
    let mut file =
        File::create(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut done: u64 = 0;
    let mut next_report = 10;
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        done += n as u64;
        if let Some(total) = total.filter(|t| *t > 0) {
            let pct = done * 100 / total;
            if pct >= next_report {
                info!("  {pct}% ({done} / {total} bytes)");
                next_report = pct / 10 * 10 + 10;
            }
        }
    }
    file.flush()?;
    drop(file);
    fs::rename(&tmp, dest)?;
    Ok(hex::encode(hasher.finalize()))
}
