// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements built from the prompt displays (F4), and the reverse: a
//! typed statement split into the prompt's fields so F4 on a partial
//! statement shows what was typed. Each prompt's fields are named;
//! the driver reads them off the display by those names.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::IsqlError;

/// A prompt's field values by name.
pub type Values = BTreeMap<String, String>;

/// The statements F4 prompts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `SELECT`.
    Select,
    /// `INSERT`.
    Insert,
    /// `UPDATE`.
    Update,
    /// `DELETE`.
    Delete,
    /// `CREATE TABLE`.
    CreateTable,
    /// `CREATE VIEW`.
    CreateView,
    /// `CREATE INDEX`.
    CreateIndex,
}

/// The statements the prompt list offers, in its order.
pub const LIST: &[(Kind, &str)] = &[
    (Kind::Select, "SELECT"),
    (Kind::Insert, "INSERT"),
    (Kind::Update, "UPDATE"),
    (Kind::Delete, "DELETE"),
    (Kind::CreateTable, "CREATE TABLE"),
    (Kind::CreateView, "CREATE VIEW"),
    (Kind::CreateIndex, "CREATE INDEX"),
];

/// The statement a typed text starts, for F4; `None` for an empty
/// line (the prompt list shows) or a statement without a prompt.
#[must_use]
pub fn kind_of(text: &str) -> Option<Kind> {
    let words: Vec<String> = text
        .split_whitespace()
        .take(3)
        .map(str::to_ascii_uppercase)
        .collect();
    let w = |i: usize| words.get(i).map_or("", String::as_str);
    Some(match w(0) {
        "SELECT" => Kind::Select,
        "INSERT" => Kind::Insert,
        "UPDATE" => Kind::Update,
        "DELETE" => Kind::Delete,
        "CREATE" => match (w(1), w(2)) {
            ("TABLE", _) => Kind::CreateTable,
            ("VIEW", _) => Kind::CreateView,
            ("INDEX", _) | ("UNIQUE", "INDEX") => Kind::CreateIndex,
            _ => return None,
        },
        _ => return None,
    })
}

