// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build test`: run the conformance suite against the disk image.

use anyhow::{Context, Result};
use tracing::{error, info};

use buildkit::Ctx;
use buildkit::host::paths;

/// No new text on display paths (`tools/textaudit`): a string literal
/// a panel or report would show, not in the baseline, fails the run.
fn text(ctx: &Ctx) -> Result<()> {
    let report = textaudit::audit(&ctx.paths.repo)?;
    if report.fails() {
        anyhow::bail!(
            "new text literals on display paths (cargo run -p textaudit):\n{}",
            report.details(20)
        );
    }
    for line in report.summary().lines() {
        info!("textaudit: {line}");
    }
    Ok(())
}

/// The format freeze (`tools/fmtcheck`): a frozen format changed without
/// a version bump fails the run before anything boots.
fn freeze(ctx: &Ctx) -> Result<()> {
    let rows = fmtcheck::check(&ctx.paths.repo)?;
    let bad: Vec<&str> = rows
        .iter()
        .filter(|r| r.fails())
        .map(|r| r.name.as_str())
        .collect();
    if !bad.is_empty() {
        anyhow::bail!(
            "frozen formats changed without a version bump: {} (cargo run -p fmtcheck)",
            bad.join(", ")
        );
    }
    info!("fmtcheck: {} frozen formats unchanged", rows.len());
    Ok(())
}

/// Help everywhere (`tools/helpaudit`): a panel, display file, command,
/// parameter or message without help fails the run before anything boots.
fn help(ctx: &Ctx) -> Result<()> {
    let report = helpaudit::audit(&ctx.paths.repo)?;
    if report.fails() {
        anyhow::bail!(
            "help is missing (cargo run -p helpaudit):\n{}",
            report.summary()
        );
    }
    let stale = helpaudit::generate::stale(&ctx.paths.repo)?;
    if !stale.is_empty() {
        anyhow::bail!(
            "command help is out of date (cargo run -p helpaudit -- gen): {}",
            stale.join(" ")
        );
    }
    for line in report.summary().lines() {
        info!("helpaudit: {line}");
    }
    Ok(())
}

/// The SQL kit (`tools/conformance/sql`) in memory: a difference from
/// the oracle's capture not listed in `known.txt` fails the run.
fn sql(_ctx: &Ctx) -> Result<()> {
    let report = conformance::sql::run_all(None).map_err(anyhow::Error::msg)?;
    let problems = report.problems();
    if !problems.is_empty() {
        anyhow::bail!(
            "SQL kit (cargo test -p conformance --test sql_kit):\n{}",
            problems.join("\n")
        );
    }
    info!(
        "sql kit: {} agree, {} known to differ, {} not captured",
        report.pass.len(),
        report.fail.len(),
        report.uncaptured.len()
    );
    Ok(())
}

/// Run the Phase 1 suite for `media`.
pub fn test(
    ctx: &Ctx,
    media: conformance::Media,
    install: bool,
    bench: u64,
    pulls: u32,
) -> Result<()> {
    freeze(ctx)?;
    help(ctx)?;
    text(ctx)?;
    sql(ctx)?;
    let image = paths::read_pointer(&ctx.paths.disk_pointer())
        .context("no disk image; run `disk` first")?;
    let iso = paths::read_pointer(&ctx.paths.iso_pointer()).ok();
    let mut suite = conformance::Suite::new(
        Some(&image),
        iso.as_deref(),
        media,
        ctx.paths.target.join("conformance"),
    )?;
    suite.set_install(install);
    suite.set_phase2(bench, pulls);
    let l = suite.launch();
    info!(
        "test: {} with {} ({})",
        l.qemu.display(),
        l.firmware.display(),
        if l.kvm { "kvm" } else { "tcg" }
    );
    match suite.run_all() {
        Ok(reports) => {
            for r in &reports {
                info!(
                    "test: PASS {:<24} {:6.1}s  {}",
                    r.name,
                    r.elapsed.as_secs_f64(),
                    r.detail
                );
            }
            info!("test: PASS, {} steps", reports.len());
            Ok(())
        }
        Err(e) => {
            if let Some(log) = e.log() {
                eprintln!("----- serial log -----\n{log}\n----- end serial log -----");
            }
            error!("test: FAIL: {e}");
            Err(e.into())
        }
    }
}
