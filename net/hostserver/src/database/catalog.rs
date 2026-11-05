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

/// A column of a catalog result: its name and shape (`v` VARCHAR, `g`
/// VARGRAPHIC, `c` CHAR, all with a length; `s` SMALLINT, `i` INTEGER,
/// `b` BIGINT), and whether it may be null.
type Spec = (&'static str, char, u32, bool);

fn col(&(name, kind, n, null): &Spec) -> ColType {
    let t = |ty: u16, len: i64| ColType::from_describe(i64::from(ty) + i64::from(null), len, name);
    match kind {
        'g' => {
            let mut c = t(sqltype::VARGRAPHIC, i64::from(n));
            c.ccsid = 1200;
            c
        }
        'c' => t(sqltype::CHAR, i64::from(n)),
        's' => t(sqltype::SMALLINT, 2),
        'i' => t(sqltype::INTEGER, 4),
        'b' => t(sqltype::BIGINT, 8),
        _ => t(sqltype::VARCHAR, i64::from(n)),
    }
}

/// `'text'` as an SQL literal.
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A filter on `column`: equal to a name, `LIKE` a pattern; none for no
/// value or `%`.
fn filter(column: &str, v: Option<&str>) -> Option<String> {
    let v = v.map(str::trim).filter(|v| !v.is_empty() && *v != "%")?;
    Some(if v.contains(['%', '_']) { format!("{column} LIKE {}", lit(v)) } else { format!("{column} = {}", lit(v)) })
}

/// `SELECT cols FROM SYSIBM.view WHERE ... ORDER BY ...`.
fn select(view: &str, specs: &[Spec], wheres: &[Option<String>], order: &str) -> String {
    let cols: Vec<&str> = specs.iter().map(|s| s.0).collect();
    let mut q = format!("SELECT {} FROM SYSIBM.{view}", cols.join(", "));
    let w: Vec<&str> = wheres.iter().flatten().map(String::as_str).collect();
    if !w.is_empty() {
        q.push_str(" WHERE ");
        q.push_str(&w.join(" AND "));
    }
    if !order.is_empty() {
        q.push_str(" ORDER BY ");
        q.push_str(order);
    }
    q
}

/// The value of `name=value` in a call's options (`DATATYPE='JDBC';...`).
fn option<'a>(options: Option<&'a str>, name: &str) -> Option<&'a str> {
    options?
        .split(';')
        .find_map(|o| o.split_once('=').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().trim_matches('\'')))
}

const TABLES: &[Spec] = &[
    ("TABLE_CAT", 'v', 128, false),
    ("TABLE_SCHEM", 'v', 128, false),
    ("TABLE_NAME", 'v', 128, false),
    ("TABLE_TYPE", 'v', 24, false),
    ("REMARKS", 'g', 2000, true),
    ("TYPE_CAT", 'v', 128, true),
    ("TYPE_SCHEM", 'v', 128, true),
    ("TYPE_NAME", 'v', 128, true),
    ("SELF_REFERENCING_COL_NAME", 'v', 128, true),
    ("REF_GENERATION", 'v', 128, true),
];

const COLUMNS: &[Spec] = &[
    ("TABLE_CAT", 'v', 128, false),
    ("TABLE_SCHEM", 'v', 128, false),
    ("TABLE_NAME", 'v', 128, false),
    ("COLUMN_NAME", 'v', 128, false),
    ("JDBC_DATA_TYPE", 'i', 0, false),
    ("TYPE_NAME", 'v', 261, false),
    ("COLUMN_SIZE", 'i', 0, false),
    ("BUFFER_LENGTH", 'i', 0, false),
    ("DECIMAL_DIGITS", 's', 0, true),
    ("NUM_PREC_RADIX", 's', 0, true),
    ("NULLABLE", 's', 0, false),
    ("REMARKS", 'g', 2000, true),
    ("COLUMN_DEF", 'g', 2000, true),
    ("SQL_DATA_TYPE", 's', 0, false),
    ("SQL_DATETIME_SUB", 's', 0, true),
    ("CHAR_OCTET_LENGTH", 'i', 0, true),
    ("ORDINAL_POSITION", 'i', 0, false),
    ("IS_NULLABLE", 'v', 3, false),
    ("SCOPE_CATALOG", 'v', 128, true),
    ("SCOPE_SCHEMA", 'v', 128, true),
    ("SCOPE_TABLE", 'v', 128, true),
    ("SOURCE_DATA_TYPE", 's', 0, true),
];

