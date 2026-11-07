// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::scan::nic::tests::{Nic, add_nic, remove_nic};
use crate::table::Removal;

fn root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("qs400-names-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("class/net")).unwrap();
    fs::create_dir_all(root.join("bus/pci/devices")).unwrap();
    root
}

fn card(root: &Path, addr: &str, ifname: &str, mac: &str) {
    add_nic(
        root,
        &Nic {
            addr,
            driver: "e1000e",
            ifname,
            dev_port: 0,
            sub: None,
            mac,
            device: "10d3",
        },
    );
}

/// The name, location and status of every resource of a prefix.
fn of(t: &Table, prefix: &str) -> Vec<(String, String, Status)> {
    t.resources
        .iter()
        .filter(|r| r.name.starts_with(prefix))
        .map(|r| (r.name.clone(), r.location.clone(), r.status))
        .collect()
}

fn row(name: &str, loc: &str, s: Status) -> (String, String, Status) {
    (name.into(), loc.into(), s)
}

/// An IPL: the walk named by the map, the map saved and read back.
fn ipl(root: &Path, map: &mut NameMap) -> Table {
    ipl_with(root, map, &PermMacs::new())
}

fn ipl_with(root: &Path, map: &mut NameMap, perm: &PermMacs) -> Table {
    let (t, _) = Table::scan_named(root, perm, map);
    *map = NameMap::decode(&NameMap::of(&t).encode()).unwrap();
    t
}

const A: &str = "52:54:00:aa:00:01";
const B: &str = "52:54:00:bb:00:02";
const C: &str = "52:54:00:cc:00:03";
use Status::{NotDetected as Gone, Operational as Up};

