// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Host prerequisite check, printed as a table.

use std::path::Path;

use anyhow::{Result, bail};

use crate::ctx::Ctx;
use crate::host::cmd::{Cmd, which};
use crate::userland::RUST_TARGET;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Missing,
}

struct Row {
    name: &'static str,
    required: bool,
    status: Status,
    detail: String,
}

fn tool(name: &'static str, required: bool, version_args: &[&str]) -> Row {
    match which(name) {
        Some(path) => {
            let detail = Cmd::new(name, "doctor")
                .args(version_args)
                .capture()
                .ok()
                .and_then(|s| s.lines().next().map(str::to_string))
                .unwrap_or_else(|| path.display().to_string());
            Row {
                name,
                required,
                status: Status::Ok,
                detail,
            }
        }
        None => Row {
            name,
            required,
            status: Status::Missing,
            detail: "not on PATH".into(),
        },
    }
}

fn file(name: &'static str, required: bool, candidates: &[&str], hint: &str) -> Row {
    match candidates.iter().find(|p| Path::new(p).exists()) {
        Some(p) => Row {
            name,
            required,
            status: Status::Ok,
            detail: (*p).to_string(),
        },
        None => Row {
            name,
            required,
            status: if required {
                Status::Missing
            } else {
                Status::Warn
            },
            detail: hint.to_string(),
        },
    }
}

fn firmware() -> Row {
    let name = "OVMF (QS400_OVMF_CODE)";
    match conformance::firmware::locate() {
        Ok(p) => Row {
            name,
            required: true,
            status: Status::Ok,
            detail: p.display().to_string(),
        },
        Err(e) => Row {
            name,
            required: true,
            status: Status::Missing,
            detail: e.to_string(),
        },
    }
}

/// `build doctor`.
pub fn doctor(ctx: &Ctx) -> Result<()> {
    let mut rows = vec![
        tool("rustc", true, &["--version"]),
        tool("cargo", true, &["--version"]),
        rust_target(),
        tool("clang", true, &["--version"]),
        tool("ld.lld", true, &["--version"]),
        tool("llvm-ar", true, &["--version"]),
        tool("llvm-nm", true, &["--version"]),
        tool("llvm-objcopy", true, &["--version"]),
        tool("llvm-objdump", true, &["--version"]),
        tool("llvm-readelf", true, &["--version"]),
        tool("llvm-strip", true, &["--version"]),
        tool("make", true, &["--version"]),
        tool("flex", true, &["--version"]),
        tool("bison", true, &["--version"]),
        tool("bc", true, &["--version"]),
        tool("perl", true, &["--version"]),
        tool("tar", true, &["--version"]),
        tool("xz", true, &["--version"]),
        tool("gzip", true, &["--version"]),
        file(
            "libelf headers",
            true,
            &["/usr/include/libelf.h", "/usr/include/libelf/libelf.h"],
            "apt install libelf-dev (objtool needs it)",
        ),
        file(
            "openssl headers",
            false,
            &["/usr/include/openssl/opensslv.h"],
            "apt install libssl-dev (only if kernel signing options are enabled)",
        ),
        tool("git", true, &["--version"]),
        tool("gpg", false, &["--version"]),
        tool("cargo-deny", true, &["--version"]),
        tool("qemu-system-x86_64", true, &["--version"]),
        firmware(),
        kvm(),
        target_dir(ctx),
    ];
    if !cfg!(target_os = "linux") {
        rows.insert(
            0,
            Row {
                name: "host os",
                required: true,
                status: Status::Missing,
                detail: "the pipeline runs on Linux only; use WSL".into(),
            },
        );
    }

    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(10);
    println!("{:<width$}  {:<8}  detail", "check", "status");
    println!("{}", "-".repeat(width + 60));
    let mut missing = 0;
    for r in &rows {
        let status = match r.status {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Missing if r.required => {
                missing += 1;
                "MISSING"
            }
            Status::Missing => "absent",
        };
        println!("{:<width$}  {:<8}  {}", r.name, status, r.detail);
    }
    println!();
    if missing > 0 {
        bail!("{missing} required prerequisite(s) missing; see docs/PHASE0.md");
    }
    println!("all required prerequisites present");
    Ok(())
}

fn rust_target() -> Row {
    let installed = Cmd::new("rustup", "doctor")
        .args(["target", "list", "--installed"])
        .capture()
        .unwrap_or_default();
    if installed.lines().any(|l| l == RUST_TARGET) {
        Row {
            name: "musl target",
            required: true,
            status: Status::Ok,
            detail: RUST_TARGET.into(),
        }
    } else {
        Row {
            name: "musl target",
            required: true,
            status: Status::Missing,
            detail: format!("rustup target add {RUST_TARGET}"),
        }
    }
}

fn kvm() -> Row {
    let name = "/dev/kvm";
    if !Path::new(name).exists() {
        return Row {
            name,
            required: false,
            status: Status::Warn,
            detail: "absent; QEMU will use TCG".into(),
        };
    }
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(name)
    {
        Ok(_) => Row {
            name,
            required: false,
            status: Status::Ok,
            detail: "usable".into(),
        },
        Err(e) => Row {
            name,
            required: false,
            status: Status::Warn,
            detail: format!("{e}; add yourself to the kvm group, else TCG"),
        },
    }
}

fn target_dir(ctx: &Ctx) -> Row {
    let t = ctx.paths.target.display().to_string();
    if t.starts_with("/mnt/") {
        Row {
            name: "target dir",
            required: false,
            status: Status::Warn,
            detail: format!("{t} is on drvfs; set QS400_TARGET_DIR to an ext4 path"),
        }
    } else {
        Row {
            name: "target dir",
            required: false,
            status: Status::Ok,
            detail: t,
        }
    }
}
