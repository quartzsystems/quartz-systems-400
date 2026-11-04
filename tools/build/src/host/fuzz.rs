// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build fuzz`: the fuzzer (`tools/fuzz`) built with `SanitizerCoverage`
//! counters in its own target directory, then every target (or one) run
//! for the time given. Crashes land in `tools/fuzz/crashes/<target>/`;
//! the corpora grow in `tools/fuzz/corpus/<target>/`.

use std::env;

use anyhow::Result;
use tracing::info;

use crate::ctx::Ctx;
use crate::host::cmd::Cmd;

/// The flags that give every edge an 8-bit counter.
const SANCOV: &str = "-C passes=sancov-module -C llvm-args=-sanitizer-coverage-level=3 \
                      -C llvm-args=-sanitizer-coverage-inline-8bit-counters";

/// The host triple the fuzzer builds for. Naming it keeps `RUSTFLAGS` off
/// build scripts and proc macros, which must not be instrumented.
const HOST: &str = "x86_64-unknown-linux-gnu";

/// Runs `target` (every one when `None`) for `secs` seconds each.
pub fn fuzz(ctx: &Ctx, target: Option<&str>, secs: u64) -> Result<()> {
    // MSVC's linker cannot take the counters' sections (LLVM's COMDAT
    // error); the fuzzer runs on Linux, like the rest of the pipeline.
    anyhow::ensure!(cfg!(target_os = "linux"), "build fuzz runs on Linux (WSL)");
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let dir = ctx.paths.target.join("fuzz");
    let mut c = Cmd::new(&cargo, "qsfuzz");
    c.cwd(&ctx.paths.repo)
        .env("RUSTFLAGS", SANCOV)
        .env("CARGO_TARGET_DIR", &dir)
        .args(["run", "--profile", "fuzz", "--target", HOST, "-p", "fuzz", "--bin", "qsfuzz", "--"]);
    match target {
        Some(t) => c.args(["run", t]),
        None => c.arg("all"),
    };
    c.arg(secs.to_string());
    info!("fuzz: {} for {secs} s each", target.unwrap_or("every target"));
    c.run(ctx)
}
