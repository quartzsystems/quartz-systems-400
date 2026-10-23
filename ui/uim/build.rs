// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lists the shipped system panel groups (`pnlgrp/*.pnlgrp`) for
//! `shipped.rs`, so a new file is shipped without touching code.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("pnlgrp");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .expect("the pnlgrp directory exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pnlgrp"))
        .collect();
    entries.sort();
    let mut out = String::from(
        "/// Every shipped system panel group: name and UIM source.\npub const FILES: &[(&str, &str)] = &[\n",
    );
    for p in entries {
        println!("cargo:rerun-if-changed={}", p.display());
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("a name")
            .to_string();
        let file = p.file_name().unwrap().to_string_lossy().into_owned();
        let _ = writeln!(
            out,
            "    ({name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/pnlgrp/{file}\"))),"
        );
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("shipped_list.rs");
    std::fs::write(dest, out).expect("write the list");
}
