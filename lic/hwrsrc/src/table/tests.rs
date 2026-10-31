// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use std::os::unix::fs::symlink;
use std::path::PathBuf;

use super::*;

/// A PCI function under `parent`, linked from `bus/pci/devices`.
fn function(root: &Path, parent: &str, addr: &str, ids: [&str; 5]) -> PathBuf {
    let dir = root.join(parent).join(addr);
    fs::create_dir_all(&dir).unwrap();
    let attrs = ["vendor", "device", "class", "subsystem_vendor", "subsystem_device"];
    for (attr, v) in attrs.iter().zip(ids) {
        fs::write(dir.join(attr), format!("0x{v}\n")).unwrap();
    }
    symlink(&dir, root.join("bus/pci/devices").join(addr)).unwrap();
    dir
}

/// A disk at `under/<hctl path>/block/<name>`, linked from `class/block`.
fn disk(root: &Path, under: &Path, scsi: &str, name: &str, mib: u64) -> PathBuf {
    let dir = under.join(scsi).join("block").join(name);
    fs::create_dir_all(dir.join("device")).unwrap();
    fs::write(dir.join("size"), format!("{}\n", mib * 2048)).unwrap();
    fs::write(dir.join("device/type"), "0\n").unwrap();
    symlink(&dir, root.join("class/block").join(name)).unwrap();
    dir
}

fn fresh_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("qs400-sysfs-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
    fs::create_dir_all(root.join("class/block")).unwrap();
    root
}

fn names(t: &Table) -> Vec<(&str, &str)> {
    t.resources
        .iter()
        .map(|r| (r.name.as_str(), r.location.as_str()))
        .collect()
}

