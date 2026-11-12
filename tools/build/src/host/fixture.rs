// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build fixture`: the proof that another repository can pin
//! `buildkit` and drive it (docs/BUILDKIT.md). The fixture product
//! `qsdemo` (`tools/buildkit/fixture/`) is copied to a clean directory,
//! given a `Cargo.toml` that takes QS/400's crates as git dependencies
//! at this repository's HEAD, built from nothing with its own
//! orchestrator, and a copy of its disk image booted under QEMU. The
//! step passes when the serial console shows `QSDEMO` with the fixture's
//! own release (not QS/400's), QEMU exits cleanly after the power off,
//! and the disk is the fixture's own in what QS/400's carries its name
//! in: the ESP's volume label and id, and the A/B record its PID 1
//! wrote (`abrec` under its own magic and partition type).
//!
//! Only committed work is seen: a git dependency is HEAD, not the
//! working tree.

use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use abrec::{AbPart, Spec};
use anyhow::{Context, Result, bail, ensure};
use buildkit::Ctx;
use buildkit::host::cmd::Cmd;
use diskimg::layout::ESP_TYPE_GUID;
use diskimg::{SECTOR, Window, fat};
use tracing::{info, warn};
use uuid::{Uuid, uuid};

/// The fixture in the tree.
const FIXTURE: &str = "tools/buildkit/fixture";
/// The build ID the fixture is built with, so the banner is known.
const BUILD_ID: &str = "fixture";
/// How long the boot may take, under TCG too.
const BOOT: Duration = Duration::from_secs(300);
/// The fixture's A/B record as its PID 1 declares it
/// (`fixture/init/src/main.rs`).
const RECORD: Spec = Spec {
    magic: *b"QSDEMOAB",
    version: 1,
    type_guid: uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d01"),
    slot_bytes: 4096,
    min_payload: 4,
};
/// The fixture's ESP as its orchestrator names it
/// (`fixture/build/src/main.rs`).
const ESP_LABEL: &[u8; 11] = b"QSDEMO ESP ";
const ESP_VOLUME_ID: u32 = 0x5153_444D;

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
        .arg(format!("if=virtio,format=raw,file={}", disk.display()))
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

/// The first partition of `disk` with GPT type `ty`: its offset and
/// length in bytes.
fn partition(disk: &mut fs::File, ty: Uuid) -> Result<(u64, u64)> {
    let table = diskimg::read_table(disk)?;
    let (_, _, first, last) = table
        .iter()
        .find(|p| p.1 == ty)
        .with_context(|| format!("the fixture's disk has no partition of type {ty}"))?;
    Ok((first * SECTOR, (last - first + 1) * SECTOR))
}

/// The ESP is the fixture's: its label and volume id in the FAT32 boot
/// sector, not the defaults QS/400's volumes have.
fn check_esp(disk: &mut fs::File) -> Result<()> {
    let (at, _) = partition(disk, ESP_TYPE_GUID)?;
    let mut sector = [0u8; 512];
    disk.seek(SeekFrom::Start(at))?;
    disk.read_exact(&mut sector)?;
    let id = u32::from_le_bytes(sector[67..71].try_into()?);
    let label = &sector[71..82];
    ensure!(
        label == ESP_LABEL && id == ESP_VOLUME_ID,
        "the fixture's ESP is `{}` with volume id {id:#010x}",
        String::from_utf8_lossy(label)
    );
    ensure!(
        label != fat::LABEL && id != fat::VOLUME_ID,
        "the fixture's ESP has QS/400's name"
    );
    Ok(())
}

/// The A/B record is the fixture's: on the partition of its own type,
/// under its own magic, the one write of the boot in copy A; and no LIC
/// record to QS/400.
fn check_record(disk: &mut fs::File) -> Result<()> {
    ensure!(
        RECORD.type_guid != licpart::LIC_TYPE_GUID && RECORD.magic != licpart::SPEC.magic,
        "the fixture's record must not be QS/400's"
    );
    ensure!(
        partition(disk, licpart::LIC_TYPE_GUID).is_err(),
        "the fixture's disk has a QS/400 LIC partition"
    );
    let (at, len) = partition(disk, RECORD.type_guid)?;
    let mut bytes = vec![0u8; 2 * RECORD.slot_bytes];
    Window::new(disk, at, len).read_exact(&mut bytes)?;
    ensure!(
        bytes[..8] == RECORD.magic,
        "the record partition starts with {:02x?}",
        &bytes[..8]
    );
    let mut part = AbPart::open(RECORD, std::io::Cursor::new(bytes.clone()));
    let [a, b] = part.read_slots();
    let a = a.context("copy A of the fixture's record does not read")?;
    ensure!(
        a.sequence == 1 && a.payload == 1u32.to_le_bytes() && b.is_none(),
        "the fixture's record after one boot: copy A {a:?}, copy B {b:?}"
    );
    let mut ours = licpart::LicPart::open(std::io::Cursor::new(bytes));
    ensure!(
        ours.read().is_err(),
        "QS/400 reads the fixture's record as a LIC record"
    );
    Ok(())
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
    // A copy is booted, so the image stays as built and the record the
    // boot wrote can be read afterwards.
    let booted = dir.join("booted.img");
    fs::copy(&disk, &booted)?;
    let open = || fs::OpenOptions::new().read(true).write(true).open(&booted);
    check_esp(&mut open()?)?;
    let lines = boot(&booted)?;
    let want = format!("QSDEMO {release} {BUILD_ID}");
    let record = format!(
        "QSDEMO record {} copy A sequence 1 boots 1",
        String::from_utf8_lossy(&RECORD.magic)
    );
    for want in [&want, &record] {
        ensure!(
            lines.iter().any(|l| l.trim() == want.as_str()),
            "no `{want}` on the fixture's console:\n{}",
            lines.join("\n")
        );
    }
    check_record(&mut open()?)?;
    fs::remove_file(&booted)?;
    let line = format!(
        "qsdemo built from git dependencies at {} and booted: `{want}`, powered off; \
         its ESP is `{}` and its A/B record `{}`",
        &rev[..rev.len().min(9)],
        String::from_utf8_lossy(ESP_LABEL).trim_end(),
        String::from_utf8_lossy(&RECORD.magic)
    );
    info!("fixture: {line}");
    Ok(line)
}
