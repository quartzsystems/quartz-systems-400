// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `conformance [--image <disk.img>]`: run the Phase 1 suite.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use conformance::{Media, Suite};

#[derive(Parser)]
#[command(
    name = "conformance",
    about = "Boot a QS/400 image under QEMU and run the Phase 1 steps"
)]
struct Cli {
    /// Disk image. Defaults to the orchestrator's latest.
    #[arg(long)]
    image: Option<PathBuf>,
    /// Installation ISO. Defaults to the orchestrator's latest.
    #[arg(long)]
    iso: Option<PathBuf>,
    /// What to boot: `disk` (the six steps) or `iso` (the media steps).
    #[arg(long, default_value = "disk")]
    media: Media,
    /// With `--media iso`: install onto a blank disk and boot it.
    #[arg(long)]
    install: bool,
    /// Objects in the bench step of the disk suite.
    #[arg(long, default_value_t = 20_000)]
    bench: u64,
    /// Power pulls in the disk suite (0 skips them).
    #[arg(long, default_value_t = 3)]
    pulls: u32,
    /// Run only the steps whose names start with this (the boot step
    /// always runs; the later phases' steps are skipped).
    #[arg(long)]
    only: Option<String>,
    /// Instead of the suite: the load run with this many concurrent
    /// TN5250 sessions.
    #[arg(long)]
    load: Option<usize>,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let image = cli.image.or_else(|| conformance::default_image().ok());
    let iso = cli.iso.or_else(|| conformance::default_iso().ok());
    let mut suite = Suite::new(
        image.as_deref(),
        iso.as_deref(),
        cli.media,
        conformance::target_dir().join("conformance"),
    )?;
    suite.set_install(cli.install);
    suite.set_phase2(cli.bench, cli.pulls);
    suite.set_only(cli.only.clone());
    if cli.only.is_some() {
        suite.set_phase2(cli.bench, 0);
    }
    if let Some(n) = cli.load {
        let r = suite.run_load(n)?;
        println!("LOAD: {} sessions, {} completed in {:.1}s", r.sessions, r.completed, r.wall.as_secs_f64());
        println!("  sign-on        median {:6.2}s  p95 {:6.2}s", r.sign_on.0.as_secs_f64(), r.sign_on.1.as_secs_f64());
        println!("  WRKACTJOB      median {:6.2}s  p95 {:6.2}s", r.first.0.as_secs_f64(), r.first.1.as_secs_f64());
        println!("  F5 refresh     median {:6.2}s  p95 {:6.2}s", r.refresh.0.as_secs_f64(), r.refresh.1.as_secs_f64());
        println!("  QEMU peak RSS  {} MiB; CPU {:.1}s over the run", r.qemu_rss_mib, r.qemu_cpu_secs);
        let mut seen = std::collections::BTreeMap::new();
        for e in &r.errors {
            *seen.entry(e.lines().next().unwrap_or("").to_string()).or_insert(0) += 1;
        }
        for (e, k) in &seen {
            println!("  error x{k}: {e}");
        }
        if let Some(first) = r.errors.first() {
            println!("--- the first failure ---
{first}");
        }
        println!("--- WRKSYSSTS at the peak ---\n{}", r.sys_status);
        return Ok(if r.completed == r.sessions { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    }
    let l = suite.launch();
    println!(
        "{} + {} ({})",
        l.qemu.display(),
        l.firmware.display(),
        if l.kvm { "kvm" } else { "tcg" }
    );
    match suite.run_all() {
        Ok(reports) => {
            for r in &reports {
                println!(
                    "  PASS {:<24} {:6.1}s  {}",
                    r.name,
                    r.elapsed.as_secs_f64(),
                    r.detail
                );
            }
            println!("PASS: {} steps", reports.len());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            if let Some(log) = e.log() {
                eprintln!("----- serial log -----\n{log}\n----- end serial log -----");
            }
            eprintln!("FAIL: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}
