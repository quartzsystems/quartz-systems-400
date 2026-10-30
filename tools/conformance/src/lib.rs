// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400 conformance harness, Phase 1: boot a disk image under QEMU with
//! OVMF and drive it over serial and QMP through the six Phase 1 steps.
//!
//! OVMF is test-only host firmware standing in for a machine's UEFI; it
//! never ships in an image. QS/400 is `x86_64` only.

#![forbid(unsafe_code)]

pub mod cl;
pub mod cmdkit;
mod cmn;
pub mod db;
pub mod ifsapi;
mod dst;
mod error;
mod ifs;
mod load;
mod phase2;
mod phase5;
mod phase6;
mod phase7;
mod phase8;
mod phase9;
#[cfg(unix)]
mod png;
mod qemu;
pub mod qir;
pub mod rpg;
mod servers;
mod steps;
mod tcp;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub use error::HarnessError;
pub use phase2::SYSTEM_LIBRARIES;
pub use qemu::firmware;
pub use qemu::session::{PROMPT, Session};
pub use steps::{Media, SRC_SEQUENCE, SRC_SEQUENCE_MEDIA, StepReport, Suite};

/// The target directory: `QS400_TARGET_DIR` or the workspace `target/`.
pub fn target_dir() -> PathBuf {
    env::var_os("QS400_TARGET_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .map_or_else(|| PathBuf::from("target"), |p| p.join("target"))
        },
        PathBuf::from,
    )
}

/// The ISO the orchestrator last wrote.
pub fn default_iso() -> Result<PathBuf, HarnessError> {
    pointed("iso.path")
}

/// The disk image the orchestrator last wrote.
pub fn default_image() -> Result<PathBuf, HarnessError> {
    pointed("disk.path")
}

fn pointed(name: &str) -> Result<PathBuf, HarnessError> {
    let pointer = target_dir().join("images").join(name);
    let text = fs::read_to_string(&pointer).map_err(|_| HarnessError::ImageMissing(pointer))?;
    let image = PathBuf::from(text.trim());
    if image.is_file() {
        Ok(image)
    } else {
        Err(HarnessError::ImageMissing(image))
    }
}
