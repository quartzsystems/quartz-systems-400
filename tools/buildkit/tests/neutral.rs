// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The library stays product-neutral and light (docs/BUILDKIT.md): it
//! depends on no operating-system crate, and its code names no product.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The crates of this repository `buildkit` may depend on: the disk
/// layout and what it pulls in, and the ISO 9660 writer.
const ALLOWED: &[&str] = &["buildkit", "diskimg", "blockdev", "xfsfmt", "optical"];

/// Crates it must never reach, whatever else changes.
const FORBIDDEN: &[&str] = &[
    "conformance",
    "instmedia",
    "lpbuild",
    "kcbuild",
    "ptf",
    "ptf-update",
    "ptfbuild",
    "fmtcheck",
    "helpaudit",
    "textaudit",
    "release",
    "slic",
    "qsjob",
    "store",
];

#[test]
fn depends_on_no_operating_system_crate() {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", "buildkit", "--locked", "-e", "normal,build"])
        .args(["--prefix", "none", "--format", "{p}"])
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let mut seen = 0;
    for line in text.lines() {
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        seen += 1;
        assert!(
            !FORBIDDEN.contains(&name),
            "buildkit reaches {name}: {line}"
        );
        // A crate of this repository is printed with its path.
        if line.contains(" (/") || line.contains(":\\") {
            assert!(
                ALLOWED.contains(&name),
                "buildkit depends on {name}, a crate of this repository outside its allowed list"
            );
        }
    }
    assert!(seen > 20, "cargo tree printed {seen} lines");
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn names_no_product() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 15, "{} source files", files.len());
    let mut hits = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).unwrap();
        // Tests may name a product; so may documentation.
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (n, line) in code.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if t.to_ascii_lowercase().contains("qs400") || t.contains("QS/400") {
                hits.push(format!("{}:{}: {t}", f.display(), n + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "a product named in the library:\n{}",
        hits.join("\n")
    );
}
