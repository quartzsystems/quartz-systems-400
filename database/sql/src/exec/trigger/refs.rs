// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a trigger's body may hold, and what it uses: the statements a
//! trigger may not run (`SQL0751`), the transition variables it reads
//! (`SYSTRIGCOL`) and the tables it names (`SYSTRIGDEP`).

use crate::ast::{PlKind, PlStmt, Statement};
use crate::error::{Result, SqlError};
use crate::lex::{Tok, tokenize};

/// Every statement of a body, nested ones included.
fn each<'a>(s: &'a PlStmt, out: &mut Vec<&'a PlStmt>) {
    out.push(s);
    let all = |v: &'a [PlStmt], out: &mut Vec<&'a PlStmt>| v.iter().for_each(|s| each(s, out));
    match &s.kind {
        PlKind::Compound(c) => {
            all(&c.body, out);
            for h in &c.handlers {
                each(&h.body, out);
            }
        }
        PlKind::If {
            branches,
            otherwise,
        } => {
            for (_, b) in branches {
                all(b, out);
            }
            if let Some(o) = otherwise {
                all(o, out);
            }
        }
        PlKind::Case {
            whens, otherwise, ..
        } => {
            for (_, b) in whens {
                all(b, out);
            }
            if let Some(o) = otherwise {
                all(o, out);
            }
        }
        PlKind::Loop(b)
        | PlKind::While(_, b)
        | PlKind::Repeat(b, _)
        | PlKind::For { body: b, .. } => all(b, out),
        _ => {}
    }
}

/// A `BEFORE` trigger's body changes no data, so its `ATOMIC` compounds
/// have nothing to undo: they compile as plain ones.
pub(super) fn not_atomic(s: &mut PlStmt) {
    let all = |v: &mut [PlStmt]| v.iter_mut().for_each(not_atomic);
    match &mut s.kind {
        PlKind::Compound(c) => {
            c.atomic = false;
            all(&mut c.body);
            for h in &mut c.handlers {
                // Nothing to undo: an UNDO handler only leaves the compound.
                if h.kind == crate::ast::HandlerKind::Undo {
                    h.kind = crate::ast::HandlerKind::Exit;
                }
                not_atomic(&mut h.body);
            }
        }
        PlKind::If {
            branches,
            otherwise,
        } => {
            for (_, b) in branches {
                all(b);
            }
            if let Some(o) = otherwise {
                all(o);
            }
        }
        PlKind::Case {
            whens, otherwise, ..
        } => {
            for (_, b) in whens {
                all(b);
            }
            if let Some(o) = otherwise {
                all(o);
            }
        }
        PlKind::Loop(b)
        | PlKind::While(_, b)
        | PlKind::Repeat(b, _)
        | PlKind::For { body: b, .. } => all(b),
        _ => {}
    }
}

fn not_allowed(what: &str) -> SqlError {
    SqlError::Coded {
        code: -751,
        state: "0W000",
        what: format!("{what} is not allowed in this trigger"),
    }
}