const KEYS: &[Spec] = &[
    ("TABLE_CAT", 'v', 128, false),
    ("TABLE_SCHEM", 'v', 128, false),
    ("TABLE_NAME", 'v', 128, false),
    ("COLUMN_NAME", 'v', 128, false),
    ("KEY_SEQ", 's', 0, false),
    ("PK_NAME", 'v', 128, false),
];

const FOREIGN: &[Spec] = &[
    ("PKTABLE_CAT", 'v', 128, false),
    ("PKTABLE_SCHEM", 'v', 128, false),
    ("PKTABLE_NAME", 'v', 128, false),
    ("PKCOLUMN_NAME", 'v', 128, false),
    ("FKTABLE_CAT", 'v', 128, false),
    ("FKTABLE_SCHEM", 'v', 128, false),
    ("FKTABLE_NAME", 'v', 128, false),
    ("FKCOLUMN_NAME", 'v', 128, false),
    ("KEY_SEQ", 's', 0, false),
    ("UPDATE_RULE", 's', 0, false),
    ("DELETE_RULE", 's', 0, false),
    ("FK_NAME", 'v', 128, false),
    ("PK_NAME", 'v', 128, true),
    ("DEFERRABILITY", 's', 0, false),
];

const STATISTICS: &[Spec] = &[
    ("TABLE_CAT", 'v', 128, false),
    ("TABLE_SCHEM", 'v', 128, false),
    ("TABLE_NAME", 'v', 128, false),
    ("NON_UNIQUE", 's', 0, true),
    ("INDEX_QUALIFIER", 'v', 128, true),
    ("INDEX_NAME", 'v', 128, true),
    ("TYPE", 's', 0, false),
    ("ORDINAL_POSITION", 's', 0, true),
    ("COLUMN_NAME", 'g', 2000, true),
    ("ASC_OR_DESC", 'c', 1, true),
    ("CARDINALITY", 'b', 0, true),
    ("PAGES", 'b', 0, true),
    ("FILTER_CONDITION", 'g', 2000, true),
];

const SPECIAL: &[Spec] = &[
    ("SCOPE", 's', 0, false),
    ("COLUMN_NAME", 'v', 128, false),
    ("JDBC_DATA_TYPE", 'i', 0, false),
    ("TYPE_NAME", 'v', 261, false),
    ("COLUMN_SIZE", 'i', 0, false),
    ("BUFFER_LENGTH", 'i', 0, false),
    ("DECIMAL_DIGITS", 's', 0, true),
    ("PSEUDO_COLUMN", 's', 0, false),
];

const PRIVILEGES: &[Spec] = &[
    ("TABLE_CAT", 'v', 128, false),
    ("TABLE_SCHEM", 'v', 128, false),
    ("TABLE_NAME", 'v', 128, false),
    ("GRANTOR", 'v', 128, true),
    ("GRANTEE", 'v', 128, false),
    ("PRIVILEGE", 'v', 10, false),
    ("IS_GRANTABLE", 'v', 3, false),
];

