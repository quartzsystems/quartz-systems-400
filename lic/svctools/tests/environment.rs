// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DST environment, service tool and security screens against a
//! fixed environment: golden files and the actions they take.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::{DstEnv, LogEntry, MemEnv, ResourceRow};
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
    assert_eq!(dump, want, "screen {name} differs from {}", path.display());
}

fn f(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    e.hardware = vec![
        ResourceRow {
            name: "CMB01".into(),
            kind: "Bridge".into(),
            status: "Operational".into(),
            location: "0000:00:00.0".into(),
            description: "host bridge".into(),
        },
        ResourceRow {
            name: "DD001".into(),
            kind: "Disk".into(),
            status: "Operational".into(),
            location: "vda".into(),
            description: "vda 4096 MiB".into(),
        },
        ResourceRow {
            name: "LIN01".into(),
            kind: "Lan".into(),
            status: "NotDetected".into(),
            location: "0000:00:03.0".into(),
            description: "ethernet, port eth0".into(),
        },
    ];
    e.log = vec![LogEntry {
        time: 1_767_225_700,
        code: "A6001255".into(),
        text: "Mirrored unit missing, protection suspended".into(),
    }];
    e.lic = vec![
        ("Release".into(), "V1R0M0".into()),
        ("Build".into(), "2026-10-01-00000000".into()),
        ("Slot of this IPL".into(), "A".into()),
        ("Last good slot".into(), "A".into()),
        ("IPLs".into(), "3 (1 abnormal)".into()),
        ("Installed".into(), "2026-01-01 00:00:00".into()),
    ];
    e
}

fn runner() -> Runner {
    Runner::new(Box::new(svctools::dst::menu::MainMenu::new(
        "QSECOFR".into(),
        Privilege::ALL,
    )))
}

fn walk(env: &mut MemEnv, path: &[&str], name: &str) -> Runner {
    let mut r = runner();
    for sel in path {
        r.enter(&f(&[sel]), env);
    }
    check(name, &r.screen(env).dump());
    r
}

#[test]
fn environment_screens_match_their_golden_files() {
    let mut e = env();
    walk(&mut e, &["5"], "env");
    walk(&mut e, &["5", "1"], "env-active");
    walk(&mut e, &["5", "2"], "env-devices");
    walk(&mut e, &["5", "3"], "users");
    let mut r = walk(&mut e, &["5", "3"], "users");
    r.enter(&f(&["", "4", "", ""]), &mut e);
    check("user-display", &r.screen(&mut e).dump());
    let mut r = walk(&mut e, &["5", "3"], "users");
    r.enter(&f(&["2", "", "", ""]), &mut e);
    check("user-privileges", &r.screen(&mut e).dump());
    let mut r = walk(&mut e, &["5", "3"], "users");
    r.function(6, &mut e);
    check("user-create", &r.screen(&mut e).dump());
    walk(&mut e, &["5", "4"], "sysvals");
    walk(&mut e, &["7"], "tools");
    walk(&mut e, &["7", "1"], "pal");
    walk(&mut e, &["7", "4"], "dad");
    walk(&mut e, &["7", "5"], "lic-log");
    walk(&mut e, &["7", "7"], "hsm");
    walk(&mut e, &["7", "7", "1"], "hsm-packaging");
    walk(&mut e, &["7", "7", "3"], "hsm-failed");
    walk(&mut e, &["13"], "security");
    walk(&mut e, &["3"], "lic-info");
    walk(&mut e, &["6"], "console-mode");
}

#[test]
fn console_mode_is_chosen_and_kept() {
    use svctools::dst::console_mode::ConsoleMode;
    let mut e = env();
    let mut r = walk(&mut e, &["6"], "console-mode");
    assert_eq!(r.enter(&f(&["2"]), &mut e), None);
    let d = r.screen(&mut e).dump();
    assert!(d.contains("Use Dedicated Service Tools"), "back on the menu");
    assert!(d.contains("takes effect at the next IPL"));
    assert_eq!(e.console_mode, ConsoleMode::Serial);
    assert_eq!(e.actions, vec!["console Serial"]);
    let r = walk(&mut e, &["5", "2"], "env-devices-serial");
    assert!(r.screen(&mut e).dump().contains("Serial console (ttyS0)"));
}

