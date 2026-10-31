// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 build orchestrator: `cargo run -p build -- <command>`.
//!
//! Modules by concern: `host` (running tools, paths, doctor, deny, test),
//! `kernel` (fetch, config, build), `userland` (slic), `image`
//! (cpio, initramfs, disk, manifest). QS/400 is `x86_64` only.

mod ctx;
mod host;
mod image;
mod kernel;
mod userland;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

use crate::ctx::Ctx;

#[derive(Parser)]
#[command(name = "build", about = "QS/400 build orchestrator", version)]
struct Cli {
    /// Stream tool output instead of writing it to target/logs.
    #[arg(short, long, global = true)]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download, verify and extract the pinned kernel tarball.
    Fetch,
    /// Configure and build the kernel.
    Kernel,
    /// Build slic for the musl target; verify it is static and stripped.
    Userland,
    /// Write the deterministic newc cpio.
    Initramfs,
    /// Rebuild the kernel with the initramfs embedded; emit the slot A and
    /// slot B UKIs.
    Uki {
        /// Omit the serial service console (`qs400.svc=1`).
        #[arg(long)]
        release: bool,
    },
    /// Write the GPT disk image: two-slot ESP, LIC and ASP1 partitions.
    Disk {
        /// Image size in bytes (default 4 GiB).
        #[arg(long)]
        size: Option<u64>,
    },
    /// Write the UEFI-bootable installation ISO.
    Iso,
    /// Write and sign the artifact manifest.
    Manifest,
    /// Package each product of `licpgm/products/` into a signed product
    /// load save file under `target/images/lp/`.
    Lp,
    /// Run the whole pipeline.
    All {
        /// Omit the serial service console (`qs400.svc=1`).
        #[arg(long)]
        release: bool,
    },
    /// Boot under QEMU and run the suite: `--media disk` (default) or `iso`.
    Test {
        /// What to boot.
        #[arg(long, default_value = "disk")]
        media: conformance::Media,
        /// With `--media iso`: install onto a blank disk, then boot it.
        #[arg(long)]
        install: bool,
        /// Objects in the bench step (disk suite).
        #[arg(long, default_value_t = 20_000)]
        bench: u64,
        /// Power pulls in the disk suite; 0 skips them.
        #[arg(long, default_value_t = 3)]
        pulls: u32,
    },
    /// Check host prerequisites.
    Doctor,
    /// Run cargo-deny plus the C-linkage lockfile check.
    Deny,
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let ctx = Ctx::new(cli.verbose)?;
    if !cfg!(target_os = "linux") && !matches!(cli.command, Command::Doctor | Command::Deny) {
        tracing::warn!("the pipeline needs a Linux host; run this from WSL");
    }

    match cli.command {
        Command::Fetch => {
            kernel::fetch::fetch(&ctx)?;
        }
        Command::Kernel => kernel::kernel(&ctx)?,
        Command::Userland => {
            userland::userland(&ctx)?;
        }
        Command::Initramfs => {
            image::initramfs::initramfs(&ctx)?;
        }
        Command::Uki { release } => {
            kernel::uki(&ctx, release)?;
        }
        Command::Disk { size } => {
            image::disk::disk(&ctx, size)?;
        }
        Command::Iso => {
            image::media::iso(&ctx)?;
        }
        Command::Manifest => {
            image::manifest::manifest(&ctx)?;
        }
        Command::Lp => {
            image::lp::lp(&ctx)?;
        }
        Command::All { release } => all(&ctx, release)?,
        Command::Test {
            media,
            install,
            bench,
            pulls,
        } => host::testrun::test(&ctx, media, install, bench, pulls)?,
        Command::Doctor => host::doctor::doctor(&ctx)?,
        Command::Deny => host::deny::deny(&ctx)?,
    }
    Ok(())
}

/// The full pipeline in order.
fn all(ctx: &Ctx, release: bool) -> Result<()> {
    tracing::info!("building {}", ::release::full());
    kernel::fetch::fetch(ctx)?;
    kernel::kernel(ctx)?;
    userland::userland(ctx)?;
    image::initramfs::initramfs(ctx)?;
    kernel::uki(ctx, release)?;
    image::disk::disk(ctx, None)?;
    image::media::iso(ctx)?;
    image::manifest::manifest(ctx)?;
    Ok(())
}
