// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CSV the oracle reads and writes: no header, every text field
//! quoted, `""` for a quote, CRLF or LF.

use std::fmt::Write as _;

use super::{Case, Op, Row};

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// Split one line into fields.
fn split(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
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
            ',' if !quoted => fields.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    fields.push(cur);
    fields
}

/// Cases to CSV: `id,op,sa,a,sb,b`.
#[must_use]
pub fn write_cases(cases: &[Case]) -> String {
    let mut out = String::new();
    for c in cases {
        let _ = write!(
            out,
            "{},{},{},{},{},{}\r\n",
            c.id,
            quote(c.op.symbol()),
            c.shape_a,
            quote(&c.a),
            c.shape_b,
            quote(&c.b)
        );
    }
    out
}

/// CSV to cases.
pub fn read_cases(text: &str) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f = split(line);
        let bad = |what: &str| format!("cases line {}: {what}", n + 1);
        if f.len() != 6 {
            return Err(bad("expected 6 fields"));
        }
        let op = Op::parse(f[1].trim(), f[5].trim()).ok_or_else(|| bad("unknown operation"))?;
        let shape = |s: &str| {
            s.trim()
                .parse::<usize>()
                .ok()
                .filter(|i| *i < super::SHAPES.len())
                .ok_or_else(|| bad("bad shape index"))
        };
        cases.push(Case {
            id: f[0].trim().parse().map_err(|_| bad("bad id"))?,
            op,
            shape_a: shape(&f[2])?,
            a: f[3].trim().to_string(),
            shape_b: shape(&f[4])?,
            b: f[5].trim().to_string(),
        });
    }
    Ok(cases)
}

/// Rows to CSV: `id,inter,r1,rh,rr`.
#[must_use]
pub fn write_rows(rows: &[Row]) -> String {
    let mut out = String::new();
    for r in rows {
        let _ = write!(
            out,
            "{},{},{},{},{}\r\n",
            r.id,
            quote(&r.inter),
            quote(&r.r1),
            quote(&r.rh),
            quote(&r.rr)
        );
    }
    out
}

/// CSV to rows. Trailing blanks of each field are dropped, as
/// `CPYTOIMPF` drops them.
pub fn read_rows(text: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let f = split(line);
        let bad = |what: &str| format!("results line {}: {what}", n + 1);
        if f.len() != 5 {
            return Err(bad("expected 5 fields"));
        }
        let t = |s: &str| s.trim_end().to_string();
        rows.push(Row {
            id: f[0].trim().parse().map_err(|_| bad("bad id"))?,
            inter: t(&f[1]),
            r1: t(&f[2]),
            rh: t(&f[3]),
            rr: t(&f[4]),
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips() {
        assert_eq!(
            split("1,\"a,b\",\"say \"\"hi\"\"\",,x"),
            ["1", "a,b", "say \"hi\"", "", "x"]
        );
        let rows = vec![Row {
            id: 7,
            inter: "[ 1,234.56-]".into(),
            r1: "E00103".into(),
            rh: String::new(),
            rr: "\"q\"".into(),
        }];
        let text = write_rows(&rows);
        assert_eq!(read_rows(&text).unwrap(), rows);
        let cases = vec![Case {
            id: 1,
            op: Op::EditCode {
                code: 'J',
                fill: super::super::FillKind::Asterisk,
            },
            shape_a: 0,
            a: "-12.34".into(),
            shape_b: 0,
            b: "J*".into(),
        }];
        assert_eq!(read_cases(&write_cases(&cases)).unwrap(), cases);
        assert!(read_cases("1,\"?\",0,\"1\",0,\"1\"").is_err());
    }
}
