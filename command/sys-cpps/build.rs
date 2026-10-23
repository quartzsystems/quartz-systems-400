// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Lists `cmds/*.cmd` into `defs.rs`: one `(name, source)` pair per
//! definition, so the crate embeds every command without naming each.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("cmds");
    // The directory, each file, and this script: a file added on a
    // file system that keeps directory times still (drvfs) is seen when
    // the script itself changes.
    println!("cargo:rerun-if-changed=cmds");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=inventory.toml");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .expect("cmds/ exists")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cmd"))
        .map(|p| {
            println!("cargo:rerun-if-changed={}", p.display());
            p.file_stem()
                .expect("a stem")
                .to_string_lossy()
                .to_ascii_uppercase()
        })
        .collect();
    names.sort();
    let mut out = String::from(
        "/// Every definition source, by command name, in name order.\npub const DEFS: &[(&str, &str)] = &[\n",
    );
    for n in &names {
        let path = dir.join(format!("{n}.cmd"));
        let _ = writeln!(
            out,
            "    ({n:?}, include_str!({:?})),",
            path.to_string_lossy().replace('\\', "/")
        );
    }
    out.push_str("];\n");
    let dst = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("defs.rs");
    std::fs::write(dst, out).expect("write defs.rs");
}
