// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The catalog procedures the JDBC driver calls for metadata
//! (`SYSIBM.SQLTABLES`, `SQLCOLUMNS`, ...): answered by the server
//! from the `QSYS2` catalog views, as result sets of the JDBC shapes.

use sql::value::Value;

use super::types::{ColType, sqltype};

/// The catalog procedures answered here.
pub const PROCEDURES: &[&str] = &[
    "SQLTABLES",
    "SQLCOLUMNS",
    "SQLPRIMARYKEYS",
    "SQLFOREIGNKEYS",
    "SQLSTATISTICS",
    "SQLPROCEDURES",
    "SQLPROCEDURECOLS",
    "SQLFUNCTIONS",
    "SQLFUNCTIONCOLS",
    "SQLSPECIALCOLUMNS",
    "SQLTABLEPRIVILEGES",
    "SQLCOLPRIVILEGES",
    "SQLGETTYPEINFO",
    "SQLUDTS",
    "SQLSUPERTABLES",
    "SQLSUPERTYPES",
    "SQLATTRIBUTES",
    "SQLPSEUDOCOLUMNS",
];

/// A `CALL` of a catalog procedure: its name and argument expressions
/// (`?` for markers, quoted strings, `NULL`, numbers).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The procedure.
    pub name: String,
    /// The arguments as written.
    pub args: Vec<String>,
}

/// Recognise `CALL SYSIBM.name(args)`.
pub fn parse(text: &str) -> Option<Call> {
    let t = text.trim();
    let upper = t.to_ascii_uppercase();
    let rest = upper.strip_prefix("CALL")?.trim_start();
    let rest_orig = &t[t.len() - rest.len()..];
    let (name, args) = match rest.find('(') {
        Some(i) => (&rest[..i], &rest_orig[i + 1..rest_orig.rfind(')')?]),
        None => (rest, ""),
    };
    let name = name.trim();
    let proc_ = name.strip_prefix("SYSIBM.").or_else(|| name.strip_prefix("SYSIBM/"))?;
    if !PROCEDURES.contains(&proc_) {
        return None;
    }
    Some(Call {
        name: proc_.to_string(),
        args: split_args(args),
    })
}

fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = false;
    for ch in s.chars() {
        match ch {
            '\'' => {
                quote = !quote;
                cur.push(ch);
            }
            ',' if !quote => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

impl Call {
    /// How many parameter markers the call has.
    pub fn markers(&self) -> usize {
        self.args.iter().filter(|a| *a == "?").count()
    }

    /// The arguments' values, markers taken in order from `markers`.
    pub fn values(&self, markers: &[Value]) -> Vec<Option<String>> {
        let mut m = markers.iter();
        self.args
            .iter()
            .map(|a| {
                if a == "?" {
                    m.next().and_then(|v| (!v.is_null()).then(|| v.as_text().trim_end().to_string()))
                } else if a.eq_ignore_ascii_case("NULL") {
                    None
                } else {
                    Some(a.trim_matches('\'').replace("''", "'"))
                }
            })
            .collect()
    }
}

/// Whether `s` matches an SQL `LIKE` pattern (`%`, `_`); no pattern
/// matches everything.
pub fn like(pattern: Option<&str>, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match (p.first(), s.first()) {
            (None, None) => true,
            (Some('%'), _) => go(&p[1..], s) || (!s.is_empty() && go(p, &s[1..])),
            (Some('_'), Some(_)) => go(&p[1..], &s[1..]),
            (Some('\\'), _) if p.len() > 1 => s.first() == Some(&p[1]) && go(&p[2..], &s[1..]),
            (Some(a), Some(b)) => a == b && go(&p[1..], &s[1..]),
            _ => false,
        }
    }
    let Some(p) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    let p: Vec<char> = p.chars().collect();
    let s: Vec<char> = s.chars().collect();
    go(&p, &s)
}

/// A result set the server holds itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    /// The columns.
    pub cols: Vec<ColType>,
    /// The rows.
    pub rows: Vec<Vec<Value>>,
}

fn vc(name: &str, n: u32, null: bool) -> ColType {
    ColType::varchar(name, n, null)
}