const TYPEINFO: &[Spec] = &[
    ("TYPE_NAME", 'v', 128, false),
    ("JDBC_DATA_TYPE", 's', 0, false),
    ("COLUMN_SIZE", 'i', 0, true),
    ("LITERAL_PREFIX", 'v', 128, true),
    ("LITERAL_SUFFIX", 'v', 128, true),
    ("CREATE_PARAMS", 'v', 128, true),
    ("NULLABLE", 's', 0, true),
    ("CASE_SENSITIVE", 's', 0, true),
    ("SEARCHABLE", 's', 0, true),
    ("UNSIGNED_ATTRIBUTE", 's', 0, true),
    ("FIXED_PREC_SCALE", 's', 0, true),
    ("AUTO_UNIQUE_VALUE", 's', 0, true),
    ("LOCAL_TYPE_NAME", 'v', 128, true),
    ("MINIMUM_SCALE", 's', 0, true),
    ("MAXIMUM_SCALE", 's', 0, true),
    ("SQL_DATA_TYPE", 'i', 0, true),
    ("SQL_DATETIME_SUB", 'i', 0, true),
    ("NUM_PREC_RADIX", 'i', 0, true),
    ("INTERVAL_PRECISION", 's', 0, true),
];

/// The JDBC names of columns the `SYSIBM` views name otherwise.
fn jdbc_name(view_name: &str) -> &str {
    match view_name {
        "JDBC_DATA_TYPE" => "DATA_TYPE",
        "SCOPE_CATALOG" => "SCOPE_CATLOG",
        "AUTO_UNIQUE_VALUE" => "AUTO_INCREMENT",
        n => n,
    }
}

/// The result of a query over a view: its columns as JDBC names them,
/// the catalog columns holding the server's name.
fn run(specs: &[Spec], q: &str, rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let typeinfo = std::ptr::eq(specs, TYPEINFO);
    let mut cols: Vec<ColType> = specs.iter().map(col).collect();
    for (c, sp) in cols.iter_mut().zip(specs) {
        c.name = if typeinfo && sp.0 == "COLUMN_SIZE" { "PRECISION".into() } else { jdbc_name(sp.0).to_string() };
    }
    let cat: Vec<bool> = specs.iter().map(|sp| sp.0.ends_with("_CAT")).collect();
    let rows = query(q)
        .into_iter()
        .map(|mut r| {
            for (v, is_cat) in r.iter_mut().zip(&cat) {
                if *is_cat && !v.is_null() {
                    *v = s(rdb);
                }
            }
            r
        })
        .collect();
    Rows { cols, rows }
}

