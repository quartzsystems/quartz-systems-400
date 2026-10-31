// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Sign On display against its golden files, at password level 0
//! and 1 and at level 2 and 3; `QS400_UPDATE_GOLDEN=1` rewrites them.

use std::path::PathBuf;

fn golden(name: &str, long_password: bool) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/signon")
        .join(name);
    let got = console::screens::sign_on("QS400", "QBASE", "QSCONSOLE", long_password).dump();
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        got, want,
        "{name} differs; QS400_UPDATE_GOLDEN=1 rewrites it"
    );
}

#[test]
fn sign_on_matches() {
    golden("sign-on.txt", false);
    golden("sign-on-pwdlvl2.txt", true);
}

/// Rows 1 to 12 and where the copyright starts, against IBM i 7.1's
/// sign-on (password level 0) with its values.
#[test]
fn sign_on_matches_the_71_capture() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/oracle71/signon/sign-on.txt");
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    let want: Vec<&str> = want.lines().collect();
    let got = console::screens::sign_on("SYSNAME", "QBASE", "QPADEV0001", false).dump();
    let got: Vec<&str> = got.lines().collect();
    for i in 0..12 {
        assert_eq!(got[i].trim_end(), want[i].trim_end(), "row {}", i + 1);
    }
    assert_eq!(got[23].find("(C)"), want[23].find("(C)"), "the copyright row");
}
