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