/// Answer a catalog call from the `SYSIBM` metadata views
/// (`sql::exec::catalog::sysibm`), in the result shapes JDBC reads
/// (`tools/conformance/jtopen/expected/oracle/metadata.out`). `query`
/// runs a `SELECT` and gives its rows.
pub fn answer(c: &Call, args: &[Option<String>], rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let a = |i: usize| args.get(i).cloned().flatten();
    match c.name.as_str() {
        "SQLTABLES" => tables(&a, rdb, query),
        "SQLCOLUMNS" => {
            let w = [filter("TABLE_SCHEM", a(1).as_deref()), filter("TABLE_NAME", a(2).as_deref()), filter("COLUMN_NAME", a(3).as_deref())];
            let mut r = run(COLUMNS, &select("SQLCOLUMNS", COLUMNS, &w, "TABLE_SCHEM, TABLE_NAME, ORDINAL_POSITION"), rdb, query);
            r.cols.push(vc("IS_AUTOINCREMENT", 3, false));
            for row in &mut r.rows {
                row.push(s("NO"));
            }
            r
        }
        "SQLPRIMARYKEYS" => {
            let w = [filter("TABLE_SCHEM", a(1).as_deref()), filter("TABLE_NAME", a(2).as_deref())];
            run(KEYS, &select("SQLPRIMARYKEYS", KEYS, &w, "TABLE_SCHEM, TABLE_NAME, KEY_SEQ"), rdb, query)
        }
        "SQLFOREIGNKEYS" => {
            let w = [
                filter("PKTABLE_SCHEM", a(1).as_deref()),
                filter("PKTABLE_NAME", a(2).as_deref()),
                filter("FKTABLE_SCHEM", a(4).as_deref()),
                filter("FKTABLE_NAME", a(5).as_deref()),
            ];
            let order = if option(a(6).as_deref(), "IMPORTEDKEY") == Some("1") {
                "PKTABLE_SCHEM, PKTABLE_NAME, KEY_SEQ"
            } else {
                "FKTABLE_SCHEM, FKTABLE_NAME, KEY_SEQ"
            };
            run(FOREIGN, &select("SQLFOREIGNKEYS", FOREIGN, &w, order), rdb, query)
        }
        "SQLSTATISTICS" => {
            let mut w = vec![filter("TABLE_SCHEM", a(1).as_deref()), filter("TABLE_NAME", a(2).as_deref())];
            if a(3).as_deref().map(str::trim) == Some("0") {
                w.push(Some("(NON_UNIQUE = 0 OR TYPE = 0)".into()));
            }
            run(STATISTICS, &select("SQLSTATISTICS", STATISTICS, &w, "NON_UNIQUE, TYPE, INDEX_QUALIFIER, INDEX_NAME, ORDINAL_POSITION"), rdb, query)
        }
        "SQLSPECIALCOLUMNS" => {
            // Columns whose scope lasts at least as long as asked for.
            let scope = a(4).as_deref().and_then(|x| x.trim().parse::<i64>().ok()).unwrap_or(0);
            let w = [filter("TABLE_SCHEM", a(2).as_deref()), filter("TABLE_NAME", a(3).as_deref()), Some(format!("SCOPE >= {scope}"))];
            run(SPECIAL, &select("SQLSPECIALCOLUMNS", SPECIAL, &w, "SCOPE"), rdb, query)
        }
        "SQLTABLEPRIVILEGES" => {
            let w = [filter("TABLE_SCHEM", a(1).as_deref()), filter("TABLE_NAME", a(2).as_deref())];
            run(PRIVILEGES, &select("SQLTABLEPRIVILEGES", PRIVILEGES, &w, "TABLE_SCHEM, TABLE_NAME, PRIVILEGE"), rdb, query)
        }
        "SQLCOLPRIVILEGES" => col_privileges(a(1).as_deref(), a(2).as_deref(), a(3).as_deref(), rdb, query),
        "SQLGETTYPEINFO" => run(TYPEINFO, &select("SQLTYPEINFO", TYPEINFO, &[], "JDBC_DATA_TYPE, TYPE_NAME"), rdb, query),
        _ => empty(&c.name),
    }
}

/// `SQLTABLES`, and through its options the schemas, the catalog and
/// the table types.
fn tables(a: &dyn Fn(usize) -> Option<String>, rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let o = a(4);
    if option(o.as_deref(), "GETSCHEMAS").is_some() {
        let specs: &[Spec] = &[("TABLE_SCHEM", 'v', 128, false), ("TABLE_CAT", 'v', 128, true)];
        let mut r = run(specs, &select("SQLSCHEMAS", specs, &[filter("TABLE_SCHEM", a(1).as_deref())], "TABLE_SCHEM"), rdb, query);
        r.cols[1].name = "TABLE_CATALOG".into();
        return r;
    }
    if option(o.as_deref(), "GETCATALOGS").is_some() {
        return Rows { cols: vec![vc("TABLE_CAT", 128, false)], rows: vec![vec![s(rdb)]] };
    }
    if option(o.as_deref(), "GETTABLETYPES").is_some() {
        let specs: &[Spec] = &[("TABLE_TYPE", 'v', 128, false)];
        return run(specs, &select("SQLTABLETYPES", specs, &[], "TABLE_TYPE"), rdb, query);
    }
    let types: Vec<String> = a(3)
        .map(|t| t.split(',').map(|x| x.trim().trim_matches('\'').trim().to_ascii_uppercase()).filter(|x| !x.is_empty()).map(|x| lit(&x)).collect())
        .unwrap_or_default();
    let tf = (!types.is_empty()).then(|| format!("TABLE_TYPE IN ({})", types.join(", ")));
    let w = [filter("TABLE_SCHEM", a(1).as_deref()), filter("TABLE_NAME", a(2).as_deref()), tf];
    run(TABLES, &select("SQLTABLES", TABLES, &w, "TABLE_TYPE, TABLE_SCHEM, TABLE_NAME"), rdb, query)
}

