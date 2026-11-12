// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product description: everything the pipeline needs to know about
//! what it builds. The caller makes one [`Product`] and hands it to
//! [`Ctx::new`](crate::Ctx::new); docs/BUILDKIT.md goes through it field
//! by field.

use std::path::PathBuf;

use anyhow::Result;
use diskimg::fat;
use diskimg::layout::{LayoutGuids, PartDesc};

use crate::ctx::Ctx;
use crate::host::doctor::Row;
use crate::image::cpio::Entry;
use crate::image::iso::Media;
use crate::kernel::UkiKind;

/// One userland package: a binary built for the image target.
#[derive(Debug, Clone, Copy)]
pub struct Package {
    /// The cargo package, which is also the binary's name.
    pub name: &'static str,
    /// Whether the signed manifest lists the binary (under its name).
    pub manifest: bool,
}

/// A crate outside the workspace with its own lockfile, which `deny`
/// checks the same way under the product's `deny.toml`.
#[derive(Debug, Clone, Copy)]
pub struct Standalone {
    /// Its manifest, relative to the repository root.
    pub manifest: &'static str,
    /// `(advisory, reason)` ignored in this graph alone.
    pub ignore: &'static [(&'static str, &'static str)],
}

/// The third-party notices file of the product.
#[derive(Debug, Clone, Copy)]
pub struct Notices {
    /// Where the generated file lives, relative to the repository root.
    pub file: &'static str,
    /// The text before the list of crates, from the product's release,
    /// the kernel's version and the number of crates.
    pub intro: fn(release: &str, kernel_version: &str, crates: usize) -> String,
}

/// What goes on the installation media around the media UKI `uki`.
pub type MediaFn = fn(ctx: &Ctx, uki: &[u8]) -> Result<Media>;

/// What is being built.
pub struct Product {
    /// Lower-case identifier: artifact names (`<id>-<release>-...`), the
    /// kernel build's user and host, the extraction marker, the HTTP
    /// user agent.
    pub id: &'static str,
    /// The name as people write it, for the manifest and messages.
    pub name: &'static str,
    /// The ESP vendor directory (`EFI/<vendor>/B.EFI`) and the ISO
    /// file's prefix.
    pub vendor: &'static str,
    /// Prefix of every environment variable the pipeline reads
    /// (`<prefix>TARGET_DIR`, `<prefix>JOBS`, `<prefix>SIGNING_KEY`).
    pub env_prefix: &'static str,
    /// Prefix of the product's kernel command-line keys.
    pub key_prefix: &'static str,
    /// The repository root. The caller passes it; nothing here guesses.
    pub repo: PathBuf,
    /// The release in artifact names and the manifest.
    pub release: String,
    /// The build ID beside it.
    pub build_id: String,
    /// Kernel configuration fragments under `kernel/config/`, merged in
    /// order onto the defconfig.
    pub fragments: &'static [&'static str],
    /// Symbols the final `.config` must have `=y`, past the baseline's.
    pub required_y: &'static [&'static str],
    /// Symbols it must not have `=y`, past the baseline's.
    pub required_n: &'static [&'static str],
    /// The baked kernel command line before each UKI's own words.
    pub cmdline: &'static str,
    /// The userland packages, built together for the image target.
    pub packages: &'static [Package],
    /// The package that is `/init` in a plain initramfs
    /// ([`initramfs::plain`](crate::image::initramfs::plain)).
    pub init: &'static str,
    /// The UKIs to build, in order.
    pub kinds: &'static [UkiKind],
    /// One UKI's full command line; `dev` adds the development words.
    pub uki_cmdline: fn(product: &Product, kind: UkiKind, dev: bool) -> String,
    /// The initramfs of one UKI kind, in name order, from the built
    /// binaries ([`Paths::bin`](crate::host::paths::Paths::bin)).
    pub initramfs: fn(ctx: &Ctx, kind: UkiKind) -> Result<Vec<Entry>>,
    /// The disk image's size when none is asked for.
    pub disk_bytes: u64,
    /// The disk's partitions in order; one is the ESP (an EFI System
    /// partition with `Content::Image`), which gets both slots' UKIs.
    pub disk_parts: fn() -> Vec<PartDesc>,
    /// The disk's GUIDs: fixed, so the image is the same every build.
    pub disk_guids: fn() -> LayoutGuids,
    /// The volume label of the ESP and of the media's boot image: at
    /// most eleven characters of a FAT short name (`diskimg::fat::label`).
    pub esp_label: &'static str,
    /// Their volume id, fixed for the same reason as the GUIDs.
    pub esp_volume_id: u32,
    /// What goes on the installation media around the media UKI `uki`,
    /// for a product that has media.
    pub media: Option<MediaFn>,
    /// Rows `doctor` adds to its own.
    pub doctor: fn(ctx: &Ctx) -> Vec<Row>,
    /// Graphs `deny` checks besides the workspace.
    pub deny: &'static [Standalone],
    /// The notices file, for a product that ships one.
    pub notices: Option<Notices>,
    /// The Secure Boot db certificate the UKIs are signed for, when
    /// there is one: the manifest records its SHA-256.
    pub secure_boot_db: fn() -> Option<Vec<u8>>,
}

impl Product {
    /// The environment variable `<prefix><name>`.
    #[must_use]
    pub fn env_name(&self, name: &str) -> String {
        format!("{}{name}", self.env_prefix)
    }

    /// The value of `<prefix><name>`, when set.
    #[must_use]
    pub fn env(&self, name: &str) -> Option<std::ffi::OsString> {
        std::env::var_os(self.env_name(name))
    }

    /// The kernel command-line key `<prefix><name>`.
    #[must_use]
    pub fn key(&self, name: &str) -> String {
        format!("{}{name}", self.key_prefix)
    }

    /// `spec` as the product's volume: its label and volume id.
    pub fn fat_spec(&self, spec: fat::Spec) -> Result<fat::Spec> {
        Ok(fat::Spec {
            label: fat::label(self.esp_label)?,
            volume_id: self.esp_volume_id,
            ..spec
        })
    }

    /// `<release> build <build id>`.
    #[must_use]
    pub fn full(&self) -> String {
        format!("{} build {}", self.release, self.build_id)
    }
}
