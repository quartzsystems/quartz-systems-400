// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The services' shapes against pub400's capture: `shapes.txt` is what
//! `SVCCOLS.out` says. `QS400_UPDATE_SHAPES=1` rewrites it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A kit row's values, quotes taken off; `None` for `NULL`.
fn values(line: &str) -> Vec<Option<String>> {
    let body = line.trim().trim_start_matches('|').trim_end_matches('|');
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    cur.push('\'');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '\'' {
            quoted = true;
            was_quoted = true;
            cur.clear();
        } else if c == '|' {
            out.push(take(&mut cur, &mut was_quoted));
        } else if !was_quoted {
            cur.push(c);
        }
        i += 1;
    }
    out.push(take(&mut cur, &mut was_quoted));
    out
}

fn take(cur: &mut String, was_quoted: &mut bool) -> Option<String> {
    let v = if *was_quoted {
        cur.clone()
    } else {
        cur.trim().to_string()
    };
    cur.clear();
    let q = std::mem::take(was_quoted);
    if !q && v == "NULL" { None } else { Some(v) }
}

/// The capture's sections: each statement's rows.
fn sections(text: &str) -> Vec<Vec<Vec<Option<String>>>> {
    let mut out: Vec<Vec<Vec<Option<String>>>> = Vec::new();
    for line in text.lines() {
        if line.starts_with("> ") {
            out.push(Vec::new());
        } else if line.starts_with("| ")
            && let Some(s) = out.last_mut()
        {
            s.push(values(line));
        }
    }
    out
}

fn v(r: &[Option<String>], i: usize) -> String {
    r.get(i).cloned().flatten().unwrap_or_default()
}

fn n(r: &[Option<String>], i: usize) -> u32 {
    v(r, i).parse().unwrap_or(0)
}

/// A `SYSPARMS` type as `SYSCOLUMNS` spells it, with its length.
fn parm_type(ty: &str, chars: u32, prec: u32) -> (String, u32) {
    let t = match ty {
        "CHARACTER VARYING" => "VARCHAR",
        "CHARACTER" => "CHAR",
        "GRAPHIC VARYING" => "VARG",
        "CHARACTER LARGE OBJECT" => "CLOB",
        "DOUBLE-BYTE CHARACTER LARGE OBJECT" => "DBCLOB",
        "BINARY LARGE OBJECT" => "BLOB",
        "BINARY VARYING" => "VARBIN",
        "TIMESTAMP" => return ("TIMESTMP".into(), chars.saturating_sub(20).min(12)),
        "DOUBLE PRECISION" => "DOUBLE",
        other => other,
    };
    let len = match t {
        "SMALLINT" | "INTEGER" | "BIGINT" | "DECIMAL" | "NUMERIC" | "DOUBLE" => prec,
        "DATE" | "TIME" | "XML" => 0,
        _ => chars,
    };
    (t.into(), len)
}

/// One word for the shapes file: blanks would split it.
fn word(s: &str) -> String {
    if s.is_empty() {
        "-".into()
    } else {
        s.replace(' ', "_")
    }
}

type Pair = (String, String);

fn generate(capture: &str) -> String {
    let s = sections(capture);
    assert_eq!(s.len(), 4, "SVCCOLS has four statements");
    let mut views: BTreeMap<(String, String), (String, Vec<String>)> = BTreeMap::new();
    for r in &s[0] {
        if v(r, 3) == "V" || v(r, 3) == "T" {
            views.insert((v(r, 0), v(r, 1)), (v(r, 2).trim().to_string(), Vec::new()));
        }
    }
    for r in &s[1] {
        let Some((_, cols)) = views.get_mut(&(v(r, 0), v(r, 1))) else {
            continue;
        };
        let ty = v(r, 5);
        let len = match ty.as_str() {
            "TIMESTMP" => (n(r, 6).saturating_sub(7)) * 2,
            "SMALLINT" => 5,
            "INTEGER" => 10,
            "BIGINT" => 19,
            "DATE" | "TIME" => 0,
            _ => n(r, 6),
        };
        cols.push(format!(
            "c {} {} {} {} {} {} {}",
            v(r, 3),
            word(v(r, 4).trim()),
            ty,
            len,
            n(r, 7),
            v(r, 8),
            n(r, 9)
        ));
    }
    let mut kinds: BTreeMap<(String, String), (String, String)> = BTreeMap::new();
    for r in &s[2] {
        let kind = match (v(r, 3).as_str(), v(r, 4).as_str()) {
            ("PROCEDURE", _) => "P",
            (_, "T") => "T",
            _ => "S",
        };
        kinds.insert((v(r, 0), v(r, 2)), (v(r, 1), kind.into()));
    }
    let mut parms: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for r in &s[3] {
        let key = (v(r, 0), v(r, 1));
        let (ty, len) = parm_type(&v(r, 6), n(r, 7), n(r, 8));
        let null = if v(r, 10) == "YES" { "Y" } else { "N" };
        let line = match v(r, 2).as_str() {
            "P" => format!(
                "p {} {} {ty} {len} {} {null} {} {}",
                v(r, 4),
                v(r, 5),
                n(r, 9),
                n(r, 11),
                r.get(12)
                    .cloned()
                    .flatten()
                    .map_or_else(|| "-".into(), |d| d.trim().to_string())
            ),
            "R" => format!("c {} - {ty} {len} {} {null} {}", v(r, 5), n(r, 9), n(r, 11)),
            _ => format!("r {ty} {len} {} {}", n(r, 9), n(r, 11)),
        };
        parms.entry(key).or_default().push(line);
    }
    let mut out = String::new();
    for ((schema, name), (system, cols)) in &views {
        let _ = writeln!(out, "V {schema} {name} {system}");
        for c in cols {
            let _ = writeln!(out, "{c}");
        }
    }
    let mut routines: Vec<(&Pair, &Pair)> = kinds.iter().collect();
    routines.sort_by(|a, b| (&a.0.0, &a.1.0, &a.0.1).cmp(&(&b.0.0, &b.1.0, &b.0.1)));
    for ((schema, specific), (name, kind)) in routines {
        let _ = writeln!(out, "F {schema} {name} {specific} {kind}");
        for l in parms
            .get(&(schema.clone(), specific.clone()))
            .into_iter()
            .flatten()
        {
            let _ = writeln!(out, "{l}");
        }
    }
    out
}

#[test]
fn shapes_are_the_capture() {
    let capture =
        std::fs::read_to_string(root().join("tools/conformance/sql/expected/services/SVCCOLS.out"))
            .expect("the capture");
    let want = generate(&capture.replace("\r\n", "\n"));
    let path = root().join("database/sql/src/exec/catalog/services/shapes.txt");
    if std::env::var("QS400_UPDATE_SHAPES").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &want).expect("write shapes.txt");
        return;
    }
    let have = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        have == want,
        "shapes.txt differs from the capture; QS400_UPDATE_SHAPES=1 rewrites it"
    );
}
