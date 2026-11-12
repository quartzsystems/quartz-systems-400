// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The build pipeline as a library (docs/BUILDKIT.md). A caller
//! describes its product as a [`Product`] and drives the steps: the
//! pinned kernel ([`kernel`]), the static userland ([`userland`]), the
//! initramfs, the UKIs, the disk image, the installation media and the
//! signed manifest ([`image`]), the host checks ([`host`]) and the third-party notices
//! ([`notices`]). Nothing here knows which product it builds: names,
//! environment variables, kernel fragments, the initramfs and the
//! command lines all come from the description. `x86_64` only.

pub mod ctx;
pub mod host;
pub mod image;
pub mod kernel;
pub mod notices;
pub mod product;
pub mod userland;

pub use ctx::Ctx;
pub use kernel::UkiKind;
pub use product::{Notices, Package, Product, Standalone};

use anyhow::Result;

/// The pipeline in order: fetch, kernel, userland, initramfs, UKIs,
/// disk, ISO (when the product has media), manifest. `dev` puts the
/// product's development words on the command lines.
pub fn all(ctx: &Ctx, dev: bool) -> Result<()> {
    tracing::info!("building {}", ctx.product.full());
    kernel::fetch::fetch(ctx)?;
    kernel::kernel(ctx)?;
    userland::userland(ctx)?;
    image::initramfs::initramfs(ctx)?;
    kernel::uki(ctx, dev)?;
    image::disk::disk(ctx, None)?;
    if ctx.product.media.is_some() {
        image::iso::iso(ctx)?;
    }
    image::manifest::manifest(ctx)?;
    Ok(())
}
