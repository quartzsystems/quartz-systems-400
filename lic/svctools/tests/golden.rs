// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every DST screen, rendered against a fixed environment, must match
//! its golden file in `conformance/golden/dst/` whitespace-exact. Set
//! `QS400_UPDATE_GOLDEN=1` to rewrite the files from the code.

use std::fs;
use std::path::{Path, PathBuf};

use svctools::env::MemEnv;
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

#[test]
fn dst_screens_match_their_golden_files() {
    let mut env = MemEnv::new("QS400");
    let mut r = svctools::dst::start();
    check("signon", &r.screen(&mut env).dump());
    r.enter(&f(&["QSECOFR", "QSECOFR"]), &mut env);
    check("change-password", &r.screen(&mut env).dump());
    r.enter(&f(&["QSECOFR", "QSECOFR", "Quartz1", "Quartz1"]), &mut env);
    check("main", &r.screen(&mut env).dump());
    let mut r = Runner::new(Box::new(svctools::dst::menu::MainMenu::new(
        "22222222".into(),
        Privilege::IPL,
    )));
    r.enter(&f(&["2"]), &mut env);
    check("main-not-authorized", &r.screen(&mut env).dump());
}
