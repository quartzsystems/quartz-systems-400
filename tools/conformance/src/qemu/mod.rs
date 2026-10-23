// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Locating QEMU, choosing acceleration, building the command line, and
//! talking to a running guest.

pub mod firmware;
#[cfg(unix)]
pub mod qmp;
pub mod serial;
pub mod session;

use std::env;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use crate::error::HarnessError;

/// The system emulator binary.
pub const QEMU_BINARY: &str = "qemu-system-x86_64";
/// Environment override for the QEMU binary path.
pub const QEMU_ENV: &str = "QS400_QEMU";
/// Fixed SMBIOS UUID so QSRLNBR is deterministic in tests.
pub const GUEST_UUID: &str = "51533430-0000-4000-8000-000000000400";

/// How to launch the guest.
#[derive(Debug, Clone)]
pub struct Launch {
    /// QEMU binary.
    pub qemu: PathBuf,
    /// OVMF code image.
    pub firmware: PathBuf,
    /// Use KVM.
    pub kvm: bool,
    /// Disk image (a working copy; boots write to it), if attached.
    pub image: Option<PathBuf>,
    /// ISO attached as a CD-ROM, if any.
    pub cdrom: Option<PathBuf>,
    /// Boot the CD-ROM before the disk. Expressed as `bootindex`
    /// properties on the devices (OVMF honours those through `fw_cfg`;
    /// `-boot order=` is not reliable with virtio disks).
    pub cdrom_first: bool,
    /// Further disks, never booted: `/dev/vdb`, `/dev/vdc`, ... in the guest.
    pub extra_disks: Vec<PathBuf>,
    /// QMP Unix socket path.
    pub qmp: PathBuf,
    /// A user-mode network with the guest's telnet port forwarded to
    /// this host port; no network device when `None`.
    pub telnet_port: Option<u16>,
}

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

/// The QEMU argument list. `q35`, standard VGA (pinned), a virtio keyboard,
/// serial on stdio, no reboot suppression (restart tests need it), QMP on a
/// Unix socket.
pub fn args(l: &Launch) -> Vec<OsString> {
    let mut a: Vec<OsString> = ["-machine", "q35"].iter().map(OsString::from).collect();
    if l.kvm {
        a.extend(["-enable-kvm", "-cpu", "host"].iter().map(OsString::from));
    }
    a.extend(
        ["-m", "1024", "-smp", "2", "-drive"]
            .iter()
            .map(OsString::from),
    );
    a.push(join("if=pflash,format=raw,readonly=on,file=", &l.firmware));
    let (disk_index, cd_index) = if l.cdrom_first { (1, 0) } else { (0, 1) };
    if let Some(image) = &l.image {
        a.push(OsString::from("-drive"));
        a.push(join("if=none,id=hd,format=raw,file=", image));
        a.push(OsString::from("-device"));
        a.push(OsString::from(format!(
            "virtio-blk-pci,drive=hd,serial=QS400HD0,bootindex={disk_index}"
        )));
    }
    for (i, disk) in l.extra_disks.iter().enumerate() {
        a.push(OsString::from("-drive"));
        a.push(join(&format!("if=none,id=extra{i},format=raw,file="), disk));
        a.push(OsString::from("-device"));
        a.push(OsString::from(format!(
            "virtio-blk-pci,drive=extra{i},serial=QS400HD{}",
            i + 1
        )));
    }
    if let Some(cdrom) = &l.cdrom {
        a.push(OsString::from("-drive"));
        a.push(join("if=none,id=cd,media=cdrom,format=raw,file=", cdrom));
        a.push(OsString::from("-device"));
        a.push(OsString::from(format!(
            "ide-cd,drive=cd,bus=ide.0,bootindex={cd_index}"
        )));
    }
    a.extend(
        [
            "-rtc",
            "base=utc",
            "-uuid",
            GUEST_UUID,
            "-vga",
            "std",
            "-device",
            "virtio-keyboard-pci",
            "-nographic",
            "-serial",
            "stdio",
            "-monitor",
            "none",
            "-qmp",
        ]
        .iter()
        .map(OsString::from),
    );
    let mut qmp = OsString::from("unix:");
    qmp.push(&l.qmp);
    qmp.push(",server,nowait");
    a.push(qmp);
    if let Some(port) = l.telnet_port {
        a.push(OsString::from("-netdev"));
        a.push(OsString::from(format!(
            "user,id=n0,net=10.0.2.0/24,hostfwd=tcp:127.0.0.1:{port}-:23"
        )));
        a.push(OsString::from("-device"));
        a.push(OsString::from("virtio-net-pci,netdev=n0"));
    } else {
        a.push(OsString::from("-nic"));
        a.push(OsString::from("none"));
    }
    a
}

fn join(prefix: &str, path: &Path) -> OsString {
    let mut s = OsString::from(prefix);
    s.push(path);
    s
}
