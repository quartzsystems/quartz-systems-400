// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Pathname expansion against a directory source, so `/QSYS.LIB` and
//! the case-insensitive root match as the file system names them.

use super::{Compiled, PChar, has_glob};

/// What pathname expansion needs from the file system.
pub trait Dirs {
    /// The names in a directory (absolute path), `None` when it cannot
    /// be read.
    fn names(&self, dir: &str) -> Option<Vec<String>>;
    /// Does the path exist (without following a final symbolic link)?
    fn exists(&self, path: &str) -> bool;
    /// Is the path a directory?
    fn is_dir(&self, path: &str) -> bool;
    /// Are names in this directory matched without regard to case?
    fn case_insensitive(&self, dir: &str) -> bool;
}

/// Join a directory and a name.
pub fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Resolve a path against the working directory.
pub fn absolute(cwd: &str, path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        join(cwd, path)
    }
}

/// Expand one field. Returns the matches sorted, or an empty list when
/// nothing matches (the caller keeps the field).
pub fn expand(field: &[PChar], cwd: &str, dirs: &dyn Dirs) -> Vec<String> {
    if !has_glob(field) {
        return Vec::new();
    }
    let comps: Vec<&[PChar]> = field.split(|c| c.c == '/').collect();
    let absolute_path = field.first().is_some_and(|c| c.c == '/');
    // (shown path, file system path)
    let mut cands: Vec<(String, String)> = vec![if absolute_path {
        ("/".to_string(), "/".to_string())
    } else {
        (String::new(), cwd.to_string())
    }];
    let start = usize::from(absolute_path);
    let last = comps.len() - 1;
    for (i, comp) in comps.iter().enumerate().skip(start) {
        let mut next = Vec::new();
        if comp.is_empty() {
            // A doubled or trailing slash: keep directories only.
            for (shown, fs) in cands {
                if i == last && !dirs.is_dir(&fs) {
                    continue;
                }
                next.push((format!("{shown}/"), fs));
            }
            cands = next;
            continue;
        }
        if has_glob(comp) {
            let pat = Compiled::new(comp);
            let dot = comp.first().is_some_and(|c| c.c == '.');
            for (shown, fs) in &cands {
                let Some(mut names) = dirs.names(fs) else {
                    continue;
                };
                names.sort();
                let icase = dirs.case_insensitive(fs);
                let pat = if icase {
                    Compiled::new(&fold_pat(comp))
                } else {
                    pat.clone()
                };
                for name in names {
                    if name.starts_with('.') && !dot {
                        continue;
                    }
                    if (name == "." || name == "..") && !dot {
                        continue;
                    }
                    let probe: Vec<char> = if icase {
                        name.chars().flat_map(char::to_lowercase).collect()
                    } else {
                        name.chars().collect()
                    };
                    if pat.matches(&probe) {
                        let fs_child = join(fs, &name);
                        if i != last && !dirs.is_dir(&fs_child) {
                            continue;
                        }
                        next.push((join_shown(shown, &name), fs_child));
                    }
                }
            }
        } else {
            let name: String = comp.iter().map(|c| c.c).collect();
            for (shown, fs) in &cands {
                let fs_child = join(fs, &name);
                if i == last && !dirs.exists(&fs_child) {
                    continue;
                }
                next.push((join_shown(shown, &name), fs_child));
            }
        }
        cands = next;
        if cands.is_empty() {
            break;
        }
    }
    let mut out: Vec<String> = cands.into_iter().map(|(s, _)| s).collect();
    out.sort();
    out
}

fn join_shown(shown: &str, name: &str) -> String {
    if shown.is_empty() || shown.ends_with('/') {
        format!("{shown}{name}")
    } else {
        format!("{shown}/{name}")
    }
}

/// The pattern with unquoted letters lowered, for case-insensitive
/// directories (names are lowered the same way).
fn fold_pat(p: &[PChar]) -> Vec<PChar> {
    p.iter()
        .map(|c| PChar {
            c: c.c.to_lowercase().next().unwrap_or(c.c),
            quoted: c.quoted,
        })
        .collect()
}
