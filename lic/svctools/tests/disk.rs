// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The disk menus against a fixed environment: every screen matches
//! its golden file, and the actions reach the environment.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::{DiskUnit, MemEnv, NonConfigured, ParityUnit};
use svctools::panel::Runner;
use svctools::users::Privilege;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace")
        .join("tools")
        .join("conformance")
        .join("golden")
        .join("dst")
}

fn check(name: &str, dump: &str) {
    let path = golden_dir().join(format!("{name}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(golden_dir()).unwrap();
        fs::write(&path, dump).unwrap();
        return;
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with QS400_UPDATE_GOLDEN=1", path.display()));
    let want = want.replace("\r\n", "\n");
    assert_eq!(dump, want, "screen {name} differs from {}", path.display());
}

fn f(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn unit(n: u16, mirrored: bool, partner: Option<u16>, status: &str, attached: bool) -> DiskUnit {
    DiskUnit {
        asp: 1,
        unit: n,
        serial: format!("QM0000{n}"),
        type_model: "QEMU HARDDISK".into(),
        resource: format!("DD00{}", n + 1),
        size_mib: 4096,
        percent_used: 12 + u8::try_from(n).unwrap(),
        status: status.into(),
        mirrored,
        partner,
        attached,
        resume_percent: None,
        device: format!("/dev/vd{}", (b'a' + u8::try_from(n).unwrap()) as char),
        controller: "DC01".into(),
    }
}

fn spare(name: &str, size: u64) -> NonConfigured {
    NonConfigured {
        device: format!("/dev/{name}"),
        resource: "DD003".into(),
        serial: format!("SP-{name}"),
        type_model: "QEMU HARDDISK".into(),
        size_mib: size,
        has_data: false,
        controller: "DC02".into(),
    }
}

/// An environment with a pair, an unprotected unit and a spare disk.
fn env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    e.units = vec![
        unit(0, true, Some(1), "Active", true),
        unit(1, true, Some(0), "Suspended", true),
        unit(2, false, None, "Unprotected", true),
    ];
    e.spare = vec![spare("vdd", 4096), spare("vde", 2048)];
    e
}

fn runner() -> Runner {
    Runner::new(Box::new(svctools::dst::menu::MainMenu::new(
        "QSECOFR".into(),
        Privilege::ALL,
    )))
}

/// Walk a path of selections from the main menu and check the screen
/// at the end.
fn walk(env: &mut MemEnv, path: &[&str], name: &str) -> Runner {
    let mut r = runner();
    for sel in path {
        r.enter(&f(&[sel]), env);
    }
    check(name, &r.screen(env).dump());
    r
}

#[test]
fn disk_menus_match_their_golden_files() {
    let mut e = env();
    walk(&mut e, &["4"], "disk");
    walk(&mut e, &["4", "1"], "disk-config");
    walk(&mut e, &["4", "1", "1"], "disk-display");
    walk(&mut e, &["4", "1", "1", "1"], "disk-status");
    walk(&mut e, &["4", "1", "1", "2"], "disk-capacity");
    walk(&mut e, &["4", "1", "1", "3"], "disk-protection");
    walk(&mut e, &["4", "1", "1", "4"], "disk-nonconfigured");
    walk(&mut e, &["4", "1", "2"], "asp-threshold");
    walk(&mut e, &["4", "1", "3"], "asp-config");
    walk(&mut e, &["4", "1", "3", "2"], "add-units");
    walk(&mut e, &["4", "1", "3", "4"], "remove-units");
    walk(&mut e, &["4", "1", "4"], "mirror");
    walk(&mut e, &["4", "1", "4", "2"], "mirror-start");
    walk(&mut e, &["4", "1", "4", "3"], "mirror-stop");
    walk(&mut e, &["4", "1", "5"], "disk-parity");
    walk(&mut e, &["4", "2"], "disk-recovery");
    walk(&mut e, &["4", "2", "3"], "replace-unit");
    walk(&mut e, &["4", "2", "6"], "disk-procedures");
    walk(&mut e, &["4", "2", "7"], "suspend");
    walk(&mut e, &["4", "2", "8"], "resume");
    // Confirmations.
    let mut r = walk(&mut e, &["4", "1", "3", "2"], "add-units");
    r.enter(&f(&["1", ""]), &mut e);
    check("add-units-confirm", &r.screen(&mut e).dump());
    let mut r = walk(&mut e, &["4", "1", "4", "2"], "mirror-start");
    r.enter(&f(&["1"]), &mut e);
    check("mirror-start-confirm", &r.screen(&mut e).dump());
}

#[test]
fn add_units_needs_a_load_source() {
    let mut e = env();
    e.units.clear();
    let mut r = walk(&mut e, &["4", "1", "3"], "asp-config");
    r.enter(&f(&["2"]), &mut e);
    assert_eq!(r.top(), "dst-asp-config");
    assert!(r.screen(&mut e).dump().contains("no load source unit"));
    assert!(e.actions.is_empty());
}

#[test]
fn actions_reach_the_environment() {
    let mut e = env();
    // Add the first spare disk.
    let mut r = walk(&mut e, &["4", "1", "3", "2"], "add-units");
    r.enter(&f(&["1", ""]), &mut e);
    assert_eq!(r.top(), "dst-add-units-confirm");
    r.function(10, &mut e);
    assert_eq!(r.top(), "dst-asp-config");
    assert!(r.screen(&mut e).dump().contains("added to ASP 1"));
    assert_eq!(e.actions, vec!["add /dev/vdd"]);
    // Start mirroring: unit 3 (unprotected, 4096 MiB) pairs with vdd.
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "1", "4", "2"], "mirror-start");
    r.enter(&f(&["1"]), &mut e);
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["mirror 2 /dev/vdd"]);
    assert!(r.screen(&mut e).dump().contains("next IPL"));
    // Resume the suspended half.
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "2", "8"], "resume");
    r.enter(&f(&["1"]), &mut e);
    assert_eq!(e.actions, vec!["resume 1"]);
    assert_eq!(r.top(), "dst-disk-recovery");
    // Replace it instead: unit 2 is stale; vdd fits, vde is too small.
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "2", "3"], "replace-unit");
    r.enter(&f(&["1"]), &mut e);
    assert_eq!(r.top(), "dst-replace-with");
    assert_eq!(
        r.screen(&mut e).fields.len(),
        1,
        "only the 4096 MiB spare fits"
    );
    r.enter(&f(&["1"]), &mut e);
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["replace 1 /dev/vdd", "resume 1"]);
    assert_eq!(r.top(), "dst-disk-recovery");
    // Operators cannot change the configuration.
    let mut r = Runner::new(Box::new(svctools::dst::menu::MainMenu::new(
        "22222222".into(),
        Privilege::DISK_UNITS_OPERATIONS,
    )));
    r.enter(&f(&["4"]), &mut e);
    r.enter(&f(&["1"]), &mut e);
    r.enter(&f(&["3"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("not authorized"));
    assert_eq!(r.top(), "dst-disk-config");
    // Threshold.
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "1", "2"], "asp-threshold");
    r.enter(&f(&["95"]), &mut e);
    assert_eq!(e.actions, vec!["threshold 95"]);
    assert_eq!(r.top(), "dst-disk-config");
}