fn get(v: &Values, k: &str) -> String {
    v.get(k).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn yes(v: &Values, k: &str) -> bool {
    get(v, k).eq_ignore_ascii_case("Y")
}

fn clause(out: &mut String, word: &str, text: &str) {
    if !text.is_empty() {
        out.push(' ');
        out.push_str(word);
        out.push(' ');
        out.push_str(text);
    }
}

fn need(v: &Values, k: &str, what: &'static str) -> Result<String, IsqlError> {
    let s = get(v, k);
    if s.is_empty() {
        Err(IsqlError::Missing(what))
    } else {
        Ok(s)
    }
}

/// A qualified name as the naming wants it: `LIB/NAME` (`*SYS`) or
/// `LIB.NAME` (`*SQL`); just the name without a library.
#[must_use]
pub fn qualified(lib: &str, name: &str, sql_naming: bool) -> String {
    let lib = lib.trim();
    if lib.is_empty() || lib.starts_with('*') {
        name.trim().to_string()
    } else {
        format!("{lib}{}{}", if sql_naming { '.' } else { '/' }, name.trim())
    }
}

/// The statement from a prompt's values.
///
/// # Errors
/// A value the statement needs is missing.
#[allow(clippy::too_many_lines)] // one arm per statement
pub fn build(kind: Kind, v: &Values, sql_naming: bool) -> Result<String, IsqlError> {
    let mut s = String::new();
    match kind {
        Kind::Select => {
            let from = need(v, "FROM", "FROM tables")?;
            let cols = get(v, "SELECT");
            s.push_str("SELECT");
            if yes(v, "DISTINCT") {
                s.push_str(" DISTINCT");
            }
            s.push(' ');
            s.push_str(if cols.is_empty() { "*" } else { &cols });
            clause(&mut s, "FROM", &from);
            clause(&mut s, "WHERE", &get(v, "WHERE"));
            clause(&mut s, "GROUP BY", &get(v, "GROUPBY"));
            clause(&mut s, "HAVING", &get(v, "HAVING"));
            clause(&mut s, "ORDER BY", &get(v, "ORDERBY"));
            clause(&mut s, "FOR UPDATE OF", &get(v, "FORUPDATE"));
        }
        Kind::Insert => {
            let into = need(v, "INTO", "INTO table")?;
            s.push_str("INSERT INTO ");
            s.push_str(&qualified(&get(v, "LIB"), &into, sql_naming));
            let cols = get(v, "COLUMNS");
            if !cols.is_empty() {
                let _ = write!(s, " ({cols})");
            }
            let vals = need(v, "VALUES", "VALUES")?;
            if get(v, "METHOD") == "2" || is_query(&vals) {
                s.push(' ');
                s.push_str(&vals);
            } else {
                let inner = vals
                    .strip_prefix('(')
                    .and_then(|r| r.strip_suffix(')'))
                    .unwrap_or(&vals);
                let _ = write!(s, " VALUES({inner})");
            }
            isolation(&mut s, v);
        }
        Kind::Update => {
            let table = need(v, "TABLE", "Table")?;
            s.push_str("UPDATE ");
            s.push_str(&qualified(&get(v, "LIB"), &table, sql_naming));
            let corr = get(v, "CORR");
            if !corr.is_empty() {
                s.push(' ');
                s.push_str(&corr);
            }
            let set = need(v, "SET", "SET columns")?;
            clause(&mut s, "SET", &set);
            clause(&mut s, "WHERE", &get(v, "WHERE"));
            isolation(&mut s, v);
        }
        Kind::Delete => {
            let table = need(v, "TABLE", "Table")?;
            s.push_str("DELETE FROM ");
            s.push_str(&qualified(&get(v, "LIB"), &table, sql_naming));
            let corr = get(v, "CORR");
            if !corr.is_empty() {
                s.push(' ');
                s.push_str(&corr);
            }
            clause(&mut s, "WHERE", &get(v, "WHERE"));
            isolation(&mut s, v);
        }
        Kind::CreateTable => {
            let name = need(v, "TABLE", "Table")?;
            let typed = get(v, "COLUMNS");
            let cols = if typed.is_empty() {
                table_columns(v)?
            } else {
                typed
            };
            if cols.is_empty() {
                return Err(IsqlError::Missing("Columns"));
            }
            s.push_str("CREATE TABLE ");
            s.push_str(&qualified(&get(v, "LIB"), &name, sql_naming));
            let _ = write!(s, " ({cols})");
        }
        Kind::CreateView => {
            let name = need(v, "VIEW", "View")?;
            let select = need(v, "SUBSELECT", "Subselect")?;
            s.push_str("CREATE VIEW ");
            s.push_str(&qualified(&get(v, "LIB"), &name, sql_naming));
            let cols = get(v, "COLUMNS");
            if !cols.is_empty() {
                let _ = write!(s, " ({cols})");
            }
            s.push_str(" AS ");
            s.push_str(&select);
            match get(v, "CHECK").as_str() {
                "2" => s.push_str(" WITH CASCADED CHECK OPTION"),
                "3" => s.push_str(" WITH LOCAL CHECK OPTION"),
                _ => {}
            }
        }
        Kind::CreateIndex => {
            let name = need(v, "INDEX", "Index")?;
            let table = need(v, "TABLE", "Table")?;
            let cols = need(v, "COLUMNS", "Columns")?;
            s.push_str("CREATE ");
            s.push_str(match get(v, "ITYPE").as_str() {
                "2" => "UNIQUE ",
                "3" => "UNIQUE WHERE NOT NULL ",
                "4" => "ENCODED VECTOR ",
                _ if yes(v, "UNIQUE") => "UNIQUE ",
                _ => "",
            });
            s.push_str("INDEX ");
            s.push_str(&qualified(&get(v, "LIB"), &name, sql_naming));
            s.push_str(" ON ");
            s.push_str(&qualified(&get(v, "TABLELIB"), &table, sql_naming));
            let _ = write!(s, " ({cols})");
        }
    }
    Ok(s)
}

/// Whether a value starts a query (an INSERT's subselect).
fn is_query(v: &str) -> bool {
    let u = v.trim_start().to_ascii_uppercase();
    u.starts_with("SELECT ") || u.starts_with("WITH ") || u.starts_with("VALUES ")
}

/// `WITH` and the isolation level the prompt chose, when not the
/// current one.
fn isolation(s: &mut String, v: &Values) {
    let level = match get(v, "ISOL").as_str() {
        "2" => "NC",
        "3" => "UR",
        "4" => "CS",
        "5" => "RS",
        "6" => "RR",
        _ => return,
    };
    s.push_str(" WITH ");
    s.push_str(level);
}

/// CREATE TABLE's column definitions from the prompt's column table
/// (`FIELDn`, `FORn`, `TYPEn`, `LENn`, `SCALEn`, `NULLSn`).
///
/// # Errors
/// A column without a type.
pub fn table_columns(v: &Values) -> Result<String, IsqlError> {
    let mut defs = Vec::new();
    for n in 0.. {
        let key = format!("FIELD{n}");
        if !v.contains_key(&key) {
            break;
        }
        let name = get(v, &key);
        if name.is_empty() {
            continue;
        }
        let ty = get(v, &format!("TYPE{n}")).to_ascii_uppercase();
        if ty.is_empty() {
            return Err(IsqlError::Missing("Type"));
        }
        let len = get(v, &format!("LEN{n}"));
        let scale = get(v, &format!("SCALE{n}"));
        let mut d = name.clone();
        let for_name = get(v, &format!("FOR{n}"));
        if !for_name.is_empty() {
            let _ = write!(d, " FOR COLUMN {for_name}");
        }
        d.push(' ');
        d.push_str(&ty);
        if !len.is_empty() && !ty.contains('(') {
            if scale.is_empty() {
                let _ = write!(d, "({len})");
            } else {
                let _ = write!(d, "({len}, {scale})");
            }
        }
        match get(v, &format!("NULLS{n}")).as_str() {
            "2" => d.push_str(" NOT NULL"),
            "1" => {}
            _ => d.push_str(" NOT NULL WITH DEFAULT"),
        }
        defs.push(d);
    }
    Ok(defs.join(", "))
}

/// Where each clause keyword starts in `upper` (an upper-cased copy of
/// the statement), outside quotes and parentheses.
fn clauses(text: &str, words: &[&str]) -> Vec<(usize, usize, usize)> {
    let upper = text.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' {
            quote = !quote;
        } else if !quote && c == b'(' {
            depth += 1;
        } else if !quote && c == b')' {
            depth -= 1;
        } else if !quote && depth == 0 && (i == 0 || bytes[i - 1] == b' ') {
            for (wi, w) in words.iter().enumerate() {
                let end = i + w.len();
                if upper[i..].starts_with(w)
                    && (end == bytes.len() || bytes[end] == b' ' || bytes[end] == b'(')
                {
                    out.push((wi, i, end));
                    i = end;
                    break;
                }
            }
        }
        i += 1;
    }
    out
}

