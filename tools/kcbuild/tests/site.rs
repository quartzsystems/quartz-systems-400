// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Knowledge Center builds from the tree and links what it promises.

use std::path::Path;

#[test]
fn builds_every_section() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let out = std::env::temp_dir().join(format!("kcbuild-{}", std::process::id()));
    let s = kcbuild::build(repo, &out).unwrap();
    assert!(s.commands > 600, "{s:?}");
    assert!(s.messages > 1000, "{s:?}");
    assert!(s.docs > 50, "{s:?}");
    for p in [
        "index.html",
        "commands/index.html",
        "commands/CRTLIB.html",
        "messages/index.html",
        "docs/index.html",
        "docs/PHASE12.html",
        "conformance.html",
        "qs400.css",
    ] {
        assert!(out.join(p).is_file(), "{p}");
    }
    let page = std::fs::read_to_string(out.join("commands/CRTLIB.html")).unwrap();
    assert!(page.contains("<a id=\"LIB\"></a>LIB"));
    assert!(page.contains(&format!("QS/400 {}", release::RELEASE)));
    let _ = std::fs::remove_dir_all(&out);
}
