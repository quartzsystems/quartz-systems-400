// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installation media as the booted system finds it (docs/INSTALL.md):
//!
//! - [`locate`]: the block device that holds the media, mounted
//!   read-only for as long as a caller reads it. `slic` uses it too.
//! - [`load`]: the media UKI's `/init`. That UKI's initramfs holds this
//!   loader and no more, so the boot image stays far under El Torito's
//!   32 MiB; `slic` and the job program are files of the media, checked
//!   against the signed manifest and then run.

#![deny(missing_docs)]

#[cfg(target_os = "linux")]
pub mod load;
#[cfg(target_os = "linux")]
pub mod locate;

pub use optical::manifest::Manifest;
