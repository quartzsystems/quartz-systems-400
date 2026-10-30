// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file system a PASE process sees, as steps the child carries out
//! in its own mount namespace before `pivot_root`.
//!
//! The root is a small read-only `tmpfs` holding every top-level entry
//! of `/QOpenSys` (directories and files bind-mounted, symbolic links
//! copied), so `/usr/bin`, `/etc`, `/tmp` and `/var` are the ones under
//! `/QOpenSys`, as an AIX program built for PASE expects. `/QOpenSys`
//! itself is mounted too, so the IFS spelling of a path works, and
//! `/home` comes from the IFS root. `/dev` holds the host's null, zero,
//! full, random, urandom and tty devices; `/proc` is the new PID
//! namespace's. `/QSYS.LIB` and the rest of the root file system are not
//! in the view: they are the storage service's, not files.

use std::path::{Path, PathBuf};

/// One step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Make a directory.
    Dir(PathBuf),
    /// Make an empty file (a mount point for a file).
    File(PathBuf),
    /// Make a symbolic link `.0` pointing at `.1`.
    Link(PathBuf, PathBuf),
    /// Bind-mount `.0` (recursively) on `.1`.
    Bind(PathBuf, PathBuf),
    /// Mount `proc` on the path.
    Proc(PathBuf),
}

/// The host devices a PASE process gets.
pub const DEVICES: &[&str] = &["null", "zero", "full", "random", "urandom", "tty"];

/// The IFS's two trees under its mount: the root file system and
/// `/QOpenSys`.
#[must_use]
pub fn trees(ifs: &Path) -> (PathBuf, PathBuf) {
    (ifs.join("root"), ifs.join("QOpenSys"))
}

/// The steps that build the view in `stage` (a fresh `tmpfs`) from the
/// IFS mounted at `ifs`, with the host's devices from `dev`. `exec`, when
/// given, is a mount of `/QOpenSys` that allows programs (the IFS mount
/// does not), used in its place.
pub fn plan(ifs: &Path, exec: Option<&Path>, stage: &Path, dev: &Path) -> std::io::Result<Vec<Step>> {
    let (root, qopensys) = trees(ifs);
    let qopensys = exec.map_or(qopensys, Path::to_path_buf);
    let mut steps = Vec::new();
    let mut names = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&qopensys)?.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name();
        let Some(n) = name.to_str() else { continue };
        if matches!(n, "dev" | "proc" | "QOpenSys") {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        let dst = stage.join(n);
        if ft.is_symlink() {
            if let Ok(t) = std::fs::read_link(e.path()) {
                steps.push(Step::Link(dst, t));
            }
        } else if ft.is_dir() {
            steps.push(Step::Dir(dst.clone()));
            steps.push(Step::Bind(e.path(), dst));
        } else {
            steps.push(Step::File(dst.clone()));
            steps.push(Step::Bind(e.path(), dst));
        }
        names.push(n.to_string());
    }
    steps.push(Step::Dir(stage.join("QOpenSys")));
    steps.push(Step::Bind(qopensys, stage.join("QOpenSys")));
    let home = root.join("home");
    if home.is_dir() && !names.iter().any(|n| n == "home") {
        steps.push(Step::Dir(stage.join("home")));
        steps.push(Step::Bind(home, stage.join("home")));
    }
    if !names.iter().any(|n| n == "tmp") {
        steps.push(Step::Dir(stage.join("tmp")));
    }
    steps.push(Step::Dir(stage.join("dev")));
    for d in DEVICES {
        let host = dev.join(d);
        if host.exists() {
            steps.push(Step::File(stage.join("dev").join(d)));
            steps.push(Step::Bind(host, stage.join("dev").join(d)));
        }
    }
    steps.push(Step::Dir(stage.join("proc")));
    steps.push(Step::Proc(stage.join("proc")));
    Ok(steps)
}

/// Where an IFS path is in the view: paths under `/QOpenSys` and
/// `/home` keep their spelling; others are not there.
#[must_use]
pub fn inside(ifs_path: &str) -> Option<String> {
    let up = ifs_path.to_ascii_uppercase();
    let keep = up == "/QOPENSYS"
        || ifs_path.starts_with("/QOpenSys/")
        || up == "/HOME"
        || up.starts_with("/HOME/");
    if !keep {
        return None;
    }
    // `/home` is case-insensitive in the IFS but not in the view.
    if up.starts_with("/HOME") {
        return Some(format!("/home{}", &ifs_path[5..]));
    }
    Some(ifs_path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_inside_the_view() {
        assert_eq!(inside("/QOpenSys/tmp/x").as_deref(), Some("/QOpenSys/tmp/x"));
        assert_eq!(inside("/HOME/QSECOFR").as_deref(), Some("/home/QSECOFR"));
        assert_eq!(inside("/QSYS.LIB/QGPL.LIB"), None);
        assert_eq!(inside("/qopensys/tmp"), None);
    }

    #[test]
    fn the_plan_mirrors_qopensys() {
        let base = std::env::temp_dir().join(format!("pase-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("QOpenSys/usr/bin")).unwrap();
        std::fs::create_dir_all(base.join("QOpenSys/tmp")).unwrap();
        std::fs::create_dir_all(base.join("root/home/A")).unwrap();
        std::fs::write(base.join("QOpenSys/f"), b"x").unwrap();
        let stage = Path::new("/stage");
        let steps = plan(&base, None, stage, &base.join("nodev")).unwrap();
        assert!(steps.contains(&Step::Bind(base.join("QOpenSys/usr"), stage.join("usr"))));
        assert!(steps.contains(&Step::File(stage.join("f"))));
        assert!(steps.contains(&Step::Bind(base.join("QOpenSys"), stage.join("QOpenSys"))));
        assert!(steps.contains(&Step::Bind(base.join("root/home"), stage.join("home"))));
        assert!(steps.contains(&Step::Proc(stage.join("proc"))));
        assert_eq!(
            steps.iter().filter(|s| **s == Step::Dir(stage.join("tmp"))).count(),
            1
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
