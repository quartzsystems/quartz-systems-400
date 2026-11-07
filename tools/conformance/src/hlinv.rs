// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HLINV's source, when it is there: the members under
//! `tools/conformance/hlinv/src/` (gitignored; the source is not ours to
//! ship) and the application's files built from its DDS, as
//! `tests/hlinv.rs` builds them.

use std::path::{Path, PathBuf};

use rpg_oracle::case::{DbStep, DdsKind};

use crate::rpg::files::KitDb;

/// `hlinv/src`.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("hlinv/src")
}

/// The members whose extension is one of `exts` (any case), in the
/// folder or one level down, sorted; none when the source is absent.
#[must_use]
pub fn members(exts: &[&str]) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            found.extend(
                std::fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path()),
            );
        } else {
            found.push(p);
        }
    }
    let mut out: Vec<PathBuf> = found
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| exts.iter().any(|e| x.eq_ignore_ascii_case(e)))
        })
        .collect();
    out.sort();
    out
}

/// A member's name (its file stem, upper case) and its lines.
#[must_use]
pub fn read(p: &Path) -> (String, Vec<String>) {
    let text = String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).replace("\r\n", "\n");
    let name = p
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_uppercase())
        .unwrap_or_default();
    (name, text.lines().map(str::to_string).collect())
}

/// The application's files, physical before the files over them; a
/// file whose DDS does not compile is left out.
#[must_use]
pub fn db() -> Option<KitDb> {
    let mut kept: Vec<DbStep> = Vec::new();
    for kind in [DdsKind::Pf, DdsKind::Lf, DdsKind::Dspf, DdsKind::Prtf] {
        for p in members(&[kind.ext()]) {
            let (name, source) = read(&p);
            let mut trial = kept.clone();
            trial.push(DbStep::Dds { name, kind, source });
            if KitDb::build(&trial).is_ok() {
                kept = trial;
            }
        }
    }
    KitDb::build(&kept).ok()
}
