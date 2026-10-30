// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The dependency graph's bans without cargo-deny: no existing shell
//! utilities anywhere in `Cargo.lock`, and nothing that compiles or
//! links C in the graph that is actually built.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Existing coreutils-style crates (uutils, busybox ports, shells'
/// utility sets). QSH's utilities are written from scratch.
const UTILITIES: &[&str] = &[
    "coreutils",
    "uucore",
    "uucore_procs",
    "uutests",
    "findutils",
    "diffutils",
    "procps",
    "sed",
    "busybox",
    "rust-busybox",
    "toybox",
    "nu-command",
    "brush-builtins",
    "brush-core",
];

/// Name prefixes of the same: uutils publishes one `uu_*` crate per
/// utility.
const UTILITY_PREFIXES: &[&str] = &["uu_", "uutils"];

/// Build helpers that exist only to compile or bind C.
const C_BUILDERS: &[&str] = &["cc", "cmake", "bindgen", "pkg-config", "vcpkg"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root")
}

/// Every package name in `Cargo.lock`.
fn locked_names(root: &Path) -> HashSet<String> {
    let text = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    text.lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .filter_map(|l| l.strip_suffix('"'))
        .map(str::to_string)
        .collect()
}

/// The crates `deny.toml` bans.
fn denied(root: &Path) -> HashSet<String> {
    let text = std::fs::read_to_string(root.join("deny.toml")).expect("deny.toml");
    text.lines()
        .filter_map(|l| l.trim().strip_prefix("{ crate = \""))
        .filter_map(|l| l.split('"').next())
        .map(str::to_string)
        .collect()
}

fn is_utility(name: &str) -> bool {
    UTILITIES.contains(&name) || UTILITY_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[test]
fn no_shell_utility_crate_in_the_lockfile() {
    let root = repo_root();
    let mut hits: Vec<String> = locked_names(&root)
        .into_iter()
        .filter(|n| is_utility(n))
        .collect();
    hits.sort();
    assert!(hits.is_empty(), "shell-utility crates in Cargo.lock: {hits:?}");
}

#[test]
fn deny_toml_bans_the_utilities_and_c_helpers() {
    let banned = denied(&repo_root());
    let missing: Vec<&&str> = UTILITIES
        .iter()
        .chain(C_BUILDERS)
        .filter(|n| !banned.contains(**n))
        .collect();
    assert!(missing.is_empty(), "deny.toml does not ban {missing:?}");
}

fn cargo(root: &Path, args: &[&str]) -> String {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(args)
        .current_dir(root)
        .output()
        .expect("cargo runs");
    assert!(
        out.status.success(),
        "cargo {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `Cargo.lock` also lists optional dependencies nobody turns on
/// (`rustls-webpki`'s `ring`, its `cc`); the graph that is built, for
/// the image target and the host, is `cargo tree`'s.
#[test]
fn no_c_in_the_built_graph() {
    let root = repo_root();
    let json = cargo(&root, &["metadata", "--format-version", "1", "--locked"]);
    let meta: serde_json::Value = serde_json::from_str(&json).expect("cargo metadata output");
    let links: HashMap<String, String> = meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter_map(|p| Some((p["name"].as_str()?.to_string(), p["links"].as_str()?.to_string())))
        .collect();
    let denied = denied(&root);
    for target in ["x86_64-unknown-linux-musl", "x86_64-unknown-linux-gnu"] {
        let tree = cargo(
            &root,
            &[
                "tree", "--workspace", "--locked", "-e", "normal,build", "--prefix", "none",
                "--format", "{p}", "--target", target,
            ],
        );
        let active: HashSet<&str> = tree.lines().filter_map(|l| l.split_whitespace().next()).collect();
        let mut hits: Vec<String> = Vec::new();
        for name in &active {
            if denied.contains(*name) || is_utility(name) {
                hits.push(format!("{name}: banned in deny.toml"));
            }
            if let Some(l) = links.get(*name) {
                hits.push(format!("{name}: links = \"{l}\""));
            }
        }
        hits.sort();
        assert!(hits.is_empty(), "{target}: {}", hits.join("; "));
        assert!(active.contains("qsjob"), "{target}: the tree holds the workspace");
    }
}
