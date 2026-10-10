// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Build `slic` for the musl target and prove it is static and stripped.

pub mod elf;

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::{Result, ensure};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::cmd::Cmd;
use crate::host::hash::sha256_file;

/// The one and only image target.
pub const RUST_TARGET: &str = "x86_64-unknown-linux-musl";
/// ELF `e_machine` for `x86_64`.
const ELF_MACHINE: u16 = 62;

/// `build userland`.
pub fn userland(ctx: &Ctx) -> Result<PathBuf> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    info!("userland: cargo build --release -p slic --target {RUST_TARGET}");
    Cmd::new(cargo, "userland")
        .cwd(&ctx.paths.repo)
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "slic",
            "--target",
            RUST_TARGET,
        ])
        .arg("--target-dir")
        .arg(&ctx.paths.target)
        .run(ctx)?;

    let bin = ctx.paths.slic();
    let info = elf::inspect(&bin)?;
    ensure!(
        info.machine == ELF_MACHINE,
        "slic: e_machine {} is not x86_64",
        info.machine
    );
    ensure!(
        !info.has_interp,
        "slic: has PT_INTERP, it is dynamically linked"
    );
    ensure!(
        info.needed.is_empty(),
        "slic: has DT_NEEDED {:?}",
        info.needed
    );
    ensure!(!info.has_symtab, "slic: not stripped (.symtab present)");

    let size = fs::metadata(&bin)?.len();
    info!(
        "userland: slic static, stripped, {size} bytes, sha256 {}",
        sha256_file(&bin)?
    );
    Ok(bin)
}
