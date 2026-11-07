// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The help is ours: no file that holds help text may use one of IBM's
//! phrasings (`command/sys-cpps/denylist.txt`, as the hygiene tests
//! read it).

use std::path::Path;

use crate::{Category, Result, files, read};

/// Where help text lives, and the extension of its files.
const SOURCES: &[(&str, &str)] = &[
    ("ui/uim/pnlgrp", "pnlgrp"),
    ("ui/menus/sys", "pnlgrp"),
    ("ui/menus/pnlgrp", "pnlgrp"),
    ("ui/dspf/sys", "dspf"),
    ("command/sys-cpps/help", "toml"),
];

/// `phrase` in `text` as whole words.
fn has_phrase(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Check every help source; a file counts once.
pub fn audit(root: &Path) -> Result<Category> {
    let denied: Vec<String> = read(&root.join("command/sys-cpps/denylist.txt"))?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect();
    let mut c = Category::new("our own phrasing", 100);
    for (dir, ext) in SOURCES {
        for p in files(&root.join(dir), ext)? {
            let lower = read(&p)?.to_ascii_lowercase();
            let hits: Vec<&String> = denied.iter().filter(|d| has_phrase(&lower, d)).collect();
            c.check(hits.is_empty(), || {
                format!(
                    "{dir}/{}: {hits:?}",
                    p.file_name().unwrap_or_default().to_string_lossy()
                )
            });
        }
    }
    Ok(c)
}