#[test]
fn disks_behind_a_perc_are_named_by_scsi_address_under_their_controller() {
    let root = fresh_root("perc");
    let top = "devices/pci0000:00";
    function(&root, top, "0000:00:01.0", ["8086", "6f02", "060400", "8086", "0000"]);
    let ahci = function(&root, top, "0000:00:1f.2", ["8086", "2922", "010601", "1af4", "1100"]);
    let perc = function(
        &root,
        &format!("{top}/0000:00:01.0"),
        "0000:02:00.0",
        ["1000", "005f", "010400", "1028", "1f4b"],
    );
    // The kernel's names in another order than the SCSI addresses.
    for (t, name) in [(3, "sda"), (1, "sdb"), (0, "sdc"), (2, "sdd")] {
        disk(&root, &perc, &format!("host0/target0:0:{t}/0:0:{t}:0"), name, 953_869);
    }
    let sda = fs::canonicalize(root.join("class/block/sda")).unwrap();
    fs::create_dir_all(sda.join("sda1")).unwrap();
    fs::write(sda.join("sda1/partition"), "1\n").unwrap();
    symlink(sda.join("sda1"), root.join("class/block/sda1")).unwrap();
    fs::write(sda.join("device/wwid"), "naa.5000c500a1b2c3d4\n").unwrap();
    disk(&root, &ahci, "ata1/host1/target1:0:0/1:0:0:0", "sde", 4096);

    let t = Table::scan(&root);
    assert_eq!(
        names(&t),
        [
            ("CMB01", "0000:00:01.0"),
            ("DC01", "0000:00:1f.2"),
            ("DC02", "0000:02:00.0"),
            ("DD001", "sde"),
            ("DD002", "sdc"),
            ("DD003", "sdb"),
            ("DD004", "sdd"),
            ("DD005", "sda"),
        ]
    );
    let perc = &t.resources[2];
    assert_eq!(perc.type_model, Some(("QD10".into(), "330".into())));
    assert_eq!(perc.description, "1000:005f PERC H330 Mini");
    assert_eq!(perc.parent, "CMB01");
    assert_eq!(t.resources[1].type_model(), ("QD03".into(), "001".into()));
    assert_eq!(t.controller_of("sdc").map(|r| r.name.as_str()), Some("DC02"));
    assert_eq!(t.controller_of("sde").map(|r| r.name.as_str()), Some("DC01"));
    assert_eq!(t.resources[4].description, "sdc 953869 MiB at 0:0:0:0 on DC02");
    assert_eq!(t.resources[4].logical, "DC02");
    assert_eq!(t.get("dd005").unwrap().serial, "naa.5000c500a1b2c3d4");
    assert_eq!(t.path_to("DD002", false), ["CMB01", "DC02", "DD002"]);
    let dump = t.dump();
    assert!(dump.contains("DC02      QD10-330"), "{dump}");

    // The storage list: the bridge above the PERC, its units under it.
    let rows: Vec<(String, usize)> = t
        .tree(|r| matches!(r.kind, Kind::Storage | Kind::Disk), true, true)
        .iter()
        .map(|r| (t.resources[r.index].name.clone(), r.depth))
        .collect();
    let want = [
        ("CMB01", 0),
        ("DC02", 1),
        ("DD002", 2),
        ("DD003", 2),
        ("DD004", 2),
        ("DD005", 2),
        ("DC01", 0),
        ("DD001", 1),
    ];
    let want: Vec<(String, usize)> = want.iter().map(|(n, d)| ((*n).to_string(), *d)).collect();
    assert_eq!(rows, want);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_hot_added_disk_takes_the_next_name_and_a_removed_one_is_not_detected() {
    let root = fresh_root("hotplug");
    let top = "devices/pci0000:00";
    let a = function(&root, top, "0000:00:03.0", ["1af4", "1001", "010000", "1af4", "0002"]);
    disk(&root, &a, "virtio1", "vda", 2048);
    let mut t = Table::scan(&root);
    assert_eq!(names(&t), [("DC01", "0000:00:03.0"), ("DD001", "vda")]);

    // A second controller at a lower address with its disk: the old
    // names stay, the new ones come next.
    let b = function(&root, top, "0000:00:02.0", ["1af4", "1001", "010000", "1af4", "0002"]);
    disk(&root, &b, "virtio2", "vdb", 1024);
    let ev = Uevent::parse(b"add@/devices/pci0000:00/0000:00:02.0/virtio2/block/vdb\0SUBSYSTEM=block\0").unwrap();
    let changes = t.apply_uevent(&root, &ev);
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(
        names(&t),
        [("DC01", "0000:00:03.0"), ("DD001", "vda"), ("DC02", "0000:00:02.0"), ("DD002", "vdb")]
    );
    assert_eq!(t.get("DD002").unwrap().logical, "DC02");
    assert!(t.get("DD002").unwrap().description.ends_with("on DC02"));

    // Removed: still listed, not detected; back: the same name.
    fs::remove_file(root.join("class/block/vdb")).unwrap();
    let ev = Uevent::parse(b"remove@/devices/pci0000:00/0000:00:02.0/virtio2/block/vdb\0SUBSYSTEM=block\0").unwrap();
    t.apply_uevent(&root, &ev);
    assert_eq!(t.get("DD002").unwrap().status, Status::NotDetected);
    symlink(b.join("virtio2/block/vdb"), root.join("class/block/vdb")).unwrap();
    t.rescan(&root);
    assert_eq!(t.get("DD002").unwrap().status, Status::Operational);
    assert_eq!(t.resources.len(), 4);
    // An event of another subsystem changes nothing.
    let ev = Uevent::parse(b"add@/devices/virtual/misc/foo\0SUBSYSTEM=misc\0").unwrap();
    assert!(t.apply_uevent(&root, &ev).is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ports_are_cmn_in_pci_order_before_serial_ports() {
    let root = nic::tests::fake_sysfs(
        "table",
        &[
            ("0000:02:00.0", "e1000e", "eth0", 0, None),
            ("0000:01:00.0", "virtio_net", "eth1", 0, Some("virtio0")),
        ],
    );
    let mut t = Table::default();
    t.add(
        Kind::Lan,
        "0000:01:00.0".into(),
        "virtio network".into(),
        "/devices/pci0000:00/0000:01:00.0".into(),
    );
    t.add_ports(&root);
    t.add(
        Kind::Comm,
        "ttyS0".into(),
        "serial port ttyS0".into(),
        "/devices/platform/serial8250/tty/ttyS0".into(),
    );
    t.link();
    assert_eq!(
        names(&t),
        [
            ("LIN01", "0000:01:00.0"),
            ("CMN01", "0000:01:00.0"),
            ("CMN02", "0000:02:00.0"),
            ("CMN03", "ttyS0"),
        ]
    );
    assert_eq!(t.resources[0].type_model, Some(("QN01".into(), "001".into())));
    assert_eq!(t.resources[2].type_model, Some(("QN03".into(), "001".into())));
    assert_eq!(t.resources[1].kind, Kind::Port);
    assert_eq!(t.resources[1].logical, "LIN01");
    assert_eq!(t.resources[1].parent, "LIN01");
    assert_eq!(t.resources[1].serial, "525400000001");
    let dump = t.dump();
    assert!(dump.contains("CMN01     QN01-001"), "{dump}");
    assert!(dump.contains("Virtio Ethernet port eth1"), "{dump}");

    // Carrier changes show after a refresh.
    fs::write(root.join("class/net/eth1/carrier"), "0\n").unwrap();
    t.refresh_ports(&root);
    assert!(t.resources[1].description.ends_with("no link"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn processors_and_main_storage() {
    let root = fresh_root("cpu");
    for c in ["cpu1", "cpu0", "cpufreq"] {
        fs::create_dir_all(root.join("devices/system/cpu").join(c)).unwrap();
    }
    let mem = root.join("devices/system/memory");
    for b in ["memory0", "memory1"] {
        fs::create_dir_all(mem.join(b)).unwrap();
        fs::write(mem.join(b).join("online"), "1\n").unwrap();
    }
    fs::write(mem.join("block_size_bytes"), "8000000\n").unwrap();
    let t = Table::scan(&root);
    assert_eq!(names(&t), [("PN01", "cpu0"), ("PN02", "cpu1"), ("MS01", "memory")]);
    assert_eq!(t.resources[2].description, "main storage 256 MiB");
    fs::remove_dir_all(root).unwrap();
}
