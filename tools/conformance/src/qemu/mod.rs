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
    /// A writable copy of OVMF's variable store: `BootNext`, `BootOrder`
    /// and `Boot####` persist across resets (the LIC PTF step). The
    /// firmware keeps no variables when `None`.
    pub vars: Option<PathBuf>,
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
    /// More forwards on that network: host port, guest port (the host
    /// servers).
    pub host_ports: Vec<(u16, u16)>,
    /// Guest memory, MiB.
    pub memory_mb: u32,
    /// Guest CPUs.
    pub cpus: u32,
    /// A virtio VGA display at this size instead of the standard VGA:
    /// the firmware's GOP mode follows the device's preferred size.
    pub display: Option<(u32, u32)>,
    /// A virtio SCSI controller (`scsi0`) with no disk on it, for a disk
    /// hot-added through QMP (step 83).
    pub scsi: bool,
    /// A network boot first: QEMU's TFTP server over this directory, the
    /// DHCP server naming this boot file (install step 15).
    pub tftp: Option<(PathBuf, String)>,
    /// A virtio-serial port named `org.qemu.guest_agent.0` on this Unix
    /// socket (QEMU the server), as libvirt and Proxmox give a guest agent.
    pub guest_agent: Option<PathBuf>,
    /// A TPM 2.0 (`tpm-crb`) over this swtpm control socket.
    pub tpm: Option<PathBuf>,
    /// A USB stick (an xHCI controller and `usb-storage`) with this image.
    pub usb_stick: Option<PathBuf>,
    /// Disks on the q35 AHCI controller (SATA), beside the virtio ones.
    pub sata_disks: Vec<PathBuf>,
    /// The PCI slot of the telnet NIC (`n0`) on the root bus, QEMU's
    /// choice when `None`.
    pub nic_slot: Option<u8>,
    /// More virtio NICs, each on a user network of its own (step 159).
    pub nics: Vec<Nic>,
}

/// A further virtio NIC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nic {
    /// Its QEMU netdev and device id.
    pub id: String,
    /// Its PCI slot on the root bus.
    pub slot: u8,
    /// Its MAC address.
    pub mac: String,
    /// Its user network, `10.0.3.0/24`.
    pub net: String,
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

/// The QEMU argument list. `q35`, standard VGA (pinned) unless a display
/// size is asked for, a virtio keyboard,
/// serial on stdio, no reboot suppression (restart tests need it), QMP on a
/// Unix socket.
#[allow(clippy::too_many_lines)]
pub fn args(l: &Launch) -> Vec<OsString> {
    let mut a: Vec<OsString> = ["-machine", "q35"].iter().map(OsString::from).collect();
    if l.kvm {
        a.extend(["-enable-kvm", "-cpu", "host"].iter().map(OsString::from));
    }
    a.extend(["-m".into(), l.memory_mb.to_string().into()]);
    a.extend([
        "-smp".into(),
        l.cpus.to_string().into(),
        OsString::from("-drive"),
    ]);
    a.push(join("if=pflash,format=raw,readonly=on,file=", &l.firmware));
    if let Some(vars) = &l.vars {
        a.push(OsString::from("-drive"));
        a.push(join("if=pflash,format=raw,file=", vars));
    }
    let (disk_index, cd_index) = if l.cdrom_first { (1, 0) } else { (0, 1) };
    // A network boot goes before both.
    let (disk_index, cd_index) = if l.tftp.is_some() {
        (disk_index + 1, cd_index + 1)
    } else {
        (disk_index, cd_index)
    };
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
    match l.display {
        Some((w, h)) => a.extend([
            OsString::from("-vga"),
            OsString::from("none"),
            OsString::from("-device"),
            OsString::from(format!("virtio-vga,xres={w},yres={h}")),
        ]),
        None => a.extend(["-vga", "std"].iter().map(OsString::from)),
    }
    if l.scsi {
        a.push(OsString::from("-device"));
        a.push(OsString::from("virtio-scsi-pci,id=scsi0"));
    }
    extras(l, &mut a);
    a.extend(
        [
            "-rtc",
            "base=utc",
            "-uuid",
            GUEST_UUID,
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
            "user,id=n0,net=10.0.2.0/24,dhcpstart={},hostfwd=tcp:127.0.0.1:{port}-10.0.2.15:23{}",
            crate::netauto::DHCP_FIRST,
            l.host_ports.iter().fold(String::new(), |mut s, (h, g)| {
                use std::fmt::Write as _;
                let _ = write!(s, ",hostfwd=tcp:127.0.0.1:{h}-10.0.2.15:{g}");
                s
            }) + &l
                .tftp
                .as_ref()
                .map_or_else(String::new, |(dir, file)| format!(
                    ",tftp={},bootfile={file}",
                    dir.display()
                ))
        )));
        a.push(OsString::from("-device"));
        let slot = l
            .nic_slot
            .map_or_else(String::new, |s| format!(",bus=pcie.0,addr=0x{s:02x}"));
        a.push(OsString::from(if l.tftp.is_some() {
            format!("virtio-net-pci,netdev=n0,bootindex=0{slot}")
        } else {
            format!("virtio-net-pci,netdev=n0{slot}")
        }));
    } else {
        a.push(OsString::from("-nic"));
        a.push(OsString::from("none"));
    }
    for n in &l.nics {
        a.push(OsString::from("-netdev"));
        a.push(OsString::from(format!("user,id={},net={}", n.id, n.net)));
        a.push(OsString::from("-device"));
        a.push(OsString::from(format!(
            "virtio-net-pci,netdev={},mac={},bus=pcie.0,addr=0x{:02x}",
            n.id, n.mac, n.slot
        )));
    }
    a
}

fn join(prefix: &str, path: &Path) -> OsString {
    let mut s = OsString::from(prefix);
    s.push(path);
    s
}

/// The devices a hypervisor's management stack adds: SATA disks on the
/// q35 AHCI controller, the guest agent's virtio-serial port, a TPM, a
/// USB stick.
fn extras(l: &Launch, a: &mut Vec<OsString>) {
    for (i, disk) in l.sata_disks.iter().enumerate() {
        a.push(OsString::from("-drive"));
        a.push(join(&format!("if=none,id=sata{i},format=raw,file="), disk));
        a.push(OsString::from("-device"));
        a.push(OsString::from(format!(
            "ide-hd,drive=sata{i},bus=ide.{},serial=QS400SD{i}",
            i + 2
        )));
    }
    if let Some(sock) = &l.guest_agent {
        a.push(OsString::from("-chardev"));
        a.push(join("socket,id=qga0,server=on,wait=off,path=", sock));
        a.extend(
            [
                "-device",
                "virtio-serial-pci",
                "-device",
                "virtserialport,chardev=qga0,name=org.qemu.guest_agent.0",
            ]
            .iter()
            .map(OsString::from),
        );
    }
    if let Some(sock) = &l.tpm {
        a.push(OsString::from("-chardev"));
        a.push(join("socket,id=chrtpm,path=", sock));
        a.extend(
            [
                "-tpmdev",
                "emulator,id=tpm0,chardev=chrtpm",
                "-device",
                "tpm-crb,tpmdev=tpm0",
            ]
            .iter()
            .map(OsString::from),
        );
    }
    if let Some(img) = &l.usb_stick {
        a.push(OsString::from("-drive"));
        a.push(join("if=none,id=stick,format=raw,file=", img));
        a.extend(
            [
                "-device",
                "qemu-xhci,id=xhci",
                "-device",
                "usb-storage,bus=xhci.0,drive=stick,removable=on",
            ]
            .iter()
            .map(OsString::from),
        );
    }
}
