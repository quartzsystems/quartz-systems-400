// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `conformance [--image <disk.img>]`: run the Phase 1 suite.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use conformance::Suite;

#[derive(Parser)]
#[command(
    name = "conformance",
    about = "Boot a QS/400 image under QEMU and run the Phase 1 steps"
)]
struct Cli {
    /// Disk image. Defaults to the orchestrator's latest.
    #[arg(long)]
    image: Option<PathBuf>,
}

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let image = match cli.image {
        Some(p) => p,
        None => conformance::default_image()?,
    };
    let suite = Suite::new(&image, conformance::target_dir().join("conformance"))?;
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
