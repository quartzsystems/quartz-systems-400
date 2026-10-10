// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 conformance harness, Phase 0: boot a disk image under QEMU with
//! OVMF and assert the IPL and heartbeat SRC codes on serial.
//!
//! OVMF is test-only host firmware standing in for a machine's UEFI; it
//! never ships in an image. QS/400 is `x86_64` only.

#![forbid(unsafe_code)]

mod boot;
mod error;
mod qemu;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use boot::{BootReport, BootTest, SRC_HEARTBEAT, SRC_IPL_STARTED};
pub use error::HarnessError;
pub use qemu::firmware;

/// The disk image the orchestrator last wrote, found through
/// `QS400_TARGET_DIR` or the workspace `target/` directory.
pub fn default_image() -> Result<PathBuf, HarnessError> {
    let target = env::var_os("QS400_TARGET_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .map_or_else(|| PathBuf::from("target"), |p| p.join("target"))
        },
        PathBuf::from,
    );
    let pointer = target.join("images").join("disk.path");
    let text = fs::read_to_string(&pointer).map_err(|_| HarnessError::ImageMissing(pointer))?;
    let image = PathBuf::from(text.trim());
    if image.is_file() {
        Ok(image)
    } else {
        Err(HarnessError::ImageMissing(image))
    }
}
