// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sign-on panels and the interim MAIN menu against their golden
//! files; `QS400_UPDATE_GOLDEN=1` rewrites them.

use std::path::PathBuf;

use wm::ctl::Panel;
use wm::panels;

fn golden(rel: &str, p: &Panel) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden")
        .join(rel);
    let got = p.lines().join("\n") + "\n";
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        got, want,
        "{rel} differs; QS400_UPDATE_GOLDEN=1 rewrites it"
    );
}

#[test]
fn panels_match() {
    golden(
        "signon/change-password.txt",
        &panels::change_password("QSECOFR", 1_792_540_800, false, ""),
    );
    golden(
        "signon/change-password-pwdlvl2.txt",
        &panels::change_password("QSECOFR", 1_792_540_800, true, ""),
    );
    golden(
        "signon/sign-on-info.txt",
        &panels::signon_info("QS400", 1_792_540_800 + 51_000, 2, Some(5)),
    );
    golden("main-interim.txt", &panels::main_menu("QS400", "", ""));
}

/// The interim sign-on agrees with the console's at both levels.
#[test]
fn sign_on_matches_the_console() {
    golden(
        "signon/sign-on.txt",
        &panels::sign_on("QS400", "QBASE", "QSCONSOLE", false, ""),
    );
    golden(
        "signon/sign-on-pwdlvl2.txt",
        &panels::sign_on("QS400", "QBASE", "QSCONSOLE", true, ""),
    );
}

/// Change Password's text against IBM's: level 0 against the 7.1
/// capture (`tools/conformance/golden/verified/oracle71/signon`), level
/// 2 and 3 against the 7.5 one (`verified/pub400/signon`), whose F9
/// for the password rules the row of keys leaves out.
#[test]
fn change_password_matches_ibm() {
    let cases = [
        ("oracle71", "USERNAM", false, 23),
        ("pub400", "USERNAME", true, 22),
    ];
    for (source, user, long, rows) in cases {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/golden/verified")
            .join(source)
            .join("signon/change-password.txt");
        let want = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .replace("\r\n", "\n");
        let want: Vec<&str> = want.lines().collect();
        let mut got = panels::change_password(user, 1_792_540_800, long, "").lines();
        got[4].replace_range(46..54, "MM/DD/YY");
        for i in 0..rows {
            assert_eq!(
                got[i].trim_end(),
                want[i].trim_end(),
                "{source}: row {}",
                i + 1
            );
        }
    }
}