/// Refuse what the body may not run: `COMMIT` and `ROLLBACK` in any
/// trigger, and changes of data in a `BEFORE` trigger.
pub(super) fn check(body: &PlStmt, before: bool) -> Result<()> {
    let mut all = Vec::new();
    each(body, &mut all);
    for s in all {
        let PlKind::Sql(q) = &s.kind else { continue };
        match &q.stmt {
            Statement::Commit { .. } | Statement::Rollback { .. } => {
                return Err(not_allowed("COMMIT or ROLLBACK"));
            }
            Statement::Insert(_)
            | Statement::Update(_)
            | Statement::Delete(_)
            | Statement::Merge(_)
                if before =>
            {
                return Err(not_allowed("a change of data in a BEFORE trigger"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reads of `corr.COLUMN` in `text`: the column names, upper case for
/// regular ones, in order, once per read. A `SET` target is no read.
pub(super) fn qualified_uses(text: &str, corrs: &[&str]) -> Vec<String> {
    let Ok(toks) = tokenize(text) else {
        return Vec::new();
    };
    let t: Vec<&Tok> = toks.iter().map(|t| &t.tok).collect();
    let name = |t: &Tok| match t {
        Tok::Word(s) | Tok::Delimited(s) => Some(s.clone()),
        _ => None,
    };
    let mut out = Vec::new();
    for i in 0..t.len().saturating_sub(2) {
        let (Some(q), Tok::Sym("."), Some(c)) = (name(t[i]), t[i + 1], name(t[i + 2])) else {
            continue;
        };
        if !corrs.contains(&q.as_str()) {
            continue;
        }
        let prev = i.checked_sub(1).map(|p| t[p]);
        let target = matches!(t.get(i + 3), Some(Tok::Sym("=")))
            && matches!(prev, Some(Tok::Word(w)) if w == "SET")
            || matches!(prev, Some(Tok::Sym(","))) && matches!(t.get(i + 3), Some(Tok::Sym("=")));
        if !target {
            out.push(c);
        }
    }
    out
}

/// The triggered statement as the catalog keeps it: its tokens one
/// blank apart, its lines as written, a blank at the end, and the
/// columns of a table it inserts into without listing them listed
/// (`cols_of` gives them from the schema, when named, and the table).
pub(super) fn statement_text(
    text: &str,
    cols_of: &mut dyn FnMut(Option<&str>, &str) -> Vec<String>,
) -> String {
    let Ok(toks) = tokenize(text) else {
        return text.to_string();
    };
    let toks: Vec<_> = toks
        .into_iter()
        .filter(|t| !matches!(t.tok, Tok::End))
        .collect();
    let word =
        |i: usize, w: &str| matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Word(x)) if x == w);
    let ident = |i: usize| match toks.get(i).map(|t| &t.tok) {
        Some(Tok::Word(s) | Tok::Delimited(s)) => Some(s.clone()),
        _ => None,
    };
    // (token index after which, line, columns)
    let mut adds: Vec<(usize, u32, Vec<String>)> = Vec::new();
    for i in 0..toks.len() {
        if !(word(i, "INSERT") && word(i + 1, "INTO")) {
            continue;
        }
        let Some(first) = ident(i + 2) else { continue };
        let qualified = matches!(toks.get(i + 3).map(|t| &t.tok), Some(Tok::Sym("." | "/")));
        let (schema, name, last) = match (qualified, ident(i + 4)) {
            (true, Some(n)) => (Some(first), n, i + 4),
            _ => (None, first, i + 2),
        };
        let listed = matches!(toks.get(last + 1).map(|t| &t.tok), Some(Tok::Sym("(")))
            && !(word(last + 2, "SELECT") || word(last + 2, "VALUES") || word(last + 2, "WITH"));
        if listed {
            continue;
        }
        let cols = cols_of(schema.as_deref(), &name);
        if !cols.is_empty() {
            adds.push((last, toks[last].line, cols));
        }
    }
    let mut seq: Vec<(Tok, u32)> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        seq.push((t.tok.clone(), t.line));
        if let Some((_, line, cols)) = adds.iter().find(|(at, _, _)| *at == i) {
            seq.push((Tok::Sym("("), *line));
            for (k, c) in cols.iter().enumerate() {
                if k > 0 {
                    seq.push((Tok::Sym(","), *line));
                }
                seq.push((Tok::Word(c.clone()), *line));
            }
            seq.push((Tok::Sym(")"), *line));
        }
    }
    let mut out = String::new();
    for (i, (t, line)) in seq.iter().enumerate() {
        crate::pl::rewrite::render(t, &mut out);
        let next_line = seq.get(i + 1).is_some_and(|(_, l)| l > line);
        out.push(if next_line { '\n' } else { ' ' });
    }
    out
}

/// A table named in `text`: its schema when qualified, its name, and
/// for `INSERT INTO` the columns listed (`Some(empty)` for none listed).
pub(super) struct Named {
    /// The schema.
    pub schema: Option<String>,
    /// The table.
    pub name: String,
    /// `INSERT INTO t (columns)`: the columns; `None` for other uses.
    pub insert: Option<Vec<String>>,
}

/// The tables `text` names after `FROM`, `JOIN`, `INTO`, `UPDATE` and
/// `TABLE`, `FROM` lists included.
pub(super) fn tables(text: &str) -> Vec<Named> {
    let Ok(toks) = tokenize(text) else {
        return Vec::new();
    };
    let t: Vec<&Tok> = toks.iter().map(|t| &t.tok).collect();
    let ident = |i: usize| match t.get(i) {
        Some(Tok::Word(s) | Tok::Delimited(s)) => Some(s.clone()),
        _ => None,
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let Tok::Word(w) = t[i] else {
            i += 1;
            continue;
        };
        let into = w == "INTO"
            && i > 0
            && matches!(t[i - 1], Tok::Word(p) if p == "INSERT" || p == "MERGE");
        let starts = matches!(w.as_str(), "FROM" | "JOIN" | "UPDATE") || into;
        i += 1;
        if !starts {
            continue;
        }
        while let Some(first) = ident(i) {
            if matches!(
                first.as_str(),
                "SELECT" | "VALUES" | "TABLE" | "LATERAL" | "FINAL" | "NEW" | "OLD"
            ) && matches!(t.get(i), Some(Tok::Word(_)))
            {
                break;
            }
            let (schema, name) = if matches!(t.get(i + 1), Some(Tok::Sym("." | "/"))) {
                let Some(second) = ident(i + 2) else { break };
                i += 3;
                (Some(first), second)
            } else {
                i += 1;
                (None, first)
            };
            let mut insert = None;
            if into {
                let mut cols = Vec::new();
                if matches!(t.get(i), Some(Tok::Sym("(")))
                    && ident(i + 1).is_some_and(|w| w != "SELECT" && w != "VALUES")
                {
                    i += 1;
                    while let Some(c) = ident(i) {
                        cols.push(c);
                        i += 1;
                        if !matches!(t.get(i), Some(Tok::Sym(","))) {
                            break;
                        }
                        i += 1;
                    }
                }
                insert = Some(cols);
            }
            out.push(Named {
                schema,
                name,
                insert,
            });
            // `FROM a, b`: the next table; a correlation name is skipped.
            if matches!(t.get(i), Some(Tok::Word(w)) if w == "AS") {
                i += 1;
            }
            if ident(i).is_some_and(|w| !is_clause(&w)) {
                i += 1;
            }
            if w == "FROM" && matches!(t.get(i), Some(Tok::Sym(","))) {
                i += 1;
                continue;
            }
            break;
        }
    }
    out
}

/// A word that ends a table reference.
fn is_clause(w: &str) -> bool {
    matches!(
        w,
        "WHERE"
            | "GROUP"
            | "HAVING"
            | "ORDER"
            | "FETCH"
            | "LIMIT"
            | "OFFSET"
            | "UNION"
            | "EXCEPT"
            | "INTERSECT"
            | "JOIN"
            | "INNER"
            | "LEFT"
            | "RIGHT"
            | "FULL"
            | "CROSS"
            | "ON"
            | "SET"
            | "VALUES"
            | "SELECT"
            | "WITH"
            | "FOR"
            | "USING"
            | "OVERRIDING"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_and_tables() {
        let text = "INSERT INTO QSTEST.STRGLOG VALUES ('ROW ' CONCAT N.NAME, (SELECT COUNT(*) FROM QSTEST.STRG1 S WHERE S.ID = N.ID))";
        assert_eq!(qualified_uses(text, &["N"]), ["NAME", "ID"]);
        assert_eq!(
            qualified_uses(
                "BEGIN SET N.A = UPPER(N.A); SET N.B = 1, N.C = N.B; END",
                &["N"]
            ),
            ["A", "B"]
        );
        let t = tables(text);
        assert_eq!(
            t.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["STRGLOG", "STRG1"]
        );
        assert_eq!(t[0].insert.as_deref(), Some(&[][..]));
        let t = tables("INSERT INTO LOG (TXT, N) SELECT A, B FROM X, Y AS Z JOIN W ON 1 = 1");
        assert_eq!(
            t.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["LOG", "X", "Y", "W"]
        );
        assert_eq!(
            t[0].insert.as_deref(),
            Some(&["TXT".to_string(), "N".to_string()][..])
        );
    }
}