fn vg(name: &str, n: u32) -> ColType {
    let mut c = ColType::from_describe(i64::from(sqltype::VARGRAPHIC) + 1, i64::from(n), name);
    c.ccsid = 1200;
    c
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// JDBC's `java.sql.Types` code and the type name of a catalog type.
pub fn jdbc_type(t: &str) -> (i64, &'static str) {
    match t.trim() {
        "CHAR" | "CHARACTER" => (1, "CHAR"),
        "VARCHAR" => (12, "VARCHAR"),
        "GRAPHIC" => (-15, "NCHAR"),
        "VARGRAPHIC" | "VARG" => (-9, "NVARCHAR"),
        "INTEGER" | "INT" => (4, "INTEGER"),
        "SMALLINT" => (5, "SMALLINT"),
        "BIGINT" => (-5, "BIGINT"),
        "DECIMAL" => (3, "DECIMAL"),
        "NUMERIC" => (2, "NUMERIC"),
        "FLOAT" | "DOUBLE" => (8, "DOUBLE"),
        "REAL" => (7, "REAL"),
        "DATE" => (91, "DATE"),
        "TIME" => (92, "TIME"),
        "TIMESTMP" | "TIMESTAMP" => (93, "TIMESTAMP"),
        "BINARY" => (-2, "BINARY"),
        "VARBIN" | "VARBINARY" => (-3, "VARBINARY"),
        _ => (1111, "OTHER"),
    }
}

/// Answer a catalog call. `query` runs a `SELECT` and gives its rows.
pub fn answer(c: &Call, args: &[Option<String>], rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let arg = |i: usize| args.get(i).cloned().flatten();
    match c.name.as_str() {
        "SQLTABLES" => tables(arg(1).as_deref(), arg(2).as_deref(), arg(3).as_deref(), rdb, query),
        "SQLCOLUMNS" => columns(arg(1).as_deref(), arg(2).as_deref(), arg(3).as_deref(), rdb, query),
        _ => empty(&c.name),
    }
}

fn tables(schema: Option<&str>, table: Option<&str>, types: Option<&str>, rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let cols = vec![
        vc("TABLE_CAT", 128, false),
        vc("TABLE_SCHEM", 128, false),
        vc("TABLE_NAME", 128, false),
        vc("TABLE_TYPE", 24, false),
        vg("REMARKS", 2000),
        vc("TYPE_CAT", 128, true),
        vc("TYPE_SCHEM", 128, true),
        vc("TYPE_NAME", 128, true),
        vc("SELF_REFERENCING_COL_NAME", 128, true),
        vc("REF_GENERATION", 128, true),
    ];
    let wanted: Vec<String> = types
        .map(|t| {
            t.split(',')
                .map(|x| x.trim().trim_matches('\'').trim().to_ascii_uppercase())
                .filter(|x| !x.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let raw = query("SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE, TABLE_TEXT FROM QSYS2.SYSTABLES");
    let mut rows: Vec<Vec<Value>> = raw
        .into_iter()
        .filter_map(|r| {
            let sch = r.first()?.as_text().trim_end().to_string();
            let name = r.get(1)?.as_text().trim_end().to_string();
            let kind = match r.get(2)?.as_text().trim() {
                "V" => "VIEW",
                "L" => "INDEX",
                "A" => "ALIAS",
                "M" => "MATERIALIZED QUERY TABLE",
                _ => "TABLE",
            };
            if !like(schema, &sch) || !like(table, &name) || (!wanted.is_empty() && !wanted.iter().any(|w| w == kind)) {
                return None;
            }
            let text = r.get(3).map(|v| v.as_text().trim_end().to_string()).filter(|t| !t.is_empty());
            Some(vec![
                s(rdb),
                s(&sch),
                s(&name),
                s(kind),
                text.map_or(Value::Null, Value::Str),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
            ])
        })
        .collect();
    rows.sort_by(|a, b| {
        let k = |r: &Vec<Value>| (r[3].as_text(), r[1].as_text(), r[2].as_text());
        k(a).cmp(&k(b))
    });
    Rows { cols, rows }
}

fn num(v: Option<&Value>) -> Option<i64> {
    v.and_then(|x| x.as_i64().ok())
}

fn columns(schema: Option<&str>, table: Option<&str>, column: Option<&str>, rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let int = |n: &str, null: bool| ColType::integer(n, null);
    let small = |n: &str, null: bool| ColType::smallint(n, null);
    let cols = vec![
        vc("TABLE_CAT", 128, false),
        vc("TABLE_SCHEM", 128, false),
        vc("TABLE_NAME", 128, false),
        vc("COLUMN_NAME", 128, false),
        int("DATA_TYPE", false),
        vc("TYPE_NAME", 128, false),
        int("COLUMN_SIZE", false),
        int("BUFFER_LENGTH", false),
        small("DECIMAL_DIGITS", true),
        small("NUM_PREC_RADIX", true),
        small("NULLABLE", false),
        vg("REMARKS", 2000),
        vg("COLUMN_DEF", 2000),
        small("SQL_DATA_TYPE", false),
        small("SQL_DATETIME_SUB", true),
        int("CHAR_OCTET_LENGTH", true),
        int("ORDINAL_POSITION", false),
        vc("IS_NULLABLE", 3, false),
        vc("SCOPE_CATLOG", 128, true),
        vc("SCOPE_SCHEMA", 128, true),
        vc("SCOPE_TABLE", 128, true),
        small("SOURCE_DATA_TYPE", true),
        vc("IS_AUTOINCREMENT", 3, false),
    ];
    let raw = query(
        "SELECT TABLE_SCHEMA, TABLE_NAME, COLUMN_NAME, ORDINAL_POSITION, DATA_TYPE, LENGTH, NUMERIC_SCALE, NUMERIC_PRECISION, IS_NULLABLE, COLUMN_TEXT, COLUMN_DEFAULT FROM QSYS2.SYSCOLUMNS",
    );
    let mut rows: Vec<Vec<Value>> = raw
        .into_iter()
        .filter_map(|r| {
            let t = |i: usize| r.get(i).map(|v| v.as_text().trim_end().to_string()).unwrap_or_default();
            let (sch, tab, col) = (t(0), t(1), t(2));
            if !like(schema, &sch) || !like(table, &tab) || !like(column, &col) {
                return None;
            }
            let (code, name) = jdbc_type(&t(4));
            let len = num(r.get(5)).unwrap_or(0);
            let scale = num(r.get(6));
            let prec = num(r.get(7));
            let numeric = matches!(code, 2 | 3 | 4 | 5 | -5 | 7 | 8);
            let size = if numeric { prec.unwrap_or(len) } else { len };
            let nullable = t(8) == "Y";
            let octet = (!numeric && !matches!(code, 91..=93)).then_some(len);
            let text = t(9);
            Some(vec![
                s(rdb),
                s(&sch),
                s(&tab),
                s(&col),
                Value::int(code),
                s(name),
                Value::int(size),
                Value::int(len),
                scale.filter(|_| numeric).map_or(Value::Null, Value::int),
                if numeric { Value::int(10) } else { Value::Null },
                Value::int(i64::from(nullable)),
                if text.is_empty() { Value::Null } else { Value::Str(text) },
                r.get(10).cloned().unwrap_or(Value::Null),
                Value::int(code),
                Value::Null,
                octet.map_or(Value::Null, Value::int),
                Value::int(num(r.get(3)).unwrap_or(0)),
                s(if nullable { "YES" } else { "NO" }),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                s("NO"),
            ])
        })
        .collect();
    rows.sort_by(|a, b| {
        let k = |r: &Vec<Value>| (r[1].as_text(), r[2].as_text(), r[16].as_i64().unwrap_or(0));
        k(a).cmp(&k(b))
    });
    Rows { cols, rows }
}

fn empty(name: &str) -> Rows {
    let cols = match name {
        "SQLPRIMARYKEYS" => vec![
            vc("TABLE_CAT", 128, false),
            vc("TABLE_SCHEM", 128, false),
            vc("TABLE_NAME", 128, false),
            vc("COLUMN_NAME", 128, false),
            ColType::smallint("KEY_SEQ", false),
            vc("PK_NAME", 128, true),
        ],
        _ => vec![
            vc("TABLE_CAT", 128, true),
            vc("TABLE_SCHEM", 128, true),
            vc("TABLE_NAME", 128, true),
        ],
    };
    Rows {
        cols,
        rows: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_calls() {
        let c = parse("CALL SYSIBM.SQLTABLES(?,?,?,?,?)").unwrap();
        assert_eq!(c.markers(), 5);
        assert!(parse("CALL QSYS.QCMDEXC(?, ?)").is_none());
        let v = c.values(&[Value::Null, s("QSYS2"), s("SYSTABLE%"), Value::Null, s("x")]);
        assert_eq!(v[1].as_deref(), Some("QSYS2"));
        assert!(like(Some("SYSTABLE%"), "SYSTABLESTAT"));
        assert!(!like(Some("SYSTABLE%"), "SYSCOLUMNS"));
    }
}
