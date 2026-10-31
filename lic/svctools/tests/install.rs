// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installer's screens against a fixed environment: golden files
//! for every menu, list, confirmation, status display and option form.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::{MemEnv, NonConfigured};
use svctools::install::{LIC_STEPS, OS_STEPS, Progress, disk_status};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace")
        .join("tools")
        .join("conformance")
        .join("golden")
        .join("install")
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

fn env() -> MemEnv {
    let mut e = MemEnv::new("QS400");
    e.spare = vec![
        NonConfigured {
            device: "/dev/vda".into(),
            resource: "DD001".into(),
            serial: "QM00001".into(),
            type_model: "QEMU HARDDISK".into(),
            size_mib: 4096,
            has_data: true,
        },
        NonConfigured {
            device: "/dev/vdb".into(),
            resource: "DD002".into(),
            serial: "QM00002".into(),
            type_model: "QEMU HARDDISK".into(),
            size_mib: 4096,
            has_data: false,
        },
    ];
    e
}

#[test]
fn install_screens_match_their_golden_files() {
    let mut e = env();
    let mut r = svctools::install::lic::start();
    check("lic-menu", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("lic-options", &r.screen(&mut e).dump());
    r.enter(&f(&["2"]), &mut e);
    check("load-source", &r.screen(&mut e).dump());
    r.enter(&f(&["", "1"]), &mut e);
    check("lic-confirm", &r.screen(&mut e).dump());
    check("disk-status", &disk_status("QS400", "/dev/vdb", 40).dump());
    let mut p = Progress::new(&LIC_STEPS);
    p.done(0);
    p.done(1);
    check("lic-status", &svctools::install::lic_status(&e, &p).dump());
    let mut r = svctools::install::os::start();
    check("device-type", &r.screen(&mut e).dump());
    r.enter(&f(&["2"]), &mut e);
    check("os-confirm", &r.screen(&mut e).dump());
    r.function(10, &mut e);
    check("language", &r.screen(&mut e).dump());
    r.enter(&f(&["1"]), &mut e);
    check("language-confirm", &r.screen(&mut e).dump());
    let mut p = Progress::new(&OS_STEPS);
    p.done(0);
    p.note = "Language feature 2924".into();
    check("os-status", &svctools::install::os_status(&e, &p).dump());
    let mut r = svctools::install::options::start();
    check("ipl-options", &r.screen(&mut e).dump());
    r.enter(
        &f(&["2026-01-01", "00:00:00", "Y", "N", "N", "N", "N", "Y", "N"]),
        &mut e,
    );
    check("major-options", &r.screen(&mut e).dump());
    let src = ("C1001040", "Clock set from RTC");
    check(
        "no-os",
        &svctools::install::noos::screen("QS400", src, &e.spare).dump(),
    );
    check(
        "no-os-no-disks",
        &svctools::install::noos::screen("QS400", src, &[]).dump(),
    );
}
