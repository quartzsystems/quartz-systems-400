// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! For a product's `build.rs`: capture the release and the build ID
//! into environment variables `env!` reads.
//!
//! With the prefix `QS400_`: `QS400_BUILD_ID` is `git describe --always
//! --dirty` in the product's repository; an explicit `QS400_BUILD_ID` in
//! the environment wins, so a release build can pin it, and without git
//! the ID is `unknown`. `QS400_RELEASE` (`VxRyMz`) makes a test build
//! claim another release; absent, the release follows `VERSION` at the
//! repository root (`1.1.0` is V1R1M0). The three numbers go to
//! `QS400_RELEASE_V`, `_R` and `_M`.

use std::env;
use std::path::Path;
use std::process::Command;

/// Print the cargo directives that capture the build ID and the release
/// of the product whose repository is `repo`, under `prefix`.
///
/// # Panics
///
/// When `VERSION` is needed and is not `MAJOR.MINOR.PATCH`: a build
/// script has no better way to stop the build.
pub fn capture(repo: &Path, prefix: &str) {
    println!("cargo:rerun-if-env-changed={prefix}BUILD_ID");
    println!(
        "cargo:rerun-if-changed={}",
        repo.join(".git/HEAD").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        repo.join(".git/index").display()
    );
    let id = env::var(format!("{prefix}BUILD_ID"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| git_describe(repo))
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env={prefix}BUILD_ID={id}");

    println!("cargo:rerun-if-env-changed={prefix}RELEASE");
    let rel = env::var(format!("{prefix}RELEASE")).unwrap_or_default();
    let (v, r, m) = match parse_release(rel.trim()) {
        Some(vrm) => vrm,
        None => version_file(repo),
    };
    println!("cargo:rustc-env={prefix}RELEASE_V={v}");
    println!("cargo:rustc-env={prefix}RELEASE_R={r}");
    println!("cargo:rustc-env={prefix}RELEASE_M={m}");
}

/// `VERSION` (`MAJOR.MINOR.PATCH`) at `repo` as the release's three numbers.
fn version_file(repo: &Path) -> (u8, u8, u8) {
    let path = repo.join("VERSION");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = std::fs::read_to_string(&path).expect("VERSION at the repository root");
    parse_version(&text)
        .unwrap_or_else(|| panic!("VERSION is not MAJOR.MINOR.PATCH: {}", text.trim()))
}

/// `MAJOR.MINOR.PATCH` as three numbers.
#[must_use]
pub fn parse_version(text: &str) -> Option<(u8, u8, u8)> {
    let mut parts = text.trim().split('.').map(str::parse::<u8>);
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Ok(v)), Some(Ok(r)), Some(Ok(m)), None) => Some((v, r, m)),
        _ => None,
    }
}

/// `VxRyMz` as three numbers; `None` for anything else.
#[must_use]
pub fn parse_release(s: &str) -> Option<(u8, u8, u8)> {
    let rest = s.strip_prefix('V')?;
    let (v, rest) = rest.split_once('R')?;
    let (r, m) = rest.split_once('M')?;
    Some((v.parse().ok()?, r.parse().ok()?, m.parse().ok()?))
}

fn git_describe(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["describe", "--always", "--dirty", "--abbrev=7"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_and_release_forms() {
        assert_eq!(parse_version("1.1.0\n"), Some((1, 1, 0)));
        assert_eq!(parse_version("0.3.0"), Some((0, 3, 0)));
        for bad in ["1.1", "1.1.0.0", "a.b.c", ""] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert_eq!(parse_release("V1R2M0"), Some((1, 2, 0)));
        assert_eq!(parse_release(""), None);
        assert_eq!(parse_release("1.2.0"), None);
    }
}
