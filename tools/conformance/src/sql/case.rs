// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A case file: its directives and its statements. The same rules as
//! `SqlKit.Case` on the oracle side.

use std::fs;
use std::path::{Path, PathBuf};

/// Which system settles a case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Oracle {
    /// Cody's V7R1M0 box.
    Primary,
    /// pub400, V7R5M0.
    Pub400,
    /// No oracle: QS/400's own behaviour, compared with itself.
    None,
}

/// One case.
#[derive(Clone, Debug)]
pub struct Case {
    /// Its folder under `cases/`.
    pub area: String,
    /// Its name, the file name without `.sql`.
    pub name: String,
    /// Which system settles it.
    pub oracle: Oracle,
    /// The statements, `{lib}` not yet replaced.
    pub statements: Vec<String>,
    /// The file.
    pub path: PathBuf,
}

impl Case {
    /// Read a case file.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let area = path
            .parent()
            .and_then(Path::file_name)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut oracle = None;
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("--") else {
                break;
            };
            if let Some(o) = rest.trim().strip_prefix("oracle:") {
                let o = o.trim();
                oracle = Some(match o.split_whitespace().next().unwrap_or("") {
                    "primary" => Oracle::Primary,
                    "pub400" => Oracle::Pub400,
                    "none" => Oracle::None,
                    other => return Err(format!("{}: unknown oracle {other:?}", path.display())),
                });
            }
        }
        let oracle = oracle.ok_or_else(|| format!("{}: no `-- oracle:` line", path.display()))?;
        Ok(Self { area, name, oracle, statements: statements(&text), path: path.to_path_buf() })
    }
}

/// Split a case into statements: each ends with the terminator at the end
/// of a line (`;`, or what `--#SET TERMINATOR x` set); comment and blank
/// lines between statements are skipped.
#[must_use]
pub fn statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut term = ";".to_string();
    let mut cur = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.to_ascii_uppercase().starts_with("--#SET TERMINATOR") {
            term = t["--#SET TERMINATOR".len()..].trim().to_string();
            continue;
        }
        if cur.is_empty() && (t.is_empty() || t.starts_with("--")) {
            continue;
        }
        if t.ends_with(term.as_str()) {
            let cut = line.rfind(term.as_str()).expect("ends with it");
            cur.push_str(&line[..cut]);
            out.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The first line of a statement as the outcome shows it.
#[must_use]
pub fn first(st: &str) -> String {
    let l = st.lines().next().unwrap_or("").trim();
    l.chars().take(72).collect()
}

/// Whether the statement changes rows (its count is shown).
#[must_use]
pub fn changes(st: &str) -> bool {
    let w = st.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
    matches!(w.as_str(), "INSERT" | "UPDATE" | "DELETE" | "MERGE")
}

/// Every case under `root/cases`, sorted by area and name.
pub fn all(root: &Path) -> Result<Vec<Case>, String> {
    let mut out = Vec::new();
    let cases = root.join("cases");
    let Ok(areas) = fs::read_dir(&cases) else {
        return Ok(out);
    };
    let mut paths = Vec::new();
    for a in areas.flatten() {
        if let Ok(files) = fs::read_dir(a.path()) {
            for f in files.flatten() {
                if f.path().extension().is_some_and(|e| e == "sql") {
                    paths.push(f.path());
                }
            }
        }
    }
    paths.sort();
    for p in paths {
        out.push(Case::read(&p)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_split_on_the_terminator() {
        let t = "-- oracle: primary\nSELECT 1\n  FROM X;\n\n--#SET TERMINATOR @\nBEGIN\n SET A = 1;\nEND@\n--#SET TERMINATOR ;\nVALUES 2;";
        assert_eq!(statements(t), vec!["SELECT 1\n  FROM X", "BEGIN\n SET A = 1;\nEND", "VALUES 2"]);
        assert!(changes("insert into t values 1"));
        assert!(!changes("SELECT 1"));
    }
}
