// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Captures the build ID (`git describe --always --dirty`) into the
//! `QS400_BUILD_ID` environment variable for `env!`. An explicit
//! `QS400_BUILD_ID` in the environment wins, so a release build can pin it.
//! Without git the ID is `unknown`.

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
