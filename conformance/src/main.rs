// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `conformance [--image <disk.img>]`

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;
use conformance::BootTest;

#[derive(Parser)]
#[command(
    name = "conformance",
    about = "Boot a QS/400 image under QEMU and check the SRC codes"
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
    let test = BootTest::new(image)?;
    println!(
        "{} + {} ({})",
        test.qemu.display(),
        test.firmware.display(),
        if test.kvm { "kvm" } else { "tcg" }
    );
    match test.run() {
        Ok(r) => {
            println!(
                "PASS: IPL SRC after {:.1}s, {} heartbeats, {:.1}s total",
                r.ipl_after.as_secs_f64(),
                r.heartbeats,
                r.elapsed.as_secs_f64()
            );
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
