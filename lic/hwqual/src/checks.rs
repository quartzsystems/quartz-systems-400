// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The checks. Each section reads the resource table (its names, types
//! and parents, `WRKHDWRSC`'s) and, where the table says nothing, a
//! file of sysfs or procfs. The limits are QS/400's own
//! (`docs/QUALIFIED_HARDWARE.md`, "What QSHWQUAL checks").

use std::fs;
use std::path::Path;

use hwrsrc::dmi::Dmi;
use hwrsrc::{Kind, Resource, Table};

use crate::Roots;
use crate::report::{Check, Grade, Report, timestamp};

/// Main storage below this fails.
pub const MIN_MEMORY_MIB: u64 = 512;
/// Main storage below this warns.
pub const WARN_MEMORY_MIB: u64 = 2048;
/// The EFI global variable `SecureBoot`.
const SECURE_BOOT: &str = "firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// What `QSHWQUAL` and the installer say when Secure Boot is on.
pub const SECURE_BOOT_WARNING: &str =
    "WARN: QS/400 V1R1M0 is not Secure Boot signed; disable Secure Boot or enrol the QS/400 key";

/// Whether the firmware enforces Secure Boot, from the `SecureBoot`
/// variable under `sys` (four attribute bytes, then the value); `None`
/// when it cannot be read.
#[must_use]
pub fn secure_boot(sys: &Path) -> Option<bool> {
    fs::read(sys.join(SECURE_BOOT))
        .ok()
        .and_then(|b| b.get(4).map(|v| *v == 1))
}

/// Storage controller types with a driver QS/400 names (`QD00` is a
/// class QS/400 recognises but has no table entry for).
const UNKNOWN_CONTROLLER: &str = "QD00";
/// The virtio-serial port name of the guest agent's channel.
const AGENT_PORT: &str = "org.qemu.guest_agent.0";