#[test]
fn units_holding_data_pair_with_each_other() {
    let mut e = MemEnv::new("QS400");
    e.units = (0..4)
        .map(|n| unit(n, false, None, "Unprotected", true))
        .collect();
    let mut r = walk(&mut e, &["4", "1", "4", "2"], "mirror-start");
    r.enter(&f(&["1"]), &mut e);
    check("mirror-start-confirm-units", &r.screen(&mut e).dump());
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["mirror 0 /dev/vdd", "mirror 1 /dev/vdc"]);
    assert!(r.screen(&mut e).dump().contains("2 pair(s) formed"));
    // Three units: one would be left without a partner.
    e.units.pop();
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "1", "4", "2"], "mirror-start");
    r.enter(&f(&["1"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("even number"));
    assert!(e.actions.is_empty());
}

/// Four units in a RAID 5 set (unit 3 failed, being replaced) and two
/// unprotected units, with the virtual parity controller.
fn parity_env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    let status = ["DPY/Unprotected", "DPY/Unprotected", "DPY/Unprotected", "DPY/Failed"];
    e.units = (0..4)
        .map(|n| unit(n, false, None, status[usize::from(n)], n != 3))
        .chain((4..6).map(|n| unit(n, false, None, "Unprotected", true)))
        .collect();
    e.controllers = vec!["DC03      QDPY-001  Software device parity controller".into()];
    e.parity = (0..4)
        .map(|n| ParityUnit {
            set: 1,
            level: "RAID 5".into(),
            unit: n,
            serial: format!("QM0000{n}"),
            type_model: "QEMU HARDDISK".into(),
            resource: format!("DD00{}", n + 1),
            controller: "DC03".into(),
            status: status[usize::from(n)].into(),
            percent: None,
        })
        .collect();
    e.spare = vec![spare("vdg", 4096)];
    e
}