/// The text between each clause keyword and the next.
fn split(text: &str, words: &[&str]) -> Vec<(usize, String)> {
    let found = clauses(text, words);
    found
        .iter()
        .enumerate()
        .map(|(k, (wi, _, end))| {
            let stop = found.get(k + 1).map_or(text.len(), |n| n.1);
            (*wi, text[*end..stop].trim().to_string())
        })
        .collect()
}

/// A typed statement split into its prompt's fields.
#[must_use]
#[allow(clippy::too_many_lines)] // one arm per statement
pub fn fields_of(kind: Kind, text: &str) -> Values {
    let mut v = Values::new();
    let mut put = |k: &str, s: String| {
        if !s.is_empty() {
            v.insert(k.to_string(), s);
        }
    };
    match kind {
        Kind::Select => {
            let words = [
                "SELECT",
                "FROM",
                "WHERE",
                "GROUP BY",
                "HAVING",
                "ORDER BY",
                "FOR UPDATE OF",
            ];
            let keys = [
                "SELECT",
                "FROM",
                "WHERE",
                "GROUPBY",
                "HAVING",
                "ORDERBY",
                "FORUPDATE",
            ];
            for (wi, s) in split(text, &words) {
                if wi == 0 {
                    let (distinct, rest) = match s.get(..9) {
                        Some(h) if h.eq_ignore_ascii_case("DISTINCT ") => {
                            (true, s[9..].to_string())
                        }
                        _ => (false, s.clone()),
                    };
                    if distinct {
                        put("DISTINCT", "Y".into());
                    }
                    if rest.trim() != "*" {
                        put("SELECT", rest);
                    }
                } else {
                    put(keys[wi], s);
                }
            }
        }
        Kind::Update => {
            for (wi, s) in split(text, &["UPDATE", "SET", "WHERE"]) {
                match wi {
                    0 => {
                        let mut it = s.split_whitespace();
                        put("TABLE", it.next().unwrap_or("").to_string());
                        put("CORR", it.next().unwrap_or("").to_string());
                    }
                    1 => put("SET", s),
                    _ => put("WHERE", s),
                }
            }
        }
        Kind::Delete => {
            for (wi, s) in split(text, &["FROM", "WHERE"]) {
                if wi == 0 {
                    let mut it = s.split_whitespace();
                    put("TABLE", it.next().unwrap_or("").to_string());
                    put("CORR", it.next().unwrap_or("").to_string());
                } else {
                    put("WHERE", s);
                }
            }
        }
        Kind::Insert => {
            for (wi, s) in split(text, &["INTO", "VALUES"]) {
                if wi == 0 {
                    let (table, rest) = s.split_once(' ').unwrap_or((s.as_str(), ""));
                    put("INTO", table.to_string());
                    let rest = rest.trim();
                    if let Some(c) = rest.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                        put("COLUMNS", c.to_string());
                    } else if !rest.is_empty() {
                        put("METHOD", "2".into());
                        put("VALUES", rest.to_string());
                    }
                } else {
                    let s = s.trim();
                    let inner = s
                        .strip_prefix('(')
                        .and_then(|r| r.strip_suffix(')'))
                        .unwrap_or(s);
                    put("VALUES", inner.to_string());
                }
            }
        }
        Kind::CreateTable | Kind::CreateView | Kind::CreateIndex => {
            let word = match kind {
                Kind::CreateTable => "TABLE",
                Kind::CreateView => "VIEW",
                _ => "INDEX",
            };
            if let Some((_, _, end)) = clauses(text, &[word]).first() {
                let name = text[*end..].split_whitespace().next().unwrap_or("");
                let (lib, obj) = name
                    .split_once(['/', '.'])
                    .map_or(("", name), |(l, n)| (l, n));
                let key = match kind {
                    Kind::CreateTable => "TABLE",
                    Kind::CreateView => "VIEW",
                    _ => "INDEX",
                };
                put(key, obj.trim_end_matches('(').to_string());
                put("LIB", lib.to_string());
            }
            if kind == Kind::CreateIndex && text.to_ascii_uppercase().contains("UNIQUE ") {
                put("UNIQUE", "Y".into());
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(p: &[(&str, &str)]) -> Values {
        p.iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn kinds() {
        assert_eq!(kind_of("select"), Some(Kind::Select));
        assert_eq!(kind_of("create unique index x"), Some(Kind::CreateIndex));
        assert_eq!(kind_of("create table"), Some(Kind::CreateTable));
        assert_eq!(kind_of(""), None);
        assert_eq!(kind_of("drop table x"), None);
    }

    #[test]
    fn select() {
        let v = vals(&[
            ("FROM", "qsys2.systables"),
            ("SELECT", "table_name"),
            ("WHERE", "table_schema = 'QSYS2'"),
            ("ORDERBY", "1"),
            ("DISTINCT", "Y"),
        ]);
        assert_eq!(
            build(Kind::Select, &v, false).unwrap(),
            "SELECT DISTINCT table_name FROM qsys2.systables WHERE table_schema = 'QSYS2' ORDER BY 1"
        );
        assert!(build(Kind::Select, &Values::new(), false).is_err());
        let back = fields_of(
            Kind::Select,
            "select distinct a, b from t where x = 'from y' order by a",
        );
        assert_eq!(back.get("FROM").map(String::as_str), Some("t"));
        assert_eq!(back.get("WHERE").map(String::as_str), Some("x = 'from y'"));
        assert_eq!(back.get("SELECT").map(String::as_str), Some("a, b"));
        assert_eq!(back.get("DISTINCT").map(String::as_str), Some("Y"));
    }

    #[test]
    fn dml() {
        let v = vals(&[("INTO", "T"), ("COLUMNS", "A, B"), ("VALUES", "1, 'X'")]);
        assert_eq!(
            build(Kind::Insert, &v, false).unwrap(),
            "INSERT INTO T (A, B) VALUES(1, 'X')"
        );
        let back = fields_of(Kind::Insert, "insert into t (a, b) values(1, 'x')");
        assert_eq!(back.get("COLUMNS").map(String::as_str), Some("a, b"));
        assert_eq!(back.get("VALUES").map(String::as_str), Some("1, 'x'"));
        let v = vals(&[("TABLE", "T"), ("SET", "A = 1"), ("WHERE", "B = 2")]);
        assert_eq!(
            build(Kind::Update, &v, false).unwrap(),
            "UPDATE T SET A = 1 WHERE B = 2"
        );
        let v = vals(&[("TABLE", "T"), ("CORR", "X"), ("WHERE", "X.B = 2")]);
        assert_eq!(
            build(Kind::Delete, &v, false).unwrap(),
            "DELETE FROM T X WHERE X.B = 2"
        );
        let back = fields_of(Kind::Delete, "delete from t x where x.b = 2");
        assert_eq!(back.get("CORR").map(String::as_str), Some("x"));
    }

    #[test]
    fn prompt_tables() {
        let v = vals(&[
            ("TABLE", "ITEMS"),
            ("LIB", "QGPL"),
            ("FIELD0", "ID"),
            ("TYPE0", "INTEGER"),
            ("NULLS0", "2"),
            ("FIELD1", "NAME"),
            ("TYPE1", "char"),
            ("LEN1", "10"),
            ("NULLS1", "3"),
            ("FIELD2", ""),
            ("NULLS2", "3"),
        ]);
        assert_eq!(
            build(Kind::CreateTable, &v, false).unwrap(),
            "CREATE TABLE QGPL/ITEMS (ID INTEGER NOT NULL, NAME CHAR(10) NOT NULL WITH DEFAULT)"
        );
        let v = vals(&[
            ("INDEX", "X1"),
            ("TABLE", "T"),
            ("TABLELIB", "QGPL"),
            ("COLUMNS", "A"),
            ("ITYPE", "2"),
        ]);
        assert_eq!(
            build(Kind::CreateIndex, &v, true).unwrap(),
            "CREATE UNIQUE INDEX X1 ON QGPL.T (A)"
        );
        let v = vals(&[("INTO", "T"), ("VALUES", "select * from u"), ("ISOL", "2")]);
        assert_eq!(
            build(Kind::Insert, &v, false).unwrap(),
            "INSERT INTO T select * from u WITH NC"
        );
    }

    #[test]
    fn ddl() {
        let v = vals(&[
            ("TABLE", "ITEMS"),
            ("LIB", "QGPL"),
            ("COLUMNS", "ID INTEGER, NAME CHAR(10)"),
        ]);
        assert_eq!(
            build(Kind::CreateTable, &v, false).unwrap(),
            "CREATE TABLE QGPL/ITEMS (ID INTEGER, NAME CHAR(10))"
        );
        assert_eq!(
            build(Kind::CreateTable, &v, true).unwrap(),
            "CREATE TABLE QGPL.ITEMS (ID INTEGER, NAME CHAR(10))"
        );
        let v = vals(&[("VIEW", "V"), ("SUBSELECT", "SELECT * FROM T")]);
        assert_eq!(
            build(Kind::CreateView, &v, false).unwrap(),
            "CREATE VIEW V AS SELECT * FROM T"
        );
        let v = vals(&[
            ("INDEX", "X1"),
            ("TABLE", "T"),
            ("COLUMNS", "A DESC"),
            ("UNIQUE", "Y"),
        ]);
        assert_eq!(
            build(Kind::CreateIndex, &v, false).unwrap(),
            "CREATE UNIQUE INDEX X1 ON T (A DESC)"
        );
        let back = fields_of(Kind::CreateTable, "create table qgpl/items (id int)");
        assert_eq!(back.get("TABLE").map(String::as_str), Some("items"));
        assert_eq!(back.get("LIB").map(String::as_str), Some("qgpl"));
    }
}
