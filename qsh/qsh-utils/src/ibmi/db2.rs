// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `db2 [-Stv] [-f file] [statement ...]`: run SQL statements. The
//! operands are one statement; without them the statements come from
//! `-f file` or standard input, one a line, or ended by `;` with `-t`
//! (`--` starts a comment). `-S` uses system naming, `-v` shows each
//! statement first. A result set prints as a blank line, the column
//! names, dashes, the rows, a blank line, `  n RECORD(S) SELECTED.` and
//! a blank line (the form scripts read); other statements print
//! `DB20000I  THE SQL COMMAND COMPLETED SUCCESSFULLY.` and failures the
//! `**** CLI ERROR *****` block with the SQLSTATE and code. Status 0,
//! or 1 when a statement failed.

use qsh::sys::{SqlColumn, SqlOutcome, SqlRequest, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::{Out, read_input};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "db2";

/// Split a script into statements: at `;` outside quotes with
/// `semicolons`, else one a line; `--` comments and blank statements
/// are dropped.
pub fn statements(text: &str, semicolons: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    let mut push = |cur: &mut String| {
        let t = cur.trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        cur.clear();
    };
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '-' if chars.peek() == Some(&'-') => {
                    for d in chars.by_ref() {
                        if d == '\n' {
                            if semicolons {
                                cur.push(' ');
                            } else {
                                push(&mut cur);
                            }
                            break;
                        }
                    }
                }
                ';' if semicolons => push(&mut cur),
                '\n' if !semicolons => push(&mut cur),
                '\r' => {}
                _ => cur.push(if c == '\n' { ' ' } else { c }),
            },
        }
    }
    push(&mut cur);
    out
}

fn width(c: &SqlColumn, rows: &[Vec<Option<String>>], i: usize) -> usize {
    let longest = rows
        .iter()
        .filter_map(|r| r.get(i))
        .map(|v| v.as_deref().map_or(1, |s| s.chars().count()))
        .max()
        .unwrap_or(0);
    c.width.max(c.name.chars().count()).max(longest)
}

fn pad(s: &str, w: usize, right: bool) -> String {
    if right {
        format!("{s:>w$}")
    } else {
        format!("{s:<w$}")
    }
}

/// A result set as `db2` prints it.
pub fn table(columns: &[SqlColumn], rows: &[Vec<Option<String>>]) -> String {
    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| width(c, rows, i))
        .collect();
    let line = |cells: Vec<String>| cells.join(" ").trim_end().to_string() + "\n";
    let mut s = String::from("\n");
    s.push_str(&line(
        columns
            .iter()
            .zip(&widths)
            .map(|(c, w)| pad(&c.name, *w, false))
            .collect(),
    ));
    s.push_str(&line(widths.iter().map(|w| "-".repeat(*w)).collect()));
    for r in rows {
        s.push_str(&line(
            columns
                .iter()
                .zip(&widths)
                .enumerate()
                .map(|(i, (c, w))| {
                    let v = r.get(i).cloned().flatten();
                    pad(v.as_deref().unwrap_or("-"), *w, c.numeric)
                })
                .collect(),
        ));
    }
    s.push_str(&format!("\n  {} RECORD(S) SELECTED.\n\n", rows.len()));
    s
}

/// Run `db2`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "Stvf:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let stmts = if p.operands.is_empty() {
        let src = p.value('f').unwrap_or("-");
        match read_input(cx, src) {
            Ok(b) => statements(&String::from_utf8_lossy(&b), p.has('t')),
            Err(e) => {
                complain_sys(cx, NAME, src, e);
                return 1;
            }
        }
    } else {
        if p.has('f') {
            return usage(cx, NAME, "Give a statement or -f, not both.");
        }
        let s = p.operands.join(" ");
        let s = s.trim().trim_end_matches(';').trim().to_string();
        if s.is_empty() { Vec::new() } else { vec![s] }
    };
    let runner = match cx.sys.sql_runner() {
        Ok(r) => r,
        Err(e) => {
            complain(
                cx,
                NAME,
                &format!("SQL statements cannot be run: {}", e.errno.text()),
            );
            return 1;
        }
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for st in stmts {
        if p.has('v') {
            out.text(&st);
            out.text("\n");
        }
        let req = SqlRequest {
            statement: st,
            system_naming: p.has('S'),
        };
        match runner.run_sql(&req) {
            Ok(SqlOutcome::Rows { columns, rows }) => out.text(&table(&columns, &rows)),
            Ok(SqlOutcome::Done { .. }) => {
                out.text("DB20000I  THE SQL COMMAND COMPLETED SUCCESSFULLY.\n")
            }
            Ok(SqlOutcome::Error {
                sqlstate,
                code,
                text,
            }) => {
                status = 1;
                out.text(&format!(
                    " **** CLI ERROR *****\n         SQLSTATE: {sqlstate}\nNATIVE ERROR CODE: {code}\n{text}\n"
                ))
            }
            Err(e) => {
                status = 1;
                out.flush();
                complain(
                    cx,
                    NAME,
                    &format!("The statement could not be run: {}", e.errno.text()),
                );
                true
            }
        };
    }
    out.finish(cx, NAME, status)
}

#[cfg(test)]
mod tests {
    use super::statements;

    #[test]
    fn splitting() {
        assert_eq!(
            statements("select 1;\nselect ';' from x; -- c;\n values 2", true),
            vec!["select 1", "select ';' from x", "values 2"]
        );
        assert_eq!(
            statements("a\n\n-- note\nb -- tail\n", false),
            vec!["a", "b"]
        );
    }
}
