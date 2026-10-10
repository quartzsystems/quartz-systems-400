// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Locating QEMU, choosing acceleration and building the command line.

pub mod firmware;
pub mod serial;

use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::boot::BootTest;
use crate::error::HarnessError;

/// The system emulator binary.
pub const QEMU_BINARY: &str = "qemu-system-x86_64";
/// Environment override for the QEMU binary path.
pub const QEMU_ENV: &str = "QS400_QEMU";

/// Find the system emulator: the override variable, else `PATH`.
pub fn locate() -> Result<PathBuf, HarnessError> {
    if let Some(p) = env::var_os(QEMU_ENV) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let path = env::var_os("PATH").unwrap_or_default();
    env::split_paths(&path)
        .map(|d| d.join(QEMU_BINARY))
        .find(|p| p.is_file())
        .ok_or(HarnessError::QemuNotFound {
            binary: QEMU_BINARY,
            env: QEMU_ENV,
        })
}

/// KVM is usable when the host is `x86_64`, `/dev/kvm` opens read-write
/// and `QS400_NO_KVM` is unset.
pub fn kvm_available() -> bool {
    cfg!(target_arch = "x86_64")
        && env::var_os("QS400_NO_KVM").is_none()
        && OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/kvm")
            .is_ok()
}

/// The QEMU argument list for one test.
pub fn args(test: &BootTest) -> Vec<OsString> {
    let mut a: Vec<OsString> = ["-machine", "q35"].iter().map(OsString::from).collect();
    if test.kvm {
        a.extend(["-enable-kvm", "-cpu", "host"].iter().map(OsString::from));
    }
    a.extend(
        ["-m", "1024", "-smp", "2", "-drive"]
            .iter()
            .map(OsString::from),
    );
    a.push(join(
        "if=pflash,format=raw,readonly=on,file=",
        &test.firmware,
    ));
    a.push(OsString::from("-drive"));
    a.push(join("if=virtio,format=raw,file=", &test.image));
    // -snapshot: OVMF without a vars flash writes NvVars onto the ESP; keep
    // each boot's writes in a temporary overlay so the image stays pristine.
    a.extend(
        [
            "-snapshot",
            "-nic",
            "none",
            "-nographic",
            "-serial",
            "stdio",
            "-monitor",
            "none",
            "-no-reboot",
        ]
        .iter()
        .map(OsString::from),
    );
    a
}

fn join(prefix: &str, path: &Path) -> OsString {
    let mut s = OsString::from(prefix);
    s.push(path);
    s
}
