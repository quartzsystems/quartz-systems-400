// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL conformance kit (`tools/conformance/sql/README.md`): case
//! files run here in memory and, through `oracle/SqlKit.java`, on the
//! oracle; the outcomes compared line by line.

pub mod case;
pub mod local;
pub mod render;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

pub use case::{Case, Oracle};

/// `tools/conformance/sql`.
#[must_use]
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("sql")
}

/// Where this side's outcomes are written.
#[must_use]
pub fn ours_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/conformance-sql/ours")
}

/// The captured outcome of a case, when there is one.
#[must_use]
pub fn expected(c: &Case) -> Option<String> {
    fs::read_to_string(root().join("expected").join(&c.area).join(format!("{}.out", c.name))).ok()
}

/// The outcome lines that are compared: all but the `-- captured from`
/// line, line ends normalized.
fn body(text: &str) -> Vec<&str> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with("-- captured from")).map(str::trim_end).collect();
    at_fetch(&lines)
}

/// An error a query raises before its first row reaches the requester
/// shows over JDBC at the first fetch (the status line 0, the columns,
/// then the error) or at the open; QS/400 raises it at the open. Both
/// compare as the error alone. An error after rows is kept as it is.
fn at_fetch<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("sqlcode 0 ") {
            let mut j = i + 1;
            while j < lines.len() && (lines[j].starts_with("# ") || lines[j].starts_with("warning ")) {
                j += 1;
            }
            if j > i + 1 && j < lines.len() && lines[j].starts_with("sqlcode -") {
                i = j;
                continue;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    out
}

/// The first difference between two outcomes, `None` when they agree.
#[must_use]
pub fn diff(ours: &str, theirs: &str) -> Option<String> {
    let (a, b) = (body(ours), body(theirs));
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied(), b.get(i).copied());
        if x != y {
            let stmt = a[..i.min(a.len())].iter().rev().find(|l| l.starts_with("> ")).copied().unwrap_or("");
            return Some(format!(
                "line {}: {stmt}\n    ours:   {}\n    oracle: {}",
                i + 1,
                x.unwrap_or("(end)"),
                y.unwrap_or("(end)")
            ));
        }
    }
    None
}

/// Cases known to differ from their capture (`known.txt`, one
/// `area/NAME` per line, `#` comments): work in progress, listed so a new
/// difference stands out. A listed case that agrees is reported stale.
#[must_use]
pub fn known() -> Vec<String> {
    fs::read_to_string(root().join("known.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// What one run of the kit found.
#[derive(Debug, Default)]
pub struct Report {
    /// Cases that agree with their capture.
    pub pass: Vec<String>,
    /// Cases that differ, with the first difference.
    pub fail: Vec<(String, String)>,
    /// Cases with no capture yet.
    pub uncaptured: Vec<String>,
    /// Cases that could not run.
    pub broken: Vec<(String, String)>,
}

impl Report {
    /// Differences not listed in [`known`], broken cases, and listed cases
    /// that now agree.
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let known = known();
        let mut out: Vec<String> = self
            .fail
            .iter()
            .filter(|(n, _)| !known.contains(n))
            .map(|(n, d)| format!("differ {n}: {d}"))
            .collect();
        out.extend(self.broken.iter().map(|(n, e)| format!("broken {n}: {e}")));
        out.extend(
            self.pass
                .iter()
                .filter(|n| known.contains(n))
                .map(|n| format!("stale in known.txt (agrees now): {n}")),
        );
        out
    }

    /// A text summary.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = format!(
            "{} pass, {} differ, {} not captured, {} broken\n",
            self.pass.len(),
            self.fail.len(),
            self.uncaptured.len(),
            self.broken.len()
        );
        for (n, d) in &self.fail {
            let _ = writeln!(s, "differ {n}: {d}");
        }
        for (n, e) in &self.broken {
            let _ = writeln!(s, "broken {n}: {e}");
        }
        for n in &self.uncaptured {
            let _ = writeln!(s, "not captured {n}");
        }
        s
    }
}

/// Run every case (or those whose `area/NAME` contains `only`), write our
/// outcomes under [`ours_dir`] and compare.
pub fn run_all(only: Option<&str>) -> Result<Report, String> {
    let mut r = Report::default();
    for c in case::all(&root())? {
        let id = format!("{}/{}", c.area, c.name);
        if only.is_some_and(|o| !id.contains(o)) {
            continue;
        }
        let ours = match local::run(&c) {
            Ok(t) => t,
            Err(e) => {
                r.broken.push((id, e));
                continue;
            }
        };
        let dir = ours_dir().join(&c.area);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        fs::write(dir.join(format!("{}.out", c.name)), &ours).map_err(|e| e.to_string())?;
        match expected(&c) {
            None => r.uncaptured.push(id),
            Some(theirs) => match diff(&ours, &theirs) {
                None => r.pass.push(id),
                Some(d) => r.fail.push((id, d)),
            },
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_before_the_first_row_compares_as_the_error() {
        let jdbc = "> SELECT 1\nsqlcode 0 sqlstate 00000 msgid -\n# 00001 INTEGER NOT NULL\nsqlcode -802 sqlstate 22012 msgid SQL0802\n";
        let ours = "> SELECT 1\nsqlcode -802 sqlstate 22012 msgid SQL0802\n";
        assert_eq!(diff(ours, jdbc), None);
        // Rows before the error stay a difference.
        let rows = "> SELECT 1\nsqlcode 0 sqlstate 00000 msgid -\n# 00001 INTEGER NOT NULL\n| 1 |\nsqlcode -802 sqlstate 22012 msgid SQL0802\n";
        assert!(diff(ours, rows).is_some());
    }
}
