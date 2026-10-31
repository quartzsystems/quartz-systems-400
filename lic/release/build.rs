// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Captures the build ID (`git describe --always --dirty`) into the
//! `QS400_BUILD_ID` environment variable for `env!`. An explicit
//! `QS400_BUILD_ID` in the environment wins, so a release build can pin it.
//! Without git the ID is `unknown`.
//!
//! `QS400_RELEASE` (`VxRyMz`) makes a test build claim another release,
//! for the upgrade path (docs/INSTALL.md); V1R0M0 when absent.

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=QS400_BUILD_ID");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    if let Some(repo) = &repo {
        println!(
            "cargo:rerun-if-changed={}",
            repo.join(".git/HEAD").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            repo.join(".git/index").display()
        );
    }
    let id = env::var("QS400_BUILD_ID")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| git_describe(repo.as_deref()))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=QS400_BUILD_ID={id}");
    println!("cargo:rerun-if-env-changed=QS400_RELEASE");
    let rel = env::var("QS400_RELEASE").unwrap_or_default();
    let (v, r, m) = parse_release(rel.trim()).unwrap_or((1, 0, 0));
    println!("cargo:rustc-env=QS400_RELEASE_V={v}");
    println!("cargo:rustc-env=QS400_RELEASE_R={r}");
    println!("cargo:rustc-env=QS400_RELEASE_M={m}");
}

/// `VxRyMz` as three numbers; `None` for anything else.
fn parse_release(s: &str) -> Option<(u8, u8, u8)> {
    let rest = s.strip_prefix('V')?;
    let (v, rest) = rest.split_once('R')?;
    let (r, m) = rest.split_once('M')?;
    Some((v.parse().ok()?, r.parse().ok()?, m.parse().ok()?))
}

fn git_describe(repo: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new("git");
    if let Some(r) = repo {
        cmd.arg("-C").arg(r);
    }
    let out = cmd
        .args(["describe", "--always", "--dirty", "--abbrev=7"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}
