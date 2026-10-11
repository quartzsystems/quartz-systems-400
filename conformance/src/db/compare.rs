// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Comparing the in-memory outputs with the machine's. Data files are
//! compared row by row with numbers by value and text trimmed; the
//! outfile-derived tables by the columns we produce, through the IBM
//! column names the oracle README lists.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use qdecimal::Decimal;

/// What matched, what was not there to compare, what differed.
#[derive(Debug, Default)]
pub struct Comparison {
    /// Files that compared equal.
    pub matched: Vec<String>,
    /// Files the expected directory does not have.
    pub missing: Vec<String>,
    /// Files that differ, with the first difference.
    pub mismatched: Vec<(String, String)>,
}

impl Comparison {
    /// One line per file.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for m in &self.matched {
            let _ = writeln!(out, "  ok       {m}");
        }
        for m in &self.missing {
            let _ = writeln!(out, "  missing  {m}");
        }
        for (m, why) in &self.mismatched {
            let _ = writeln!(out, "  DIFFERS  {m}: {why}");
        }
        out
    }
}

/// IBM outfile column names for ours, per file.
fn aliases(file: &str) -> &'static [(&'static str, &'static str)] {
    match file {
        "ffd.csv" => &[
            ("FILE", "WHFILE"),
            ("FORMAT", "WHNAME"),
            ("FIELD", "WHFLDE"),
            ("TYPE", "WHFLDT"),
            ("DIGITS", "WHFLDD"),
            ("DECIMALS", "WHFLDP"),
            ("BYTES", "WHFLDB"),
            ("POSITION", "WHFOBO"),
            ("NULL", "WHNULL"),
        ],
        "lvlid.csv" => &[
            ("FILE", "RFFILE"),
            ("FORMAT", "RFNAME"),
            ("LEVELID", "RFID"),
        ],
        "dspjrn.csv" => &[
            ("SEQ", "JOSEQN"),
            ("CODE", "JOCODE"),
            ("TYPE", "JOENTT"),
            ("OBJECT", "JOOBJ"),
            ("MEMBER", "JOMBR"),
            ("COUNT", "JOCTRR"),
            ("CYCLE", "JOCCID"),
        ],
        "rla.csv" => &[
            ("LINE", "LINE"),
            ("STATUS", "STATUS"),
            ("FLAGS", "FLAGS"),
            ("DETAIL", "DETAIL"),
        ],
        _ => &[],
    }
}

/// Columns compared by position only, never by value (sequence numbers
/// and cycle identifiers are the machine's own).
const UNCOMPARED: &[&str] = &["SEQ", "CYCLE"];

/// Parse CSV: quoted fields with doubled quotes, comma separated.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut row = Vec::new();
        let mut cur = String::new();
        let mut quoted = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '"' if quoted && chars.peek() == Some(&'"') => {
                    chars.next();
                    cur.push('"');
                }
                '"' => quoted = !quoted,
                ',' if !quoted => row.push(std::mem::take(&mut cur)),
                c => cur.push(c),
            }
        }
        row.push(cur);
        rows.push(row);
    }
    rows
}

/// A cell in the form both sides compare equal in: numbers canonical,
/// dates, times and timestamps as digits, text trimmed.
fn normalize(cell: &str) -> String {
    let t = cell.trim();
    if t.is_empty() {
        return String::new();
    }
    if let Ok(d) = Decimal::parse(t) {
        let text = d.to_string();
        let trimmed = if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.')
        } else {
            text.as_str()
        };
        return match trimmed {
            "" | "-" | "-0" => "0".to_string(),
            t => t.to_string(),
        };
    }
    if let Ok(f) = t.parse::<f64>() {
        let text = format!("{f}");
        let trimmed = if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.')
        } else {
            text.as_str()
        };
        return trimmed.to_string();
    }
    let digits = t.chars().filter(char::is_ascii_digit).count();
    if digits >= 6 && t.chars().all(|c| c.is_ascii_digit() || "-.: /".contains(c)) {
        return t.chars().filter(char::is_ascii_digit).collect();
    }
    t.to_ascii_uppercase()
}