/// `SQLCOLPRIVILEGES`: the store keeps no column privileges, so each
/// column has those of its table's privileges that apply to columns, as
/// the primary lists them.
fn col_privileges(schema: Option<&str>, table: Option<&str>, column: Option<&str>, rdb: &str, query: &mut dyn FnMut(&str) -> Vec<Vec<Value>>) -> Rows {
    let cols = vec![
        vc("TABLE_CAT", 128, false),
        vc("TABLE_SCHEM", 128, false),
        vc("TABLE_NAME", 128, false),
        vc("COLUMN_NAME", 128, false),
        vc("GRANTOR", 128, true),
        vc("GRANTEE", 128, false),
        vc("PRIVILEGE", 10, false),
        vc("IS_GRANTABLE", 3, false),
    ];
    let w = [filter("TABLE_SCHEM", schema), filter("TABLE_NAME", table)];
    let privs = query(&select("SQLTABLEPRIVILEGES", PRIVILEGES, &w, "PRIVILEGE"));
    let specs: &[Spec] = &[("TABLE_SCHEM", 'v', 128, false), ("TABLE_NAME", 'v', 128, false), ("COLUMN_NAME", 'v', 128, false)];
    let wc = [filter("TABLE_SCHEM", schema), filter("TABLE_NAME", table), filter("COLUMN_NAME", column)];
    let names = query(&select("SQLCOLUMNS", specs, &wc, "COLUMN_NAME"));
    let mut rows = Vec::new();
    for n in &names {
        for p in &privs {
            let kind = p.get(5).map(|v| v.as_text().trim().to_string()).unwrap_or_default();
            let same = |i: usize, j: usize| n.get(i).map(Value::as_text) == p.get(j).map(Value::as_text);
            if !matches!(kind.as_str(), "INSERT" | "REFERENCES" | "SELECT" | "UPDATE") || !same(0, 1) || !same(1, 2) {
                continue;
            }
            rows.push(vec![s(rdb), n[0].clone(), n[1].clone(), n[2].clone(), p[3].clone(), p[4].clone(), s(&kind), p[6].clone()]);
        }
    }
    Rows { cols, rows }
}

/// The procedures with nothing to list: their columns, no rows.
/// BLOCKED(sql): routines and types are listed once SQL procedures,
/// functions and distinct types are kept (`SQLPROCEDURES`, ...).
fn empty(name: &str) -> Rows {
    let names: &[&str] = match name {
        "SQLPROCEDURES" => &[
            "PROCEDURE_CAT",
            "PROCEDURE_SCHEM",
            "PROCEDURE_NAME",
            "NUM_INPUT_PARAMS",
            "NUM_OUTPUT_PARAMS",
            "NUM_RESULT_SETS",
            "REMARKS",
            "PROCEDURE_TYPE",
            "SPECIFIC_NAME",
        ],
        "SQLFUNCTIONS" => &["FUNCTION_CAT", "FUNCTION_SCHEM", "FUNCTION_NAME", "REMARKS", "FUNCTION_TYPE", "SPECIFIC_NAME"],
        "SQLUDTS" => &["TYPE_CAT", "TYPE_SCHEM", "TYPE_NAME", "CLASS_NAME", "DATA_TYPE", "REMARKS", "BASE_TYPE"],
        _ => &["TABLE_CAT", "TABLE_SCHEM", "TABLE_NAME"],
    };
    Rows { cols: names.iter().map(|n| vc(n, 128, true)).collect(), rows: Vec::new() }
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