fn read(p: &Path) -> String {
    fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn tm(r: &Resource) -> String {
    hwrsrc::view::type_model(r)
}

fn of(t: &Table, k: Kind) -> impl Iterator<Item = &Resource> {
    t.resources
        .iter()
        .filter(move |r| r.kind == k && r.operational())
}

/// Every check, in sections.
#[must_use]
pub fn all(t: &Table, roots: &Roots, now: u64) -> Report {
    let dmi = Dmi::read_from(&roots.sys.join("class/dmi/id"));
    let hypervisor = hypervisor(&dmi, roots);
    let machine = [dmi.vendor.as_str(), dmi.product.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    Report {
        machine: if machine.is_empty() {
            "not reported by the firmware".into()
        } else {
            machine
        },
        when: timestamp(now),
        checks: vec![
            ("Firmware and security".into(), firmware(roots, &hypervisor)),
            ("Processors and main storage".into(), processors(t, roots)),
            (
                "Storage controllers and disk units".into(),
                storage(t, roots),
            ),
            ("Communications".into(), network(t, &hypervisor)),
            ("Console".into(), console(t, roots)),
        ],
        hypervisor,
    }
}

/// The hypervisor by the CPU's flag and the firmware's identity.
#[must_use]
pub fn hypervisor(dmi: &Dmi, roots: &Roots) -> String {
    let cpuinfo = read(&roots.proc.join("cpuinfo"));
    let flagged = cpuinfo
        .lines()
        .any(|l| l.starts_with("flags") && l.split_whitespace().any(|f| f == "hypervisor"));
    let xen = read(&roots.sys.join("hypervisor/type"));
    let v = dmi.vendor.to_ascii_lowercase();
    let p = dmi.product.to_ascii_lowercase();
    if !xen.is_empty() {
        return xen.to_ascii_uppercase();
    }
    if qsvs(dmi, roots) {
        return QSVS.to_string();
    }
    let named = if v.contains("qemu") || p.contains("kvm") || p.contains("standard pc") {
        "KVM/QEMU"
    } else if v.contains("vmware") || p.contains("vmware") {
        "VMware"
    } else if v.contains("microsoft") && p.contains("virtual") {
        "Microsoft Hyper-V"
    } else if p.contains("virtualbox") || v.contains("innotek") {
        "Oracle VirtualBox"
    } else if flagged {
        "unidentified hypervisor"
    } else {
        return String::new();
    };
    named.to_string()
}

/// QS/VS, as [`hypervisor`] names it.
pub const QSVS: &str = "QS/VS";
/// The OEM ID of every ACPI table QS/VS gives a partition.
const QSVS_OEM_ID: &[u8; 6] = b"QUARTZ";
/// Their OEM table ID.
const QSVS_OEM_TABLE_ID: &[u8; 8] = b"QSVSPART";
/// The FADT's hypervisor vendor identity under QS/VS.
const QSVS_HV_ID: &[u8; 8] = b"QUARTZVS";
/// Where the FADT keeps the hypervisor vendor identity (ACPI 6.0).
const FADT_HV_ID: usize = 268;

/// Whether the system is a partition of QS/VS. Its ACPI tables say so
/// (the FADT's hypervisor vendor identity, or the OEM IDs of every
/// table), and so would SMBIOS tables naming it, should it give any.
#[must_use]
pub fn qsvs(dmi: &Dmi, roots: &Roots) -> bool {
    let v = dmi.vendor.to_ascii_lowercase();
    let p = dmi.product.to_ascii_lowercase();
    if p.contains("qs/vs") || (v.contains("quartz") && p.contains("qsvs")) {
        return true;
    }
    let dir = roots.sys.join("firmware/acpi/tables");
    let Ok(entries) = fs::read_dir(&dir) else {
        return false;
    };
    let tables: Vec<Vec<u8>> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| fs::read(e.path()).ok())
        .filter(|b| b.len() >= 36)
        .collect();
    let fadt_says = tables.iter().any(|t| {
        t.starts_with(b"FACP") && t.get(FADT_HV_ID..FADT_HV_ID + 8) == Some(&QSVS_HV_ID[..])
    });
    let oem = |t: &Vec<u8>| t[10..16] == QSVS_OEM_ID[..] && t[16..24] == QSVS_OEM_TABLE_ID[..];
    fadt_says || (!tables.is_empty() && tables.iter().all(oem))
}

fn firmware(roots: &Roots, hypervisor: &str) -> Vec<Check> {
    let mut v = Vec::new();
    let efi = roots.sys.join("firmware/efi");
    if efi.is_dir() {
        v.push(Check::new(
            Grade::Pass,
            "UEFI",
            "",
            "UEFI firmware: QS/400 boots its UKI from the EFI system partition.",
        ));
        v.push(match secure_boot(&roots.sys) {
            Some(true) => Check::new(Grade::Warn, "SECBOOT", "", SECURE_BOOT_WARNING),
            Some(false) => Check::new(
                Grade::Pass,
                "SECBOOT",
                "",
                "Secure Boot is disabled, as QS/400 V1R1M0 boots.",
            ),
            None => Check::new(
                Grade::Warn,
                "SECBOOT",
                "",
                "The Secure Boot state cannot be read from the firmware.",
            ),
        });
    } else if hypervisor == QSVS {
        v.push(Check::new(
            Grade::Pass,
            "UEFI",
            "",
            "No firmware: a partition of QS/VS, which loads the UKI of the IPL's side itself.",
        ));
    } else {
        v.push(Check::new(
            Grade::Fail,
            "UEFI",
            "",
            "Legacy BIOS boot: QS/400 needs UEFI firmware. Switch the machine or VM to UEFI (OVMF, Gen2).",
        ));
    }
    let tpm = roots.sys.join("class/tpm/tpm0");
    if tpm.exists() {
        let major = read(&tpm.join("tpm_version_major"));
        let ver = if major.is_empty() {
            "TPM".to_string()
        } else {
            format!("TPM {major}.0")
        };
        let grade = if major == "1" {
            Grade::Warn
        } else {
            Grade::Pass
        };
        let note = if major == "1" {
            ": QS/400 uses TPM 2.0 features; a 1.2 part is only reported"
        } else {
            ""
        };
        v.push(Check::new(
            grade,
            "TPM",
            "",
            format!("{ver} present{note}."),
        ));
    } else {
        v.push(Check::new(
            Grade::Warn,
            "TPM",
            "",
            "No TPM was found: QS/400 runs without one, but cannot seal keys to this machine. Add a TPM 2.0 (swtpm for a VM).",
        ));
    }
    v
}

fn processors(t: &Table, roots: &Roots) -> Vec<Check> {
    let cpuinfo = read(&roots.proc.join("cpuinfo"));
    let model = cpuinfo
        .lines()
        .find_map(|l| {
            l.strip_prefix("model name")
                .and_then(|r| r.split_once(':'))
                .map(|(_, m)| m.trim().to_string())
        })
        .unwrap_or_default();
    let mut v = Vec::new();
    let cpus: Vec<&Resource> = of(t, Kind::Processor).collect();
    for r in &cpus {
        let text = if model.is_empty() {
            r.description.clone()
        } else {
            format!("{}: {model}", r.description)
        };
        v.push(Check::new(Grade::Pass, &r.name, &tm(r), text));
    }
    match cpus.len() {
        0 => v.push(Check::new(Grade::Fail, "PN", "", "No processor was found in sysfs.")),
        1 => v.push(Check::new(
            Grade::Warn,
            &cpus[0].name,
            "",
            "One processor: jobs, the storage service and the console share it. Give the system two or more.",
        )),
        _ => {}
    }
    let mem_kib: u64 = read(&roots.proc.join("meminfo"))
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    let mib = mem_kib / 1024;
    let ms = of(t, Kind::Memory)
        .next()
        .map_or(("MS01".to_string(), "QM01-001".to_string()), |r| {
            (r.name.clone(), tm(r))
        });
    v.push(if mib == 0 {
        Check::new(Grade::Warn, &ms.0, &ms.1, "The size of main storage cannot be read.")
    } else if mib < MIN_MEMORY_MIB {
        Check::new(Grade::Fail, &ms.0, &ms.1, format!("Main storage {mib} MiB: QS/400 needs at least {MIN_MEMORY_MIB} MiB."))
    } else if mib < WARN_MEMORY_MIB {
        Check::new(
            Grade::Warn,
            &ms.0,
            &ms.1,
            format!("Main storage {mib} MiB: enough to IPL; {WARN_MEMORY_MIB} MiB or more for interactive and batch work."),
        )
    } else {
        Check::new(Grade::Pass, &ms.0, &ms.1, format!("Main storage {mib} MiB."))
    });
    v
}

/// The kernel driver bound to PCI function `addr`, empty when none.
fn driver(sys: &Path, addr: &str) -> String {
    fs::read_link(sys.join("bus/pci/devices").join(addr).join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// Whether a disk is a RAID controller's logical volume, not a disk.
fn raid_volume(r: &Resource) -> bool {
    let d = r.description.to_ascii_uppercase();
    ["PERC", "VIRTUAL DISK", "LOGICAL VOLUME", "MR9", "RAID"]
        .iter()
        .any(|w| d.contains(w))
}

fn storage(t: &Table, roots: &Roots) -> Vec<Check> {
    let mut v = Vec::new();
    for c in of(t, Kind::Storage) {
        let (ty, _) = c.type_model();
        let drv = driver(&roots.sys, &c.location);
        let disks = t
            .resources
            .iter()
            .filter(|d| d.kind == Kind::Disk && d.logical == c.name)
            .count();
        let bound = if drv.is_empty() {
            String::new()
        } else {
            format!(", driver {drv}")
        };
        let text = format!("{}{bound}, {disks} disk units", c.description);
        v.push(
            if drv.is_empty() && roots.sys.join("bus/pci/devices").join(&c.location).exists() {
                Check::new(
                    Grade::Fail,
                    &c.name,
                    &tm(c),
                    format!("{text}: no driver took it; its disks are out of reach."),
                )
            } else if ty == UNKNOWN_CONTROLLER {
                Check::new(
                    Grade::Warn,
                    &c.name,
                    &tm(c),
                    format!("{text}: a controller QS/400 has not been qualified with."),
                )
            } else {
                Check::new(Grade::Pass, &c.name, &tm(c), text)
            },
        );
    }
    let disks: Vec<&Resource> = of(t, Kind::Disk).collect();
    for d in &disks {
        let bytes = read(&roots.sys.join("class/block").join(&d.location).join("size"))
            .parse::<u64>()
            .unwrap_or(0)
            * 512;
        v.push(if raid_volume(d) {
            // The controller is the disk's logical parent.
            let ctl = t
                .resources
                .iter()
                .find(|c| c.kind == Kind::Storage && c.name == d.logical)
                .map_or_else(|| "an unnamed controller".to_string(), |c| format!("controller {} ({})", c.name, c.description));
            Check::new(
                Grade::Warn,
                &d.name,
                &tm(d),
                format!(
                    "{}: a RAID volume of {ctl}. Put that controller in HBA mode so QS/400 sees each disk and protects them itself.",
                    d.description
                ),
            )
        } else if bytes > 0 && bytes < diskimg::MIN_DISK_BYTES {
            Check::new(Grade::Warn, &d.name, &tm(d), format!("{}: too small for a load source.", d.description))
        } else {
            Check::new(Grade::Pass, &d.name, &tm(d), d.description.clone())
        });
    }
    match disks.len() {
        0 => v.push(Check::new(
            Grade::Fail,
            "DD",
            "",
            "No disk unit: QS/400 needs one for its load source.",
        )),
        1 => v.push(Check::new(
            Grade::Warn,
            &disks[0].name,
            "",
            "One disk unit: mirrored protection needs a second unit of at least the same size.",
        )),
        _ => {}
    }
    for o in of(t, Kind::Optical)
        .chain(of(t, Kind::Tape))
        .chain(of(t, Kind::TapeLibrary))
    {
        v.push(Check::new(
            Grade::Info,
            &o.name,
            &tm(o),
            o.description.clone(),
        ));
    }
    v
}

fn network(t: &Table, hypervisor: &str) -> Vec<Check> {
    let mut v = Vec::new();
    let ports: Vec<&Resource> = of(t, Kind::Port).collect();
    for p in &ports {
        let up = p.port.as_ref().and_then(|n| n.carrier) == Some(true);
        v.push(if up {
            Check::new(Grade::Pass, &p.name, &tm(p), p.description.clone())
        } else {
            Check::new(
                Grade::Warn,
                &p.name,
                &tm(p),
                format!(
                    "{}: connect the port before configuring TCP/IP on it.",
                    p.description
                ),
            )
        });
    }
    if ports.is_empty() {
        v.push(Check::new(
            Grade::Warn,
            "CMN",
            "",
            "No Ethernet port: the system works only from its console.",
        ));
    }
    let channels: Vec<&Resource> = of(t, Kind::Channel).collect();
    for c in &channels {
        let what = if c.location == AGENT_PORT {
            "the guest agent's channel"
        } else {
            "a hypervisor channel"
        };
        v.push(Check::new(
            Grade::Pass,
            &c.name,
            &tm(c),
            format!("{}: {what}.", c.description),
        ));
    }
    if !hypervisor.is_empty() && !channels.iter().any(|c| c.location == AGENT_PORT) {
        v.push(Check::new(
            Grade::Warn,
            "AGENT",
            "",
            "No guest agent channel: add a virtio-serial port named org.qemu.guest_agent.0 so the hypervisor can see and power down the system.",
        ));
    }
    v
}

fn console(t: &Table, roots: &Roots) -> Vec<Check> {
    let mut v = Vec::new();
    let serial: Vec<&Resource> = of(t, Kind::Comm)
        .filter(|r| r.location.starts_with("ttyS"))
        .collect();
    for s in &serial {
        v.push(Check::new(
            Grade::Pass,
            &s.name,
            &tm(s),
            format!("{}: the service console runs here.", s.description),
        ));
    }
    let fb = roots.sys.join("class/graphics/fb0");
    if fb.exists() {
        let name = read(&fb.join("name"));
        let size = read(&fb.join("virtual_size")).replace(',', "x");
        let bpp = read(&fb.join("bits_per_pixel"));
        v.push(Check::new(
            Grade::Pass,
            "FB0",
            "",
            format!("Framebuffer {name} {size} at {bpp} bits: the console display (5250 screen) runs here."),
        ));
    } else {
        v.push(Check::new(
            Grade::Warn,
            "FB0",
            "",
            "No framebuffer: the firmware handed over no display, so the console runs on the serial port alone.",
        ));
    }
    if serial.is_empty() && !fb.exists() {
        v.push(Check::new(
            Grade::Fail,
            "CONSOLE",
            "",
            "Neither a serial port nor a display: there is no console.",
        ));
    }
    for k in of(t, Kind::Keyboard) {
        v.push(Check::new(
            Grade::Info,
            &k.name,
            &tm(k),
            k.description.clone(),
        ));
    }
    v
}