fn same(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// Compare two headerless row lists.
fn compare_rows(ours: &[Vec<String>], theirs: &[Vec<String>]) -> Result<(), String> {
    if ours.len() != theirs.len() {
        return Err(format!("{} rows here, {} there", ours.len(), theirs.len()));
    }
    for (i, (a, b)) in ours.iter().zip(theirs).enumerate() {
        if a.len() != b.len() {
            return Err(format!(
                "row {}: {} fields here, {} there",
                i + 1,
                a.len(),
                b.len()
            ));
        }
        for (j, (x, y)) in a.iter().zip(b).enumerate() {
            if !same(x, y) {
                return Err(format!(
                    "row {} field {}: {x:?} here, {y:?} there",
                    i + 1,
                    j + 1
                ));
            }
        }
    }
    Ok(())
}

/// Project a table with a header onto `columns` by name, through
/// `alias` when the plain name is absent.
fn project(
    rows: &[Vec<String>],
    columns: &[&str],
    alias: &[(&str, &str)],
) -> Result<Vec<Vec<String>>, String> {
    let header: Vec<String> = rows
        .first()
        .ok_or("empty table")?
        .iter()
        .map(|h| h.trim().to_ascii_uppercase())
        .collect();
    let mut index = Vec::new();
    for c in columns {
        let other = alias.iter().find(|(o, _)| o == c).map(|(_, t)| *t);
        let i = header
            .iter()
            .position(|h| h == c || other.is_some_and(|o| h == o))
            .ok_or_else(|| format!("column {c} not found in {header:?}"))?;
        index.push(i);
    }
    Ok(rows[1..]
        .iter()
        .map(|r| {
            index
                .iter()
                .map(|&i| r.get(i).cloned().unwrap_or_default())
                .collect()
        })
        .collect())
}

/// Compare a header table: our columns, in our row order, the machine's
/// own identifiers skipped. `dspjrn.csv` compares record entries only.
fn compare_table(file: &str, ours: &str, theirs: &str) -> Result<(), String> {
    let a = parse_csv(ours);
    let b = parse_csv(theirs);
    let columns: Vec<&str> = a
        .first()
        .ok_or("empty table")?
        .iter()
        .map(String::as_str)
        .filter(|c| !UNCOMPARED.contains(c))
        .collect();
    let mut pa = project(&a, &columns, &[])?;
    let mut pb = project(&b, &columns, aliases(file))?;
    if file == "dspjrn.csv" {
        let code = columns
            .iter()
            .position(|c| *c == "CODE")
            .ok_or("no CODE column")?;
        pa.retain(|r| r[code].trim() == "R");
        pb.retain(|r| r[code].trim() == "R");
    }
    compare_rows(&pa, &pb)
}

/// Compare every file under `ours` with the same path under `expected`.
#[must_use]
pub fn compare(ours: &Path, expected: &Path) -> Comparison {
    let mut c = Comparison::default();
    let mut files = Vec::new();
    walk(ours, ours, &mut files);
    files.sort();
    for rel in files {
        if rel == "rla.log" {
            continue;
        }
        let theirs = expected.join(&rel);
        if !theirs.is_file() {
            c.missing.push(rel);
            continue;
        }
        let a = fs::read_to_string(ours.join(&rel)).unwrap_or_default();
        let b = fs::read_to_string(&theirs).unwrap_or_default();
        let result = if aliases(&rel).is_empty() {
            compare_rows(&parse_csv(&a), &parse_csv(&b))
        } else {
            compare_table(&rel, &a, &b)
        };
        match result {
            Ok(()) => c.matched.push(rel),
            Err(why) => c.mismatched.push((rel, why)),
        }
    }
    c
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_compare_by_value() {
        assert!(same("1500.00", "1500"));
        assert!(same("Alice   ", "ALICE"));
        assert!(same(
            "2026-02-01 09:30:00.000000",
            "2026-02-01-09.30.00.000000"
        ));
        assert!(same("1.5E+000", "1.5"));
        assert!(!same("1", "2"));
        assert!(same("", "  "));
    }

    #[test]
    fn tables_project_through_aliases() {
        let ours = "FILE,FORMAT,LEVELID\nCUSTOMER,CUSREC,3A1B2C3D4E5F6\n";
        let theirs = "RFFILE,RFLIB,RFNAME,RFID\nCUSTOMER,QSTEST,CUSREC,3A1B2C3D4E5F6\n";
        assert!(compare_table("lvlid.csv", ours, theirs).is_ok());
        let wrong = "RFFILE,RFLIB,RFNAME,RFID\nCUSTOMER,QSTEST,CUSREC,0\n";
        assert!(compare_table("lvlid.csv", ours, wrong).is_err());
    }

    #[test]
    fn quoted_fields_parse() {
        let rows = parse_csv("1,\"a, \"\"b\"\"\",\n");
        assert_eq!(
            rows,
            vec![vec!["1".to_string(), "a, \"b\"".into(), String::new()]]
        );
    }
}
