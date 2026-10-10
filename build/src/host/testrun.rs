// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build test`: run the conformance suite against the disk image.

use anyhow::{Context, Result};
use tracing::{error, info};

use crate::ctx::Ctx;
use crate::host::paths;

/// Run the Phase 1 suite for `media`.
pub fn test(ctx: &Ctx, media: conformance::Media) -> Result<()> {
    let image = paths::read_pointer(&ctx.paths.disk_pointer())
        .context("no disk image; run `disk` first")?;
    let iso = paths::read_pointer(&ctx.paths.iso_pointer()).ok();
    let suite = conformance::Suite::new(
        Some(&image),
        iso.as_deref(),
        media,
        ctx.paths.target.join("conformance"),
    )?;
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
