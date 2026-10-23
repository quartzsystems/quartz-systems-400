// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SST's screens as a job shows them (`to_uim`, then the character
//! grid). A screen SST shares with DST must equal DST's golden file in
//! `golden/dst/` row for row; SST's own screens have golden files in
//! `golden/sst/` (`QS400_UPDATE_GOLDEN=1` rewrites those). Changes ask
//! for confirmation first and run the functions DST runs.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::{DiskUnit, LogEntry, MemEnv, ResourceRow};
use svctools::panel::Runner;
use svctools::users::Privilege;

fn golden(dir: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden")
        .join(dir)
}

fn rows(r: &Runner, env: &mut MemEnv) -> String {
    let mut out = String::new();
    for row in sst::to_uim(&r.screen(env)).text_rows() {
        out.push_str(&row);
        out.push('\n');
    }
    out
}

/// The screen equals DST's golden of the same screen.
fn same_as_dst(name: &str, r: &Runner, env: &mut MemEnv) {
    let path = golden("dst").join(format!("{name}.txt"));
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(rows(r, env), want, "SST's {name} differs from DST's");
}

/// SST's own screen against its golden.
fn check(name: &str, r: &Runner, env: &mut MemEnv) {
    let got = rows(r, env);
    let dir = golden("sst");
    let path = dir.join(format!("{name}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, &got).unwrap();
        return;
    }
    let want = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}; run with QS400_UPDATE_GOLDEN=1", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(got, want, "screen {name} differs from {}", path.display());
}

fn f(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

fn unit(n: u16, mirrored: bool, partner: Option<u16>, status: &str) -> DiskUnit {
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
        attached: true,
        resume_percent: None,
        device: format!("/dev/vd{}", (b'a' + u8::try_from(n).unwrap()) as char),
    }
}

/// The fixed environment of DST's goldens, asking for confirmation.
fn env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    e.confirm = true;
    e.units = vec![
        unit(0, true, Some(1), "Active"),
        unit(1, true, Some(0), "Suspended"),
        unit(2, false, None, "Unprotected"),
    ];
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
    e.lic = vec![("Processors".into(), "4".into())];
    e
}

fn menu() -> Runner {
    Runner::new(Box::new(sst::SstMenu::new("QSECOFR".into(), Privilege::ALL)))
}

fn walk(env: &mut MemEnv, path: &[&str]) -> Runner {
    let mut r = menu();
    for sel in path {
        r.enter(&f(&[sel]), env);
    }
    r
}

#[test]
fn sst_screens_match_their_golden_files() {
    let mut e = env();
    let r = sst::start();
    check("signon", &r, &mut e);
    let mut r = sst::start();
    r.enter(&f(&["QSECOFR", "QSECOFR"]), &mut e);
    check("change-password", &r, &mut e);
    check("main", &menu(), &mut e);
    check("active", &walk(&mut e, &["2"]), &mut e);
    check("capacity", &walk(&mut e, &["6"]), &mut e);
    check("capacity-processor", &walk(&mut e, &["6", "1"]), &mut e);
    check("users-devices", &walk(&mut e, &["8"]), &mut e);
}

#[test]
fn shared_screens_are_dsts() {
    let mut e = env();
    same_as_dst("tools", &walk(&mut e, &["1"]), &mut e);
    same_as_dst("pal", &walk(&mut e, &["1", "1"]), &mut e);
    same_as_dst("dad", &walk(&mut e, &["1", "4"]), &mut e);
    same_as_dst("hsm", &walk(&mut e, &["1", "7"]), &mut e);
    same_as_dst("hsm-packaging", &walk(&mut e, &["1", "7", "1"]), &mut e);
    same_as_dst("hsm-failed", &walk(&mut e, &["1", "7", "3"]), &mut e);
    same_as_dst("disk", &walk(&mut e, &["3"]), &mut e);
    same_as_dst("disk-config", &walk(&mut e, &["3", "1"]), &mut e);
    same_as_dst("disk-status", &walk(&mut e, &["3", "1", "1", "1"]), &mut e);
    same_as_dst("disk-capacity", &walk(&mut e, &["3", "1", "1", "2"]), &mut e);
    same_as_dst("mirror", &walk(&mut e, &["3", "1", "4"]), &mut e);
    same_as_dst("disk-recovery", &walk(&mut e, &["3", "2"]), &mut e);
    same_as_dst("suspend", &walk(&mut e, &["3", "2", "7"]), &mut e);
    same_as_dst("security", &walk(&mut e, &["7"]), &mut e);
    same_as_dst("users", &walk(&mut e, &["8", "1"]), &mut e);
    let mut r = walk(&mut e, &["8", "1"]);
    r.enter(&f(&["", "4", "", ""]), &mut e);
    same_as_dst("user-display", &r, &mut e);
}

#[test]
fn changes_ask_first() {
    let mut e = env();
    // Suspend: the confirmation, F12 leaves the unit alone, F10 runs it.
    let mut r = walk(&mut e, &["3", "2", "7"]);
    r.enter(&f(&["1"]), &mut e);
    assert_eq!(r.top(), "dst-suspend-confirm");
    check("suspend-confirm", &r, &mut e);
    r.function(12, &mut e);
    assert!(e.actions.is_empty());
    r.enter(&f(&["1"]), &mut e);
    r.function(10, &mut e);
    assert_eq!(e.actions, vec!["suspend 0"]);
    assert_eq!(r.top(), "dst-disk-recovery");
    // Deleting a user ID.
    let mut r = walk(&mut e, &["8", "1"]);
    let row = e.users.list().iter().position(|u| u.name == "22222222").unwrap();
    let mut opts = vec![String::new(); e.users.list().len()];
    opts[row] = "3".into();
    r.enter(&opts, &mut e);
    check("user-delete-confirm", &r, &mut e);
    assert!(e.users.get("22222222").is_some());
    r.function(10, &mut e);
    assert!(e.users.get("22222222").is_none());
    assert_eq!(r.top(), "dst-users");
    // Security options: unchanged leaves at once, a change confirms.
    let mut r = walk(&mut e, &["7"]);
    r.enter(&f(&["N", "N", "N", "N"]), &mut e);
    assert_eq!(r.top(), "sst-main");
    let mut r = walk(&mut e, &["7"]);
    r.enter(&f(&["Y", "N", "N", "N"]), &mut e);
    check("security-confirm", &r, &mut e);
    assert!(!e.flags.allow_default_password_change);
    r.function(10, &mut e);
    assert!(e.flags.allow_default_password_change);
}

#[test]
fn a_user_without_the_privilege_is_refused() {
    let mut e = env();
    let mut r = Runner::new(Box::new(sst::SstMenu::new("22222222".into(), Privilege::IPL)));
    r.enter(&f(&["3"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("not authorized"));
    r.enter(&f(&["7"]), &mut e);
    assert!(r.screen(&mut e).dump().contains("not authorized"));
}