#[test]
fn parity_screens_match_their_golden_files() {
    let mut e = parity_env();
    walk(&mut e, &["4", "1", "5"], "parity");
    walk(&mut e, &["4", "1", "5", "1"], "parity-status");
    walk(&mut e, &["4", "1", "1", "1"], "disk-status-parity");
    walk(&mut e, &["4", "1", "1", "3"], "disk-protection-parity");
    walk(&mut e, &["4", "2", "3"], "replace-unit-parity");
    let mut r = walk(&mut e, &["4", "1", "5", "2"], "parity-start5");
    r.enter(&f(&["1", "1"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("needs 3 to 18 units"));
    walk(&mut e, &["4", "1", "5", "3"], "parity-start6");
    let mut r = walk(&mut e, &["4", "1", "5", "4"], "parity-stop");
    r.enter(&f(&["1"]), &mut e);
    check("parity-stop-confirm", &r.screen(&mut e).dump());
    let mut r = walk(&mut e, &["4", "1", "5", "5"], "parity-include");
    r.enter(&f(&["", "1"]), &mut e);
    check("parity-include-confirm", &r.screen(&mut e).dump());
    let mut r = walk(&mut e, &["4", "1", "5", "6"], "parity-exclude");
    r.enter(&f(&["1", "", "", ""]), &mut e);
    check("parity-exclude-confirm", &r.screen(&mut e).dump());
    // Start over three of three eligible units (the env's two plus one).
    e.units.push(unit(6, false, None, "Unprotected", true));
    let mut r = walk(&mut e, &["4", "1", "5", "2"], "parity-start5-three");
    r.enter(&f(&["1", "1", "1"]), &mut e);
    check("parity-start-confirm", &r.screen(&mut e).dump());
}

#[test]
fn parity_actions_reach_the_environment() {
    let mut e = parity_env();
    e.units.push(unit(6, false, None, "Unprotected", true));
    let mut r = walk(&mut e, &["4", "1", "5", "3"], "parity-start6-three");
    // RAID 6 needs four.
    r.enter(&f(&["1", "1", "1"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("needs 4 to 18 units"));
    let mut r = walk(&mut e, &["4", "1", "5", "2"], "parity-start5-three");
    r.enter(&f(&["1", "1", "1"]), &mut e);
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["parity start 5 [4, 5, 6]"]);
    assert_eq!(r.top(), "dst-parity");
    e.actions.clear();
    e.units.pop();
    let mut r = walk(&mut e, &["4", "1", "5", "4"], "parity-stop");
    r.enter(&f(&["1"]), &mut e);
    r.function(10, &mut e);
    let mut r2 = walk(&mut e, &["4", "1", "5", "5"], "parity-include");
    r2.enter(&f(&["", "1"]), &mut e);
    r2.function(10, &mut e);
    let mut r3 = walk(&mut e, &["4", "1", "5", "6"], "parity-exclude");
    r3.enter(&f(&["", "1", "", ""]), &mut e);
    r3.function(10, &mut e);
    assert_eq!(
        e.actions,
        vec!["parity stop 1", "parity include 1 5", "parity exclude 1"]
    );
    drop((r, r2, r3));
    // The failed unit is replaced and rebuilt; nothing resumes.
    e.actions.clear();
    let mut r = walk(&mut e, &["4", "2", "3"], "replace-unit-parity");
    r.enter(&f(&["1"]), &mut e);
    r.enter(&f(&["1"]), &mut e);
    check("replace-confirm-parity", &r.screen(&mut e).dump());
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["replace 3 /dev/vdg"]);
    assert!(r.screen(&mut e).dump().contains("rebuilding"));
    // Operators see the status but cannot start.
    let mut r = Runner::new(Box::new(svctools::dst::menu::MainMenu::new(
        "22222222".into(),
        Privilege::DISK_UNITS_OPERATIONS,
    )));
    for k in ["4", "1", "5", "2"] {
        r.enter(&f(&[k]), &mut e);
    }
    assert!(r.screen(&mut e).dump().contains("not authorized"));
}
