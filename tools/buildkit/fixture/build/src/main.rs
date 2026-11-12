// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSDEMO's build orchestrator: the product description and a clap enum
//! over `buildkit`'s steps. This is all a new repository writes to get
//! the pipeline (docs/BUILDKIT.md).

use std::path::Path;

use anyhow::{Context, Result};
use buildkit::image::initramfs;
use buildkit::{Ctx, Package, Product, UkiKind};
use clap::{Parser, Subcommand};
use diskimg::layout::{Content, ESP_TYPE_GUID, LayoutGuids, PartDesc, Size};
use tracing_subscriber::EnvFilter;
use uuid::{Uuid, uuid};

// The crates an appliance takes from QS/400, linked so they are built.
use {blockdev as _, console as _, ipc as _, licpart as _, svctools as _, xfsfmt as _};

const MIB: u64 = 1024 * 1024;
/// GPT type of QSDEMO's A/B record partition, where its PID 1 keeps the
/// record (`abrec`, under the same GUID and its own magic). Generated
/// once for the fixture; a real product generates its own.
const AB_TYPE: Uuid = uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d01");
/// GPT type of QSDEMO's data partition.
const DATA_TYPE: Uuid = uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d02");

/// The release `build.rs` captured from this repository's `VERSION`.
const RELEASE: vrm::Release = vrm::release_from_env!("QSDEMO_");

/// QSDEMO, from this crate's place in its repository.
fn qsdemo() -> Result<Product> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("build crate has no parent")?
        .to_path_buf();
    Ok(Product {
        id: "qsdemo",
        name: "QSDEMO",
        vendor: "QSDEMO",
        env_prefix: "QSDEMO_",
        key_prefix: "qsdemo.",
        repo,
        release: RELEASE.to_string(),
        build_id: env!("QSDEMO_BUILD_ID").to_string(),
        fragments: &["qsdemo.config"],
        required_y: &[
            "VIRTIO_BLK",
            "XFS_FS",
            "SERIAL_8250_CONSOLE",
            "BLK_DEV_UBLK",
            "DRM_SIMPLEDRM",
        ],
        required_n: &[],
        cmdline: "console=ttyS0,115200 panic=-1",
        packages: &[Package {
            name: "qsdemo-init",
            manifest: true,
        }],
        init: "qsdemo-init",
        kinds: &[UkiKind::SlotA, UkiKind::SlotB],
        uki_cmdline: |p, kind, _dev| {
            let slot = if kind == UkiKind::SlotB { "B" } else { "A" };
            format!("{} {}={slot}", p.cmdline, p.key("slot"))
        },
        initramfs: initramfs::plain,
        disk_bytes: 1024 * MIB,
        disk_parts,
        disk_guids: || LayoutGuids::fixed(*b"QSDM", 3),
        esp_label: "QSDEMO ESP",
        esp_volume_id: 0x5153_444D,
        media: None,
        doctor: |_| Vec::new(),
        deny: &[],
        notices: None,
        secure_boot_db: || None,
    })
}

/// An ESP with both slots, the A/B record's partition (wiped: PID 1
/// writes the first copy), and one xfs data partition taking the rest.
fn disk_parts() -> Vec<PartDesc> {
    vec![
        PartDesc {
            name: "QSDEMO ESP".into(),
            type_guid: ESP_TYPE_GUID,
            size: Size::Fixed(256 * MIB),
            content: Content::Image,
        },
        PartDesc {
            name: "QSDEMO-AB".into(),
            type_guid: AB_TYPE,
            size: Size::Fixed(MIB),
            content: Content::Wiped,
        },
        PartDesc {
            name: "QSDEMO-DATA".into(),
            type_guid: DATA_TYPE,
            size: Size::Rest { min: 300 * MIB },
            content: Content::Xfs {
                label: "QSDEMODATA".into(),
                uuid: uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d03"),
            },
        },
    ]
}

#[derive(Parser)]
#[command(name = "build", about = "QSDEMO build orchestrator", version)]
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
    /// Build the userland for the musl target.
    Userland,
    /// Write the initramfs cpio.
    Initramfs,
    /// Rebuild the kernel with the initramfs embedded: the slot UKIs.
    Uki,
    /// Write the GPT disk image.
    Disk,
    /// Write and sign the artifact manifest.
    Manifest,
    /// Run the whole pipeline.
    All,
    /// Check host prerequisites.
    Doctor,
}

fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_target(false)
        .init();
    let cli = Cli::parse();
    let ctx = Ctx::new(qsdemo()?, cli.verbose)?;
    match cli.command {
        Command::Fetch => {
            buildkit::kernel::fetch::fetch(&ctx)?;
        }
        Command::Kernel => buildkit::kernel::kernel(&ctx)?,
        Command::Userland => {
            buildkit::userland::userland(&ctx)?;
        }
        Command::Initramfs => {
            initramfs::initramfs(&ctx)?;
        }
        Command::Uki => {
            buildkit::kernel::uki(&ctx, true)?;
        }
        Command::Disk => {
            buildkit::image::disk::disk(&ctx, None)?;
        }
        Command::Manifest => {
            buildkit::image::manifest::manifest(&ctx)?;
        }
        Command::All => buildkit::all(&ctx, true)?,
        Command::Doctor => buildkit::host::doctor::doctor(&ctx)?,
    }
    Ok(())
}
