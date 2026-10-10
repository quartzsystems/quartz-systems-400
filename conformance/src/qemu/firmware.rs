// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Locating the host's OVMF code image.

use std::env;
use std::path::PathBuf;

use crate::error::HarnessError;

/// Environment variable naming the OVMF code image.
pub const FIRMWARE_ENV: &str = "QS400_OVMF_CODE";
/// Package hint for the error message.
const HINT: &str = "Debian/Ubuntu: apt install ovmf";
/// Distribution paths tried when the environment variable is unset.
const DEFAULTS: &[&str] = &[
    "/usr/share/OVMF/OVMF_CODE_4M.fd",
    "/usr/share/OVMF/OVMF_CODE.fd",
    "/usr/share/edk2/ovmf/OVMF_CODE.fd",
    "/usr/share/edk2/x64/OVMF_CODE.4m.fd",
    "/usr/share/qemu/OVMF.fd",
];

/// The OVMF file: the environment variable, else the distribution defaults.
pub fn locate() -> Result<PathBuf, HarnessError> {
    let mut tried = Vec::new();
    let found = match env::var_os(FIRMWARE_ENV) {
        Some(p) => {
            let p = PathBuf::from(p);
            tried.push(p.clone());
            p.is_file().then_some(p)
        }
        None => DEFAULTS.iter().map(PathBuf::from).find(|p| {
            tried.push(p.clone());
            p.is_file()
        }),
    };
    found.ok_or(HarnessError::FirmwareNotFound {
        env: FIRMWARE_ENV,
        tried,
        hint: HINT,
    })
}
