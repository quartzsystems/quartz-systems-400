// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample database's statement runner: `SELECT cols FROM t [WHERE]
//! [ORDER BY]` (rows in arrival order), `UPDATE t SET c = v [WHERE]`,
//! `DELETE FROM t [WHERE]`. A condition is `AND`ed comparisons of a
//! column with a literal, `IN (...)`, `LIKE` and `IS [NOT] NULL`.

use std::cmp::Ordering;

use qsweb::msg::Kind;

use super::{Table, msg};
use crate::env::db::{SqlOptions, SqlOutcome};
use crate::fixture::PROFILES;

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Num(String),
    Sym(String),
}

#[allow(clippy::many_single_char_names)]
fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let b: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_whitespace() || c == ';' {
            i += 1;
        } else if c == '\'' {
            let mut v = String::new();
            i += 1;
            loop {
                match b.get(i) {
                    None => return Err("'".into()),
                    Some('\'') if b.get(i + 1) == Some(&'\'') => {
                        v.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(&ch) => {
                        v.push(ch);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Str(v));
        } else if c.is_ascii_digit() || (c == '-' && b.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let st = i;
            i += 1;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(b[st..i].iter().collect()));
        } else if c.is_alphanumeric() || matches!(c, '_' | '$' | '#' | '@') {
            let st = i;
            while i < b.len() && (b[i].is_alphanumeric() || matches!(b[i], '_' | '$' | '#' | '@' | '.' | '/')) {
                i += 1;
            }
            out.push(Tok::Word(b[st..i].iter().collect::<String>().to_ascii_uppercase()));
        } else {
            let two: String = b[i..(i + 2).min(b.len())].iter().collect();
            if matches!(two.as_str(), "<>" | "<=" | ">=" | "!=") {
                out.push(Tok::Sym(two));
                i += 2;
            } else if "(),=<>*".contains(c) {
                out.push(Tok::Sym(c.to_string()));
                i += 1;
            } else {
                return Err(c.to_string());
            }
        }
    }
    Ok(out)
}

/// One comparison of a condition.
enum Test {
    Cmp(usize, String, String),
    In(usize, Vec<String>),
    Like(usize, String),
    Null(usize, bool),
}

struct P<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn word(&mut self, w: &str) -> bool {
        if self.peek() == Some(&Tok::Word(w.into())) {
            self.at += 1;
            return true;
        }
        false
    }

    fn sym(&mut self, s: &str) -> bool {
        if self.peek() == Some(&Tok::Sym(s.into())) {
            self.at += 1;
            return true;
        }
        false
    }

    fn name(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                let w = w.clone();
                self.at += 1;
                Some(w)
            }
            _ => None,
        }
    }

    fn literal(&mut self) -> Option<String> {
        match self.peek() {
            Some(Tok::Str(s) | Tok::Num(s)) => {
                let s = s.clone();
                self.at += 1;
                Some(s)
            }
            _ => None,
        }
    }

    fn near(&self) -> String {
        match self.peek() {
            Some(Tok::Word(w) | Tok::Num(w) | Tok::Sym(w)) => w.clone(),
            Some(Tok::Str(s)) => format!("'{s}'"),
            None => "end of statement".into(),
        }
    }
}

type Fail = (&'static str, Vec<String>);

fn syntax(p: &P<'_>) -> Fail {
    ("SQL0104", vec!["1".into(), (p.at + 1).to_string(), p.near()])
}

fn column(t: &Table, name: &str) -> Result<usize, Fail> {
    t.columns.iter().position(|c| c.name == name).ok_or(("SQL0206", vec![name.to_string()]))
}

/// The table a name stands for: `S.T`, `S/T` or `T` in the schema.
fn table_of(name: &str, opts: &SqlOptions) -> (String, String) {
    match name.split_once(['.', '/']) {
        Some((s, t)) => (s.to_string(), t.to_string()),
        None => (opts.schema.clone(), name.to_string()),
    }
}

fn find<'a>(tables: &'a mut [Table], name: &str, opts: &SqlOptions) -> Result<&'a mut Table, Fail> {
    let (s, t) = table_of(name, opts);
    if s != "HLINV" {
        return Err(("SQL0204", vec![t, s]));
    }
    tables.iter_mut().find(|x| x.obj.name == t).ok_or(("SQL0204", vec![t, s]))
}

fn condition(p: &mut P<'_>, t: &Table) -> Result<Vec<Test>, Fail> {
    let mut tests = Vec::new();
    if !p.word("WHERE") {
        return Ok(tests);
    }
    loop {
        let c = p.name().ok_or_else(|| syntax(p))?;
        let c = column(t, &c)?;
        if p.word("IN") {
            if !p.sym("(") {
                return Err(syntax(p));
            }
            let mut vs = Vec::new();
            loop {
                vs.push(p.literal().ok_or_else(|| syntax(p))?);
                if p.sym(")") {
                    break;
                }
                if !p.sym(",") {
                    return Err(syntax(p));
                }
            }
            tests.push(Test::In(c, vs));
        } else if p.word("LIKE") {
            tests.push(Test::Like(c, p.literal().ok_or_else(|| syntax(p))?));
        } else if p.word("IS") {
            let not = p.word("NOT");
            if !p.word("NULL") {
                return Err(syntax(p));
            }
            tests.push(Test::Null(c, !not));
        } else {
            let op = match p.peek() {
                Some(Tok::Sym(s)) if matches!(s.as_str(), "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=") => s.clone(),
                _ => return Err(syntax(p)),
            };
            p.at += 1;
            tests.push(Test::Cmp(c, op, p.literal().ok_or_else(|| syntax(p))?));
        }
        if !p.word("AND") {
            break;
        }
    }
    Ok(tests)
}

