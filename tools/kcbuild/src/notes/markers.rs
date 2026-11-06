// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The marker inventory: every `BLOCKED(...)`, `TODO(...)`,
//! `MIGRATE(...)` and `NEEDS-CODY` in the tree's text files, with its
//! place and reason. A marker written in backticks is a mention (a
//! document naming the form), not a marker.

use std::fs;
use std::path::Path;
use std::process::Command;

/// One marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// The marker as written: `BLOCKED(rel)`, `NEEDS-CODY`.
    pub form: String,
    /// The file, relative to the tree, with `/`.
    pub file: String,
    /// The line, from 1.
    pub line: usize,
    /// What the line says about it.
    pub reason: String,
}

impl Marker {
    /// Whether the form survives the release: `BLOCKED(rel)` and
    /// `NEEDS-CODY` only.
    #[must_use]
    pub fn released(&self) -> bool {
        self.form == "BLOCKED(rel)" || self.form == "NEEDS-CODY"
    }
}

/// Files the scan leaves out: the notes it writes into, and the
/// scanner with its fixtures.
pub const SKIP: &[&str] = &["docs/RELEASE_NOTES_V1R1M0.md", "tools/kcbuild/"];

const PARENS: &[&str] = &["BLOCKED(", "TODO(", "MIGRATE("];
const PLAIN: &str = "NEEDS-CODY";

/// The markers on one line.
#[must_use]
pub fn on_line(file: &str, line: usize, text: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut found: Vec<(usize, usize)> = Vec::new();
    for p in PARENS {
        let mut from = 0;
        while let Some(i) = text[from..].find(p).map(|i| i + from) {
            let rest = &text[i + p.len()..];
            let n = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').count();
            if n > 0 && rest[n..].starts_with(')') {
                found.push((i, i + p.len() + n + 1));
            }
            from = i + p.len();
        }
    }
    let mut from = 0;
    while let Some(i) = text[from..].find(PLAIN).map(|i| i + from) {
        found.push((i, i + PLAIN.len()));
        from = i + PLAIN.len();
    }
    found.sort_unstable();
    for (k, &(s, e)) in found.iter().enumerate() {
        if text[..s].ends_with('`') {
            continue;
        }
        let next = found.get(k + 1).map_or(text.len(), |n| n.0);
        out.push(Marker { form: text[s..e].to_string(), file: file.to_string(), line, reason: reason(text, s, e, next) });
    }
    out
}

/// The text after the marker up to the next one, else the text before it.
fn reason(text: &str, start: usize, end: usize, next: usize) -> String {
    let tidy = |s: &str| {
        let s = s.trim().trim_start_matches([':', '-', ',', ';', ')', '.']).trim();
        let s = s.trim_end_matches("*/").trim_end_matches("-->").trim_end_matches('|').trim();
        s.trim_start_matches("//").trim_start_matches(['#', '-', '|', '*', '!', '/']).trim().to_string()
    };
    let after = tidy(&text[end..next.max(end)]);
    let r = if after.chars().filter(char::is_ascii_alphanumeric).count() >= 3 { after } else { tidy(&text[..start]) };
    let r: String = r.chars().take(200).collect();
    r.replace('|', "\\|")
}

/// The tree's text files: `git ls-files` when it answers, else a walk
/// that leaves out `.git` and `target` (a worktree whose `.git` names a
/// Windows path, read from WSL, has no working `git`).
fn files(repo: &Path) -> Vec<String> {
    if let Ok(o) = Command::new("git").arg("-C").arg(repo).args(["ls-files", "-z"]).output()
        && o.status.success()
    {
        let mut v: Vec<String> =
            o.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect();
        v.sort();
        return v;
    }
    let mut v = Vec::new();
    walk(repo, repo, &mut v);
    v.sort();
    v
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "target" {
            continue;
        }
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Every marker in the tree at `repo`, by file and line.
#[must_use]
pub fn scan(repo: &Path) -> Vec<Marker> {
    let mut out = Vec::new();
    for f in files(repo) {
        if SKIP.iter().any(|s| f == *s || (s.ends_with('/') && f.starts_with(s))) {
            continue;
        }
        let Ok(bytes) = fs::read(repo.join(&f)) else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        for (i, l) in text.lines().enumerate() {
            out.extend(on_line(&f, i + 1, l));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::on_line;

    #[test]
    fn markers_and_mentions() {
        let m = on_line("a.rs", 3, "    // BLOCKED(sql): no column descriptions come back");
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].form.as_str(), m[0].reason.as_str()), ("BLOCKED(sql)", "no column descriptions come back"));
        assert!(!m[0].released());
        assert!(on_line("CLAUDE.md", 1, "marked `BLOCKED(phaseN)` (with the reason)").is_empty());
        let m = on_line("t.md", 9, "| Hyper-V Gen 2 | UEFI | NEEDS-CODY |");
        assert_eq!(m[0].reason, "Hyper-V Gen 2 \\| UEFI");
        assert!(m[0].released());
        let m = on_line("x.rs", 1, "// TODO(perf) and BLOCKED(rel): two here");
        assert_eq!(m.iter().map(|m| m.form.as_str()).collect::<Vec<_>>(), ["TODO(perf)", "BLOCKED(rel)"]);
        assert_eq!(m[0].reason, "and");
        assert!(on_line("x.rs", 1, "BLOCKED() and TODO(x y)").is_empty());
    }
}