#[test]
fn a_card_added_before_the_others_takes_the_next_name() {
    let root = root("added");
    card(&root, "0000:02:00.0", "eth0", A);
    card(&root, "0000:03:00.0", "eth1", B);
    let mut map = NameMap::default();
    let t = ipl(&root, &mut map);
    assert_eq!(
        of(&t, "CMN"),
        [
            row("CMN01", "0000:02:00.0", Up),
            row("CMN02", "0000:03:00.0", Up)
        ]
    );
    // A new card at a lower PCI address: in PCI order it comes first.
    card(&root, "0000:01:00.0", "eth2", C);
    let t = ipl(&root, &mut map);
    assert_eq!(
        of(&t, "CMN"),
        [
            row("CMN03", "0000:01:00.0", Up),
            row("CMN01", "0000:02:00.0", Up),
            row("CMN02", "0000:03:00.0", Up)
        ]
    );
    let lin: Vec<_> = of(&t, "LIN").into_iter().map(|r| (r.0, r.1)).collect();
    assert_eq!(
        lin,
        [
            ("LIN03".to_string(), "0000:01:00.0".to_string()),
            ("LIN01".into(), "0000:02:00.0".into()),
            ("LIN02".into(), "0000:03:00.0".into())
        ]
    );
    assert_eq!(
        t.get("CMN01").unwrap().port.as_ref().unwrap().ifname,
        "eth0"
    );
    assert_eq!(t.get("CMN01").unwrap().logical, "LIN01");
    // The same at every IPL after.
    let again = ipl(&root, &mut map);
    assert_eq!(of(&again, "CMN"), of(&t, "CMN"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_card_removed_keeps_its_name_until_removed() {
    let root = root("removed");
    card(&root, "0000:02:00.0", "eth0", A);
    card(&root, "0000:03:00.0", "eth1", B);
    let mut map = NameMap::default();
    let _ = ipl(&root, &mut map);
    remove_nic(&root, "0000:02:00.0");
    let (t, changes) = Table::scan_named(&root, &PermMacs::new(), &map);
    assert!(
        changes.contains(&"CMN01 not detected (0000:02:00.0)".to_string()),
        "{changes:?}"
    );
    assert_eq!(
        of(&t, "CMN"),
        [
            row("CMN02", "0000:03:00.0", Up),
            row("CMN01", "0000:02:00.0", Gone)
        ]
    );
    let gone = t.get("CMN01").unwrap();
    assert_eq!(gone.type_model(), ("QN03".into(), "001".into()));
    assert_eq!(
        (gone.parent.as_str(), gone.logical.as_str()),
        ("LIN01", "LIN01")
    );
    assert!(gone.port.is_none());
    assert_eq!(t.get("LIN01").unwrap().status, Gone);
    // The list shows it under its adapter, not detected.
    let rows = crate::view::list(
        &t,
        crate::view::spec("*CMN").unwrap(),
        &mut crate::view::ids,
    );
    let at = rows.iter().position(|r| r.name == "CMN01").unwrap();
    assert_eq!(rows[at].status, "TXT0151");
    assert_eq!(rows[at - 1].name, "LIN01");
    map = NameMap::of(&t);
    // Another card does not take the name of the one gone.
    card(&root, "0000:04:00.0", "eth2", C);
    let mut t = ipl(&root, &mut map);
    assert_eq!(t.get("CMN03").unwrap().location, "0000:04:00.0");
    assert_eq!(t.get("CMN01").unwrap().status, Gone);
    // Remove: only one that does not report.
    assert_eq!(t.remove("CMN02"), Removal::Reporting);
    assert_eq!(t.remove("CMN99"), Removal::NotFound);
    assert_eq!(t.remove("cmn01"), Removal::Removed);
    assert!(t.get("CMN01").is_none());
    map = NameMap::of(&t);
    assert!(!map.entries.iter().any(|e| e.name == "CMN01"));
    let t = ipl(&root, &mut map);
    assert!(t.get("CMN01").is_none());
    assert_eq!(t.get("LIN01").unwrap().status, Gone);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_card_moved_to_another_slot_keeps_its_names() {
    let root = root("moved");
    card(&root, "0000:02:00.0", "eth0", A);
    card(&root, "0000:03:00.0", "eth1", B);
    let mut map = NameMap::default();
    let _ = ipl(&root, &mut map);
    remove_nic(&root, "0000:02:00.0");
    card(&root, "0000:05:00.0", "eth0", A);
    let t = ipl(&root, &mut map);
    assert_eq!(
        of(&t, "CMN"),
        [
            row("CMN02", "0000:03:00.0", Up),
            row("CMN01", "0000:05:00.0", Up)
        ]
    );
    assert_eq!(t.get("LIN01").unwrap().location, "0000:05:00.0");
    assert!(t.resources.iter().all(Resource::operational));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_port_is_known_by_its_permanent_address() {
    let root = root("perm");
    card(&root, "0000:02:00.0", "eth0", A);
    let mut map = NameMap::default();
    let _ = ipl(&root, &mut map);
    assert_eq!(
        map.entries.iter().find(|e| e.name == "CMN01").unwrap().key,
        "mac:525400AA0001"
    );
    // Software set another address (ADPTADR): netlink still tells the
    // card's own, and the port keeps its name even in another slot.
    remove_nic(&root, "0000:02:00.0");
    card(&root, "0000:06:00.0", "eth0", "02:00:00:00:00:99");
    let net = root.join("class/net/eth0");
    fs::write(net.join("addr_assign_type"), "3\n").unwrap();
    let perm: PermMacs = [("eth0".to_string(), A.to_string())].into();
    let t = ipl_with(&root, &mut map, &perm);
    assert_eq!(t.get("CMN01").unwrap().location, "0000:06:00.0");
    assert_eq!(t.get("CMN01").unwrap().status, Up);
    // A random address is no identity: the port is known by its place.
    fs::write(net.join("addr_assign_type"), "1\n").unwrap();
    let t = ipl(&root, &mut map);
    let r = t.get("CMN02").unwrap();
    assert_eq!((r.key.as_str(), r.status), ("pci:0000:06:00.0", Up));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_card_plugged_back_while_up_is_the_one_it_was() {
    let root = root("hotplug");
    card(&root, "0000:02:00.0", "eth0", A);
    card(&root, "0000:03:00.0", "eth1", B);
    let mut map = NameMap::default();
    let mut t = ipl(&root, &mut map);
    remove_nic(&root, "0000:02:00.0");
    let changes = t.rescan(&root);
    assert!(
        changes.contains(
            &"CMN01 not detected (/devices/pci0000:00/0000:02:00.0/net/eth0)".to_string()
        ),
        "{changes:?}"
    );
    // Back in another slot: its own name, detected again.
    card(&root, "0000:07:00.0", "eth0", A);
    let changes = t.rescan(&root);
    assert!(
        changes
            .iter()
            .any(|c| c.starts_with("CMN01 detected again")),
        "{changes:?}"
    );
    assert_eq!(t.get("CMN01").unwrap().location, "0000:07:00.0");
    assert_eq!(t.get("CMN01").unwrap().status, Up);
    // Another card in a slot a gone one had is new.
    remove_nic(&root, "0000:03:00.0");
    t.rescan(&root);
    card(&root, "0000:03:00.0", "eth1", C);
    t.rescan(&root);
    assert_eq!(t.get("CMN02").unwrap().status, Gone);
    assert_eq!(t.get("CMN03").unwrap().location, "0000:03:00.0");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_saved_names_round_trip_and_refuse_damage() {
    let root = root("format");
    card(&root, "0000:02:00.0", "eth0", A);
    let (t, _) = Table::scan_named(&root, &PermMacs::new(), &NameMap::default());
    let map = NameMap::of(&t);
    let bytes = map.encode();
    assert_eq!(&bytes[..4], b"QSHN");
    assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), VERSION);
    assert_eq!(NameMap::decode(&bytes).unwrap(), map);
    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert_eq!(NameMap::decode(&bad), Err(FormatError::Magic));
    let mut bad = bytes.clone();
    bad[4] = 9;
    assert_eq!(NameMap::decode(&bad), Err(FormatError::Version(9)));
    assert_eq!(
        NameMap::decode(&bytes[..bytes.len() - 1]),
        Err(FormatError::Truncated)
    );
    let mut bad = bytes.clone();
    bad.push(0);
    assert_eq!(NameMap::decode(&bad), Err(FormatError::Trailing));
    // Too many to keep: the first ones are, within the limit.
    let e = map.entries[0].clone();
    let big = NameMap {
        entries: (0..2000)
            .map(|i| Entry {
                name: format!("DD{i:03}"),
                key: format!("sn:{}", "X".repeat(150)),
                ..e.clone()
            })
            .collect(),
    };
    let enc = big.encode();
    assert!(enc.len() <= MAX_BYTES);
    let back = NameMap::decode(&enc).unwrap();
    assert!(back.entries.len() > 100 && back.entries.len() < 2000);
    assert_eq!(back.entries[0], big.entries[0]);
    fs::remove_dir_all(root).unwrap();
}
