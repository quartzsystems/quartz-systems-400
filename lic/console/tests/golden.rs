// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Sign On display against its golden file; `QS400_UPDATE_GOLDEN=1`
//! rewrites it.

use std::path::PathBuf;

#[test]
fn sign_on_matches() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/signon/sign-on.txt");
    let got = console::screens::sign_on("QS400", "QBASE", "QSCONSOLE", true).dump();
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        got, want,
        "the Sign On display differs; QS400_UPDATE_GOLDEN=1 rewrites it"
    );
}
