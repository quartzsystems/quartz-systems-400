// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The checks over a resource table and a small sysfs and procfs tree:
//! the QEMU guest the conformance suite boots, and a Dell with a PERC.

use std::fs;
use std::path::{Path, PathBuf};

use hwqual::{Grade, Report, Roots};
use hwrsrc::{Kind, Resource, Table};

fn tree(tag: &str) -> (PathBuf, Roots) {
    let dir = std::env::temp_dir().join(format!("hwqual-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let roots = hwqual::roots_at(&dir);
    fs::create_dir_all(&roots.sys).unwrap();
    fs::create_dir_all(&roots.proc).unwrap();
    (dir, roots)
}

fn put(p: &Path, text: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn res(name: &str, kind: Kind, loc: &str, desc: &str, tm: Option<(&str, &str)>, logical: &str) -> Resource {
    let mut r = Resource::new(name.into(), kind, loc.into(), desc.into(), format!("/devices/{name}"));
    r.type_model = tm.map(|(t, m)| (t.to_string(), m.to_string()));
    r.logical = logical.into();
    r
}

fn grade(r: &Report, resource: &str) -> Grade {
    r.checks
        .iter()
        .flat_map(|(_, c)| c)
        .find(|c| c.resource == resource)
        .unwrap_or_else(|| panic!("{resource} not in {r:#?}"))
        .grade
}

fn qemu_table(disks: usize) -> Table {
    let mut t = Table::default();
    t.resources.push(res("PN01", Kind::Processor, "cpu0", "processor 0", None, ""));
    t.resources.push(res("PN02", Kind::Processor, "cpu1", "processor 1", None, ""));
    t.resources.push(res("MS01", Kind::Memory, "memory", "main storage 1024 MiB", None, ""));
    t.resources.push(res("DC01", Kind::Storage, "0000:00:1f.2", "ICH9 AHCI", Some(("QD03", "001")), ""));
    for i in 1..=disks {
        let name = format!("DD00{i}");
        let dev = format!("sd{}", char::from(b'a' + u8::try_from(i - 1).unwrap()));
        t.resources.push(res(&name, Kind::Disk, &dev, &format!("{dev} 4096 MiB at 0:0:{i}:0 on DC01"), Some(("VIRTUAL", "")), "DC01"));
    }
    let mut port = res("CMN01", Kind::Port, "0000:00:02.0", "Virtio Ethernet port eth0, link up", Some(("QN01", "001")), "");
    port.port = None;
    t.resources.push(port);
    t.resources.push(res("CMN02", Kind::Comm, "ttyS0", "serial port ttyS0", None, ""));
    t.resources.push(res("CMN03", Kind::Channel, "org.qemu.guest_agent.0", "virtual channel org.qemu.guest_agent.0 (vport1p1)", None, ""));
    t
}

#[test]
fn a_qemu_guest_without_a_tpm() {
    let (dir, roots) = tree("qemu");
    put(&roots.sys.join("class/dmi/id/sys_vendor"), "QEMU\n");
    put(&roots.sys.join("class/dmi/id/product_name"), "Standard PC (Q35 + ICH9, 2009)\n");
    fs::create_dir_all(roots.sys.join("firmware/efi")).unwrap();
    put(&roots.proc.join("cpuinfo"), "processor\t: 0\nmodel name\t: QEMU Virtual CPU version 2.5+\nflags\t\t: fpu lm hypervisor\n");
    put(&roots.proc.join("meminfo"), "MemTotal:        1012340 kB\nMemFree: 1 kB\n");
    for d in ["sda", "sdb"] {
        put(&roots.sys.join("class/block").join(d).join("size"), "8388608\n");
    }
    put(&roots.sys.join("class/graphics/fb0/name"), "simpledrmdrmfb\n");
    put(&roots.sys.join("class/graphics/fb0/virtual_size"), "1280,800\n");
    put(&roots.sys.join("class/graphics/fb0/bits_per_pixel"), "32\n");
    let r = hwqual::run(&qemu_table(2), &roots, 1_793_664_000);
    assert_eq!(r.machine, "QEMU Standard PC (Q35 + ICH9, 2009)");
    assert_eq!(r.hypervisor, "KVM/QEMU");
    assert_eq!(grade(&r, "UEFI"), Grade::Pass);
    // No SecureBoot variable, no TPM, 1 GiB, a port without carrier.
    assert_eq!(grade(&r, "SECBOOT"), Grade::Warn);
    assert_eq!(grade(&r, "TPM"), Grade::Warn);
    assert_eq!(grade(&r, "MS01"), Grade::Warn);
    assert_eq!(grade(&r, "CMN01"), Grade::Warn);
    assert_eq!(grade(&r, "DC01"), Grade::Pass);
    assert_eq!(grade(&r, "DD002"), Grade::Pass);
    assert_eq!(grade(&r, "CMN02"), Grade::Pass);
    assert_eq!(grade(&r, "CMN03"), Grade::Pass);
    assert_eq!(grade(&r, "FB0"), Grade::Pass);
    assert_eq!(r.result(), Grade::Warn);
    let text = r.text();
    assert!(text.contains("Result . . . . . . . :   WARN ("), "{text}");
    assert!(text.contains("TPM"), "{text}");
    assert!(text.contains("simpledrmdrmfb 1280x800"), "{text}");
    assert!(r.lines().iter().all(|l| l.chars().count() <= hwqual::report::WIDTH));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_dell_with_a_perc_in_hba_mode() {
    let (dir, roots) = tree("dell");
    put(&roots.sys.join("class/dmi/id/sys_vendor"), "Dell Inc.\n");
    put(&roots.sys.join("class/dmi/id/product_name"), "PowerEdge R240\n");
    fs::create_dir_all(roots.sys.join("firmware/efi")).unwrap();
    let mut sb = vec![6, 0, 0, 0, 1];
    put(&roots.sys.join("class/tpm/tpm0/tpm_version_major"), "2\n");
    fs::create_dir_all(roots.sys.join("firmware/efi/efivars")).unwrap();
    fs::write(roots.sys.join("firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"), &sb).unwrap();
    put(&roots.proc.join("cpuinfo"), "model name\t: Intel(R) Xeon(R) E-2224 CPU @ 3.40GHz\nflags\t: lm\n");
    put(&roots.proc.join("meminfo"), "MemTotal:       32768000 kB\n");
    let mut t = Table::default();
    for i in 0..4 {
        t.resources.push(res(&format!("PN0{}", i + 1), Kind::Processor, &format!("cpu{i}"), &format!("processor {i}"), None, ""));
    }
    t.resources.push(res("MS01", Kind::Memory, "memory", "main storage 32000 MiB", None, ""));
    t.resources.push(res("DC01", Kind::Storage, "0000:02:00.0", "PERC H330 Mini", Some(("QD10", "330")), ""));
    for (i, d) in ["sda", "sdb", "sdc", "sdd"].iter().enumerate() {
        t.resources.push(res(&format!("DD00{}", i + 1), Kind::Disk, d, &format!("{d} 953869 MiB at 0:0:{i}:0 on DC01"), Some(("SSD", "")), "DC01"));
        put(&roots.sys.join("class/block").join(d).join("size"), "1953525168\n");
    }
    let r = hwqual::run(&t, &roots, 1_793_664_000);
    assert_eq!(r.hypervisor, "");
    assert_eq!(grade(&r, "SECBOOT"), Grade::Pass);
    assert_eq!(grade(&r, "TPM"), Grade::Pass);
    assert_eq!(grade(&r, "DC01"), Grade::Pass);
    assert_eq!(grade(&r, "DD004"), Grade::Pass);
    assert_eq!(grade(&r, "MS01"), Grade::Pass);
    // No port, no framebuffer, no serial port: no console at all.
    assert_eq!(grade(&r, "CONSOLE"), Grade::Fail);
    assert!(r.text().contains("none (physical machine)"));
    // No guest agent line on bare metal.
    assert!(r.checks.iter().flat_map(|(_, c)| c).all(|c| c.resource != "AGENT"));

    // The same PERC in RAID mode: one volume, and no driver bound.
    sb[4] = 0;
    fs::write(roots.sys.join("firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c"), &sb).unwrap();
    // Windows takes no colon in a name: the driver check runs on Linux.
    let pci = fs::create_dir_all(roots.sys.join("bus/pci/devices/0000:02:00.0")).is_ok();
    t.resources.retain(|r| r.kind != Kind::Disk);
    t.resources.push(res("DD001", Kind::Disk, "sda", "sda 3815447 MiB at 0:2:0:0 on DC01 (PERC H330 Virtual Disk)", Some(("DISK", "")), "DC01"));
    let r = hwqual::run(&t, &roots, 0);
    assert_eq!(grade(&r, "SECBOOT"), Grade::Warn);
    if pci {
        assert_eq!(grade(&r, "DC01"), Grade::Fail);
    }
    assert_eq!(grade(&r, "DD001"), Grade::Warn);
    assert_eq!(r.result(), Grade::Fail);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn bios_boot_and_no_disks_fail() {
    let (dir, roots) = tree("bios");
    put(&roots.proc.join("meminfo"), "MemTotal:  262144 kB\n");
    let mut t = qemu_table(0);
    t.resources.retain(|r| r.kind != Kind::Channel);
    let r = hwqual::run(&t, &roots, 0);
    assert_eq!(grade(&r, "UEFI"), Grade::Fail);
    assert_eq!(grade(&r, "DD"), Grade::Fail);
    assert_eq!(grade(&r, "MS01"), Grade::Fail);
    assert_eq!(r.machine, "not reported by the firmware");
    assert_eq!(r.counts().2, 3);
    let _ = fs::remove_dir_all(dir);
}
