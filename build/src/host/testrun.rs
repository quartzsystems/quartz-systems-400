// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build test`: boot the disk image with the conformance harness.

use anyhow::{Context, Result};
use tracing::{error, info};

use crate::ctx::Ctx;
use crate::host::paths;

/// Run the Phase 0 boot test.
pub fn test(ctx: &Ctx) -> Result<()> {
    let image = paths::read_pointer(&ctx.paths.disk_pointer())
        .context("no disk image; run `disk` first")?;
    let test = conformance::BootTest::new(image)?;
    info!(
        "test: {} with {} ({})",
        test.qemu.display(),
        test.firmware.display(),
        if test.kvm { "kvm" } else { "tcg" }
    );
    match test.run() {
        Ok(report) => {
            info!(
                "test: PASS. IPL SRC after {:.1}s, {} heartbeats, {:.1}s total",
                report.ipl_after.as_secs_f64(),
                report.heartbeats,
                report.elapsed.as_secs_f64()
            );
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
