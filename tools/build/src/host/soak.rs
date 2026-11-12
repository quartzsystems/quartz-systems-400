// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build soak`: boot the disk image under QEMU and keep it under load
//! for a while (`conformance::soak`). The report goes to the log and to
//! `target/conformance/soak-report.txt`.

use std::time::Duration;

use anyhow::{Context, Result};
use tracing::info;

use buildkit::Ctx;
use buildkit::host::paths;

/// Run the soak for `secs` seconds.
pub fn soak(ctx: &Ctx, secs: u64) -> Result<()> {
    let image = paths::read_pointer(&ctx.paths.disk_pointer())
        .context("no disk image; run `disk` first")?;
    let suite = conformance::Suite::new(
        Some(&image),
        None,
        conformance::Media::Disk,
        ctx.paths.target.join("conformance"),
    )?;
    info!("soak: {secs}s of load");
    let r = suite.run_soak(Duration::from_secs(secs))?;
    for l in r.render().lines() {
        info!("{l}");
    }
    if r.failures.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("soak: {} failure(s)", r.failures.len())
    }
}