#[test]
fn user_ids_are_managed_and_saved() {
    let mut e = env();
    // Create OPER1, grant disk operations, sign on with it later.
    let mut r = walk(&mut e, &["5", "3"], "users");
    r.function(6, &mut e);
    r.enter(&f(&["oper1", "pw1", "pw2", "Night shift"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("do not match"));
    r.enter(&f(&["oper1", "pw1", "pw1", "Night shift"]), &mut e);
    assert_eq!(r.top(), "dst-user-privileges");
    let mut marks = vec![String::new(); 11];
    marks[1] = "X".into();
    marks[9] = "x".into();
    r.enter(&marks, &mut e);
    assert_eq!(r.top(), "dst-users");
    let oper = e.users.get("OPER1").unwrap();
    assert!(
        oper.privileges
            .has(Privilege::DISK_UNITS_OPERATIONS.with(Privilege::IPL))
    );
    assert!(!oper.privileges.has(Privilege::DISK_UNITS_ADMIN));
    assert!(oper.expired);
    assert!(e.saves >= 2);
    // Disable, enable, reset, delete.
    let row = e
        .users
        .list()
        .iter()
        .position(|u| u.name == "OPER1")
        .unwrap();
    let opts = |o: &str| {
        let mut v = vec![String::new(); e.users.list().len()];
        v[row] = o.into();
        v
    };
    let (disable, enable, reset, delete) = (opts("6"), opts("5"), opts("7"), opts("3"));
    r.enter(&disable, &mut e);
    assert!(e.users.get("OPER1").unwrap().disabled);
    r.enter(&enable, &mut e);
    assert!(!e.users.get("OPER1").unwrap().disabled);
    r.enter(&reset, &mut e);
    assert_eq!(r.top(), "dst-user-reset");
    r.enter(&f(&["OPER1", "new", "new"]), &mut e);
    assert_eq!(r.top(), "dst-users");
    r.enter(&delete, &mut e);
    assert!(e.users.get("OPER1").is_none());
    // The last administrator stays.
    let officer = e
        .users
        .list()
        .iter()
        .position(|u| u.name == "QSECOFR")
        .unwrap();
    let mut fields = vec![String::new(); e.users.list().len()];
    fields[officer] = "3".into();
    r.enter(&fields, &mut e);
    assert!(r.screen(&mut e).dump().contains("must keep that privilege"));
    assert!(e.users.get("QSECOFR").is_some());
}

#[test]
fn security_flags_and_paging() {
    let mut e = env();
    let mut r = walk(&mut e, &["13"], "security");
    r.enter(&f(&["Y", "N", "y", ""]), &mut e);
    assert!(e.flags.allow_default_password_change);
    assert!(e.flags.allow_sysval_security_changes);
    assert!(!e.flags.allow_add_certificates);
    assert_eq!(r.top(), "dst-main");
    // A long notice pages with F8 and F7.
    let mut many = env();
    many.log = (0..40)
        .map(|i| LogEntry {
            time: i,
            code: format!("A60{i:05}"),
            text: "entry".into(),
        })
        .collect();
    let mut r = walk(&mut many, &["7", "1"], "pal-long");
    assert!(r.screen(&mut many).dump().contains("More..."));
    r.function(8, &mut many);
    r.function(8, &mut many);
    assert!(r.screen(&mut many).dump().contains("Bottom"));
    r.function(7, &mut many);
    assert!(r.screen(&mut many).dump().contains("More..."));
    // Display/alter/dump refuses nonsense and shows hex for a page.
    let mut r = walk(&mut e, &["7", "4"], "dad");
    r.enter(&f(&["", "", ""]), &mut e);
    assert!(r.screen(&mut e).dump().contains("Type a unit number"));
    let lines = svctools::dst::tools::hex_lines("x", b"QS400ASP\x00\x01");
    assert_eq!(
        lines[1],
        "000000  51 53 34 30 30 41 53 50 00 01                    QS400ASP.."
    );
    let _ = e.now();
}
