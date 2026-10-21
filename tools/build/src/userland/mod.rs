// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Build `slic` and `qsjob` for the musl target and prove both are
//! static and stripped.

pub mod elf;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, ensure};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::cmd::Cmd;
use crate::host::hash::sha256_file;

/// The one and only image target.
pub const RUST_TARGET: &str = "x86_64-unknown-linux-musl";
/// ELF `e_machine` for `x86_64`.
const ELF_MACHINE: u16 = 62;

/// `build userland`: the binaries, `slic` first.
pub fn userland(ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    info!("userland: cargo build --release -p slic -p qsjob --target {RUST_TARGET}");
    Cmd::new(cargo, "userland")
        .cwd(&ctx.paths.repo)
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "slic",
            "-p",
            "qsjob",
            "--target",
            RUST_TARGET,
        ])
        .arg("--target-dir")
        .arg(&ctx.paths.target)
        .run(ctx)?;

    let bins = vec![ctx.paths.slic(), ctx.paths.qsjob()];
    for bin in &bins {
        check(bin)?;
    }
    Ok(bins)
}

/// Static, stripped, `x86_64`.
fn check(bin: &Path) -> Result<()> {
    let name = bin
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let info = elf::inspect(bin)?;
    ensure!(
        info.machine == ELF_MACHINE,
        "{name}: e_machine {} is not x86_64",
        info.machine
    );
    ensure!(
        !info.has_interp,
        "{name}: has PT_INTERP, it is dynamically linked"
    );
    ensure!(
        info.needed.is_empty(),
        "{name}: has DT_NEEDED {:?}",
        info.needed
    );
    ensure!(!info.has_symtab, "{name}: not stripped (.symtab present)");
    let size = fs::metadata(bin)?.len();
    info!(
        "userland: {name} static, stripped, {size} bytes, sha256 {}",
        sha256_file(bin)?
    );
    Ok(())
}
