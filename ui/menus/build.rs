// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lists the shipped panel group sources (`pnlgrp/`, generated, and
//! `sys/`, hand-written) for `shipped.rs`, so a new file is shipped
//! without touching code.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut names = Vec::new();
    for dir in ["pnlgrp", "sys"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
        let mut entries: Vec<_> = std::fs::read_dir(root.join(dir))
            .expect("the source directory exists")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "pnlgrp"))
            .collect();
        entries.sort();
        for p in entries {
            println!("cargo:rerun-if-changed={}", p.display());
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a name")
                .to_string();
            names.push((name, format!("{dir}/{}", p.file_name().unwrap().to_string_lossy())));
        }
    }
    let mut out = String::from("/// Every shipped panel group: name and UIM source.\npub const FILES: &[(&str, &str)] = &[\n");
    for (name, rel) in &names {
        let _ = writeln!(
            out,
            "    ({name:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{rel}\"))),"
        );
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("shipped_list.rs");
    std::fs::write(dest, out).expect("write the list");
}