fn compare(a: &str, b: &str) -> Ordering {
    match (a.trim().parse::<f64>(), b.trim().parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.trim_end().cmp(b.trim_end()),
    }
}

fn like(v: &str, pat: &str) -> bool {
    match pat.split_once('%') {
        None => v.trim_end() == pat,
        Some((pre, rest)) => {
            v.starts_with(pre) && (rest.is_empty() || v.len() >= pre.len() && like_tail(&v[pre.len()..], rest))
        }
    }
}

fn like_tail(v: &str, pat: &str) -> bool {
    (0..=v.len()).filter(|i| v.is_char_boundary(*i)).any(|i| like(&v[i..], pat))
}

fn holds(row: &[Option<String>], tests: &[Test]) -> bool {
    tests.iter().all(|t| match t {
        Test::Null(c, null) => row[*c].is_none() == *null,
        Test::In(c, vs) => row[*c].as_ref().is_some_and(|v| vs.iter().any(|x| compare(v, x).is_eq())),
        Test::Like(c, pat) => row[*c].as_ref().is_some_and(|v| like(v.trim_end(), pat)),
        Test::Cmp(c, op, x) => row[*c].as_ref().is_some_and(|v| {
            let o = compare(v, x);
            match op.as_str() {
                "=" => o.is_eq(),
                "<>" | "!=" => o.is_ne(),
                "<" => o.is_lt(),
                ">" => o.is_gt(),
                "<=" => o.is_le(),
                _ => o.is_ge(),
            }
        }),
    })
}

fn may_change(user: &str) -> bool {
    PROFILES
        .iter()
        .find(|p| p.name == user)
        .is_some_and(|p| p.special.contains(&"*ALLOBJ"))
}

fn select(tables: &mut [Table], p: &mut P<'_>, opts: &SqlOptions, max: Option<usize>) -> Result<SqlOutcome, Fail> {
    let mut names = Vec::new();
    if !p.sym("*") {
        loop {
            let n = p.name().ok_or_else(|| syntax(p))?;
            if p.peek() == Some(&Tok::Sym("(".into())) {
                return Err(("SQL0084", vec![format!("{n}()")]));
            }
            names.push(n);
            if !p.sym(",") {
                break;
            }
        }
    }
    if !p.word("FROM") {
        return Err(syntax(p));
    }
    let tname = p.name().ok_or_else(|| syntax(p))?;
    let t = find(tables, &tname, opts)?;
    let idx: Vec<usize> = if names.is_empty() {
        (0..t.columns.len()).collect()
    } else {
        names.iter().map(|n| column(t, n)).collect::<Result<_, _>>()?
    };
    let tests = condition(p, t)?;
    if p.word("ORDER") {
        if !p.word("BY") {
            return Err(syntax(p));
        }
        loop {
            let n = p.name().ok_or_else(|| syntax(p))?;
            column(t, &n)?;
            let _ = p.word("ASC") || p.word("DESC");
            if !p.sym(",") {
                break;
            }
        }
    }
    if p.peek().is_some() {
        return Err(syntax(p));
    }
    let all: Vec<Vec<Option<String>>> = t
        .rows
        .iter()
        .filter(|r| holds(r, &tests))
        .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
        .collect();
    let cap = max.unwrap_or(usize::MAX);
    Ok(SqlOutcome {
        query: true,
        columns: idx.iter().map(|i| t.columns[*i].name.clone()).collect(),
        truncated: all.len() > cap,
        rows: all.into_iter().take(cap).collect(),
        ..SqlOutcome::default()
    })
}

