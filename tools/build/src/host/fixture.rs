// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build fixture`: the proof that another repository can pin
//! `buildkit` and drive it (docs/BUILDKIT.md). The fixture product
//! `qsdemo` (`tools/buildkit/fixture/`) is copied to a clean directory,
//! given a `Cargo.toml` that takes QS/400's crates as git dependencies
//! at this repository's HEAD, built from nothing with its own
//! orchestrator, and its disk image booted under QEMU. The step passes
//! when the serial console shows `QSDEMO` with the fixture's own
//! release (not QS/400's) and QEMU exits cleanly after the power off.
//!
//! Only committed work is seen: a git dependency is HEAD, not the
//! working tree.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use buildkit::Ctx;
use buildkit::host::cmd::Cmd;
use tracing::{info, warn};

/// The fixture in the tree.
const FIXTURE: &str = "tools/buildkit/fixture";
/// The build ID the fixture is built with, so the banner is known.
const BUILD_ID: &str = "fixture";
/// How long the boot may take, under TCG too.
const BOOT: Duration = Duration::from_secs(300);

/// Copy the tree `from` to `to`.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from).with_context(|| from.display().to_string())? {
        let e = e?;
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), &dest)?;
        }
    }
    Ok(())
}

/// `git` in the repository, its output trimmed.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    Ok(Cmd::new("git", "git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .capture()?
        .trim()
        .to_string())
}

/// The fixture's tree in a clean directory, as its own repository
/// would be: its sources, its `VERSION`, the kernel pin and toolchain
/// QS/400 uses, and a `Cargo.toml` naming this repository's HEAD.
fn lay_out(ctx: &Ctx, dir: &Path) -> Result<String> {
    let repo = &ctx.paths.repo;
    if dir.exists() {
        fs::remove_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    copy_tree(&repo.join(FIXTURE), dir)?;
    let rev = git(repo, &["rev-parse", "HEAD"])?;
    if !git(repo, &["status", "--porcelain"])?.is_empty() {
        warn!("fixture: the working tree has changes; the fixture builds HEAD ({rev})");
    }
    let url = format!("file://{}", repo.display().to_string().replace('\\', "/"));
    let manifest = fs::read_to_string(dir.join("Cargo.toml.in"))?
        .replace("@REPO@", &url)
        .replace("@REV@", &rev);
    fs::write(dir.join("Cargo.toml"), manifest)?;
    fs::remove_file(dir.join("Cargo.toml.in"))?;
    // The same kernel and toolchain pins: the fixture proves the
    // pipeline, not another kernel.
    fs::create_dir_all(dir.join("kernel"))?;
    for f in ["kernel/VERSION", "kernel/SHA256", "rust-toolchain.toml"] {
        fs::copy(repo.join(f), dir.join(f)).with_context(|| f.to_string())?;
    }
    // The tarball this build already fetched, when there is one.
    let version = fs::read_to_string(repo.join("kernel/VERSION"))?;
    let tarball = format!("linux-{}.tar.xz", version.trim());
    let have = ctx.paths.src().join(&tarball);
    if have.is_file() {
        let src = dir.join("target").join("src");
        fs::create_dir_all(&src)?;
        fs::copy(&have, src.join(&tarball))?;
    }
    Ok(rev)
}

/// The fixture's version as its release, `0.3.0` as `V0R3M0`.
fn fixture_release(repo: &Path) -> Result<String> {
    let text = fs::read_to_string(repo.join(FIXTURE).join("VERSION"))?;
    let n: Vec<&str> = text.trim().split('.').collect();
    ensure!(
        n.len() == 3,
        "the fixture's VERSION is not MAJOR.MINOR.PATCH"
    );
    Ok(format!("V{}R{}M{}", n[0], n[1], n[2]))
}

/// Boot `disk` under QEMU with the serial console on standard output;
/// every line it printed, once QEMU has ended.
fn boot(disk: &Path) -> Result<Vec<String>> {
    let firmware = conformance::firmware::locate().map_err(|e| anyhow::anyhow!("{e}"))?;
    let kvm = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok();
    let mut cmd = Command::new("qemu-system-x86_64");
    cmd.args(["-machine", "q35", "-m", "512", "-nodefaults", "-no-reboot"])
        .args(["-display", "none", "-serial", "stdio"])
        .args(["-accel", if kvm { "kvm" } else { "tcg" }])
        .arg("-drive")
        .arg(format!(
            "if=pflash,format=raw,readonly=on,file={}",
            firmware.display()
        ))
        .arg("-drive")
        .arg(format!(
            "if=virtio,format=raw,snapshot=on,file={}",
            disk.display()
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().context("cannot start qemu-system-x86_64")?;
    let out = child.stdout.take().context("no QEMU output")?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut lines = Vec::new();
    loop {
        while let Ok(l) = rx.try_recv() {
            lines.push(l);
        }
        if let Some(status) = child.try_wait()? {
            // What was still in the pipe when QEMU ended.
            while let Ok(l) = rx.recv_timeout(Duration::from_millis(200)) {
                lines.push(l);
            }
            ensure!(status.success(), "QEMU ended with {status}");
            return Ok(lines);
        }
        if started.elapsed() > BOOT {
            let _ = child.kill();
            let _ = child.wait();
            bail!(
                "the fixture did not power off within {} s; its console:\n{}",
                BOOT.as_secs(),
                lines.join("\n")
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `build fixture`. Returns one line for the report.
pub fn fixture(ctx: &Ctx) -> Result<String> {
    let dir = ctx.paths.target.join("fixture");
    let rev = lay_out(ctx, &dir)?;
    let release = fixture_release(&ctx.paths.repo)?;
    ensure!(
        release != ctx.product.release,
        "the fixture's release must differ from {}'s to prove it is its own",
        ctx.product.name
    );
    info!("fixture: building qsdemo {release} in {}", dir.display());
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = dir.join("target");
    let mut build = Cmd::new(cargo, "fixture-build");
    build
        .cwd(&dir)
        .args(["run", "-p", "qsdemo-build", "--", "all"])
        // The fixture's own directories and identity, whatever this
        // build was given.
        .env("CARGO_TARGET_DIR", &target)
        .env("QSDEMO_TARGET_DIR", &target)
        .env("QSDEMO_BUILD_ID", BUILD_ID);
    if let Some(jobs) = ctx.product.env("JOBS") {
        build.env("QSDEMO_JOBS", jobs);
    }
    build.run(ctx)?;

    let pointer = target.join("images").join("disk.path");
    let disk = PathBuf::from(fs::read_to_string(&pointer)?.trim());
    ensure!(disk.is_file(), "the fixture wrote no disk image");
    let lines = boot(&disk)?;
    let want = format!("QSDEMO {release} {BUILD_ID}");
    ensure!(
        lines.iter().any(|l| l.trim() == want),
        "no `{want}` on the fixture's console:\n{}",
        lines.join("\n")
    );
    let line = format!(
        "qsdemo built from git dependencies at {} and booted: `{want}`, powered off",
        &rev[..rev.len().min(9)]
    );
    info!("fixture: {line}");
    Ok(line)
}
