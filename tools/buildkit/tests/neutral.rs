// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The library stays product-neutral and light (docs/BUILDKIT.md): it
//! depends on no operating-system crate, and its code names no product.
//! The same holds for the crates an appliance takes beside it (handoff
//! 14): the console, the screen kit, the service tools user IDs, disk
//! protection, the ublk server and the netlink client.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The crates of this repository `buildkit` may depend on: the disk
/// layout and what it pulls in, and `optical`, the ISO 9660 writer the
/// media step needs (accepted in handoff 13a).
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

/// The crates an appliance takes beside the pipeline: the package and
/// its directory in this repository.
const APPLIANCE: &[(&str, &str)] = &[
    ("console", "lic/console"),
    ("scrkit", "lic/scrkit"),
    ("svcusers", "lic/svcusers"),
    ("protect", "storage/protect"),
    ("ublk", "lic/ublk"),
    ("netlink", "net/netlink"),
];

/// What none of them may reach: QS/400's identity, its LIC record, its
/// object store, its service tools, its processes.
const NOT_UNDER_APPLIANCE: &[&str] = &["release", "licpart", "store", "svctools", "slic", "qsjob"];

/// The packages `cargo tree` names under `package`.
fn tree(package: &str, more: &[&str]) -> Vec<String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", package, "--locked", "-e", "normal,build"])
        .args(["--prefix", "none", "--format", "{p}"])
        .args(more)
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree -p {package}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

#[test]
fn the_appliance_crates_reach_no_product_crate() {
    for (package, _) in APPLIANCE {
        // With every feature, and for the target an appliance builds.
        for more in [
            &["--all-features"][..],
            &["--all-features", "--target", "x86_64-unknown-linux-musl"],
        ] {
            let names = tree(package, more);
            assert!(names.iter().any(|n| n == package), "{package}: {names:?}");
            for not in NOT_UNDER_APPLIANCE {
                assert!(
                    !names.iter().any(|n| n == not),
                    "{package} reaches {not} ({more:?})"
                );
            }
        }
    }
    // The console without its 5250 session brings nothing of the
    // display interface either.
    let bare = tree("console", &["--no-default-features"]);
    for not in ["tn5250", "ccsid"] {
        assert!(!bare.iter().any(|n| n == not), "console reaches {not}");
    }
    // The kit is the console and nothing else of this repository.
    let kit = tree("scrkit", &[]);
    for not in ["tn5250", "ccsid", "svcusers", "protect"] {
        assert!(!kit.iter().any(|n| n == not), "scrkit reaches {not}");
    }
}

/// Lines of a source file outside its tests that name the product:
/// `QS400` in any case, or `QS/400`, in code or in documentation.
fn product_lines(file: &Path, hits: &mut Vec<String>) {
    let text = fs::read_to_string(file).unwrap();
    let code = text
        .split("#[cfg(test)]")
        .next()
        .and_then(|t| t.split("#[cfg(all(test").next())
        .unwrap_or_default();
    for (n, line) in code.lines().enumerate() {
        if line.to_ascii_lowercase().contains("qs400") || line.contains("QS/400") {
            hits.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
        }
    }
}

#[test]
fn the_appliance_crates_name_no_product() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut hits = Vec::new();
    for (package, dir) in APPLIANCE {
        let dir = repo.join(dir);
        let mut files = Vec::new();
        sources(&dir.join("src"), &mut files);
        assert!(files.len() >= 3, "{package}: {} source files", files.len());
        for extra in ["build.rs", "Cargo.toml"] {
            if dir.join(extra).is_file() {
                files.push(dir.join(extra));
            }
        }
        for f in files {
            product_lines(&f, &mut hits);
        }
    }
    assert!(
        hits.is_empty(),
        "a product named in a crate an appliance takes:\n{}",
        hits.join("\n")
    );
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
