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
        &panels::change_password("QS400", 1_792_540_800, ""),
    );
    golden(
        "signon/sign-on-info.txt",
        &panels::signon_info("QS400", 1_792_540_800 + 51_000, 2, Some(5)),
    );
    golden("main-interim.txt", &panels::main_menu("QS400", "", ""));
}