fn change(tables: &mut [Table], p: &mut P<'_>, user: &str, opts: &SqlOptions, update: bool) -> Result<SqlOutcome, Fail> {
    if !update && !p.word("FROM") {
        return Err(syntax(p));
    }
    let tname = p.name().ok_or_else(|| syntax(p))?;
    let (schema, name) = table_of(&tname, opts);
    let t = find(tables, &tname, opts)?;
    if !may_change(user) {
        return Err(("SQL0551", vec![name, schema, "*FILE".into()]));
    }
    let mut sets = Vec::new();
    if update {
        if !p.word("SET") {
            return Err(syntax(p));
        }
        loop {
            let c = p.name().ok_or_else(|| syntax(p))?;
            let c = column(t, &c)?;
            if !p.sym("=") {
                return Err(syntax(p));
            }
            let v = if p.word("NULL") { None } else { Some(p.literal().ok_or_else(|| syntax(p))?) };
            sets.push((c, v));
            if !p.sym(",") {
                break;
            }
        }
    }
    let tests = condition(p, t)?;
    if p.peek().is_some() {
        return Err(syntax(p));
    }
    let before = t.rows.len();
    let mut n = 0usize;
    if update {
        for r in t.rows.iter_mut().filter(|r| holds(r, &tests)) {
            for (c, v) in &sets {
                r[*c].clone_from(v);
            }
            n += 1;
        }
    } else {
        t.rows.retain(|r| !holds(r, &tests));
        n = before - t.rows.len();
        t.obj.rows = t.rows.len() as u64;
    }
    let m = if n == 0 {
        msg(Kind::Info, "SQL0100", &[if update { "UPDATE" } else { "DELETE" }])
    } else {
        let id = if update { "SQL7957" } else { "SQL7955" };
        msg(Kind::Done, id, &[&name, &schema, &n.to_string()])
    };
    Ok(SqlOutcome {
        messages: vec![m],
        ..SqlOutcome::default()
    })
}

/// Run `statement` over HLINV's tables as `user`.
pub fn run(tables: &mut [Table], user: &str, statement: &str, opts: &SqlOptions, max: Option<usize>) -> SqlOutcome {
    let toks = match lex(statement) {
        Ok(t) => t,
        Err(near) => {
            return failed(("SQL0104", vec!["1".into(), "1".into(), near]));
        }
    };
    let mut p = P { toks: &toks, at: 0 };
    let r = if p.word("SELECT") {
        select(tables, &mut p, opts, max)
    } else if p.word("UPDATE") {
        change(tables, &mut p, user, opts, true)
    } else if p.word("DELETE") {
        change(tables, &mut p, user, opts, false)
    } else if p.peek().is_none() {
        Err(syntax(&p))
    } else {
        Err(("SQL0084", vec![p.near()]))
    };
    r.unwrap_or_else(failed)
}

fn failed((id, data): Fail) -> SqlOutcome {
    let data: Vec<&str> = data.iter().map(String::as_str).collect();
    SqlOutcome {
        messages: vec![msg(Kind::Error, id, &data)],
        ..SqlOutcome::default()
    }
}

/// The access plan of a query: a scan of its table with the selection.
pub fn explain(tables: &[Table], statement: &str, opts: &SqlOptions) -> Result<Vec<String>, qsweb::msg::Message> {
    let mut copy = tables.to_vec();
    let out = run(&mut copy, "QSECOFR", statement, opts, Some(0));
    if out.failed() {
        return Err(out.messages[0].clone());
    }
    let toks = lex(statement).unwrap_or_default();
    let from = toks.iter().position(|t| *t == Tok::Word("FROM".into()));
    let target = from
        .and_then(|i| toks.get(i + 1))
        .or_else(|| toks.get(1))
        .map(|t| match t {
            Tok::Word(w) => w.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    let (s, t) = table_of(&target, opts);
    let rows = tables.iter().find(|x| x.obj.name == t).map_or(0, |x| x.rows.len());
    let selection = toks.iter().any(|t| *t == Tok::Word("WHERE".into()));
    Ok(vec![
        format!("1  Table scan        {s}.{t}  ({rows} rows estimated)"),
        format!("2  Selection         {}", if selection { "WHERE clause, row by row" } else { "none" }),
        "3  Final result      rows in arrival order".into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SqlOptions {
        SqlOptions {
            schema: "HLINV".into(),
            ..SqlOptions::default()
        }
    }

    #[test]
    fn statements() {
        let mut t = super::super::tables();
        let o = run(&mut t, "QSECOFR", "SELECT asset_tag FROM assets WHERE status IN ('ACTIVE', 'SPARE')", &opts(), None);
        assert_eq!(o.rows.len(), 12);
        let o = run(&mut t, "QSECOFR", "select * from hlinv.assets where location like 'QS-BR%' and status = 'SPARE'", &opts(), None);
        assert_eq!((o.rows.len(), o.columns.len()), (1, 12));
        let o = run(&mut t, "WEBUSER", "UPDATE assets SET status = 'SPARE'", &opts(), None);
        assert_eq!(o.messages[0].id, "SQL0551");
        let o = run(&mut t, "QSECOFR", "UPDATE assets SET status = 'SPARE' WHERE asset_tag = 'QS000101'", &opts(), None);
        assert_eq!(o.messages[0].id, "SQL7957");
        let o = run(&mut t, "QSECOFR", "SELECT nope FROM assets", &opts(), None);
        assert_eq!(o.messages[0].id, "SQL0206");
        let o = run(&mut t, "QSECOFR", "SELECT * FROM qsys2.x", &opts(), None);
        assert_eq!(o.messages[0].id, "SQL0204");
        let o = run(&mut t, "QSECOFR", "SELECT * FROM assets WHERE", &opts(), None);
        assert_eq!(o.messages[0].id, "SQL0104");
    }
}
