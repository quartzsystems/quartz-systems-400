// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::db::Db` as the user, through the interactive SQL
//! session's requests (`isql_start`, `isql_run`, `isql_end`): the
//! schemas and their objects are queries over the `QSYS2` catalog views
//! (`SYSSCHEMAS`, `SYSTABLES`, `SYSTABLESTAT`, `SYSCOLUMNS`, `SYSCST`,
//! `SYSKEYCST`, `SYSINDEXES`, `SYSKEYS`, `SYSVIEWDEP`), Run SQL one
//! statement with the user's schema, naming and isolation. The catalog
//! has no `SYSTRIGGERS` or `SYSROUTINES`: no triggers or procedures.

use std::collections::HashMap;
use std::time::Instant;

use navigator::env::db::{
    Column, Constraint, Db, DbObject, Index, ObjKind, Schema, SqlOptions, SqlOutcome, TableDesc,
};
use qsweb::msg::{Kind, Message, substitute};
use stg::Client;
use stg::proto::IsqlReply;

use super::Link;
use crate::work::web::bridge::Main;

/// Rows a catalog query keeps.
const CATALOG_ROWS: u32 = 100_000;

/// The database as the user sees it.
pub struct LiveDb {
    /// The job's thread.
    pub link: Link,
}

/// `'text'` as an SQL literal.
fn lit(s: &str) -> String {
    format!("'{}'", s.trim().to_ascii_uppercase().replace('\'', "''"))
}

/// Seconds since 1970 of a UTC civil time.
pub fn utc_secs(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> u64 {
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + s).unwrap_or(0)
}

/// `2026-10-29 13:58:02.000000` (or with `-` and `.`), UTC, as seconds.
pub fn stamp_secs(text: &str) -> u64 {
    let parts: Vec<i64> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect();
    match parts.as_slice() {
        [y, mo, d, h, mi, s, ..] => utc_secs(*y, *mo, *d, *h, *mi, *s),
        [y, mo, d] => utc_secs(*y, *mo, *d, 0, 0, 0),
        _ => 0,
    }
}

/// A message's text from `QSQLMSG` (`QCPFMSG` for `CPF`), `data`
/// substituted; the ID alone when there is no such message.
pub fn text_of(m: &Main, id: &str, data: &[&str]) -> String {
    let file = if id.starts_with("SQL") { "QSQLMSG" } else { "QCPFMSG" };
    let ask = id.to_string();
    m.ask(|l| l.describe(ask, file.into(), "QSYS".into()))
        .map_or_else(|| id.to_string(), |d| substitute(&d.text, data))
}

/// One statement run in the job's SQL session.
fn run(m: &Main, text: &str, naming: &str, max_rows: u32) -> IsqlReply {
    let (t, n) = (text.to_string(), naming.to_string());
    m.ask(|l| l.isql_run(t, n, false, max_rows))
}

/// A catalog query's rows, each column as trimmed text.
fn catalog(m: &Main, query: &str) -> Vec<Vec<String>> {
    let _ = m.ask(|l| l.isql_start("*SQL".into(), "*NONE".into()));
    let r = run(m, query, "*SQL", CATALOG_ROWS);
    r.rows
        .into_iter()
        .map(|row| row.into_iter().map(|v| v.unwrap_or_default().trim().to_string()).collect())
        .collect()
}

fn cell(r: &[String], i: usize) -> String {
    r.get(i).cloned().unwrap_or_default()
}

fn num(r: &[String], i: usize) -> u64 {
    r.get(i).and_then(|v| v.split('.').next()?.trim().parse().ok()).unwrap_or(0)
}

/// `CYYMMDDHHMMSS`, local time `offset_min` from UTC, as seconds.
pub fn cyy_secs(t: &str, offset_min: i32) -> u64 {
    let n = |a: usize, b: usize| t.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    if t.len() < 13 {
        return 0;
    }
    let local = utc_secs(1900 + n(0, 1) * 100 + n(1, 3), n(3, 5), n(5, 7), n(7, 9), n(9, 11), n(11, 13));
    u64::try_from(i64::try_from(local).unwrap_or(0) - i64::from(offset_min) * 60).unwrap_or(0)
}

/// When `QSYS/NAME` of type `ty` was created.
fn created(m: &Main, lib: &str, name: &str, ty: &str) -> u64 {
    let (l, n, t) = (lib.to_string(), name.to_string(), ty.to_string());
    let offset = m.ask(Client::now).offset_min;
    m.ask(|c| c.object(l, n, t))
        .map_or(0, |o| cyy_secs(&o.created, offset))
}

/// Rows, deleted rows and bytes of each table in `schema`.
fn stats(m: &Main, schema: &str) -> HashMap<String, (u64, u64, u64)> {
    let q = format!(
        "SELECT TABLE_NAME, NUMBER_ROWS, NUMBER_DELETED_ROWS, DATA_SIZE FROM QSYS2.SYSTABLESTAT WHERE TABLE_SCHEMA = {}",
        lit(schema)
    );
    catalog(m, &q)
        .into_iter()
        .map(|r| (cell(&r, 0), (num(&r, 1), num(&r, 2), num(&r, 3))))
        .collect()
}

fn schemas(m: &Main) -> Vec<Schema> {
    let q = "SELECT SCHEMA_NAME, SCHEMA_TEXT, SCHEMA_OWNER FROM QSYS2.SYSSCHEMAS ORDER BY SCHEMA_NAME".to_string();
    catalog(m, &q)
        .into_iter()
        .map(|r| Schema {
            created: created(m, "QSYS", &cell(&r, 0), "*LIB"),
            name: cell(&r, 0),
            text: cell(&r, 1),
            owner: cell(&r, 2),
            ..Schema::default()
        })
        .collect()
}

/// The tables (`T`, `P`) or views (`V`) of `schema`.
fn files(m: &Main, schema: &str, types: &str) -> Vec<DbObject> {
    let q = format!(
        "SELECT TABLE_NAME, TABLE_TEXT, LAST_ALTERED_TIMESTAMP FROM QSYS2.SYSTABLES \
         WHERE TABLE_SCHEMA = {} AND TABLE_TYPE IN ({types}) AND FILE_TYPE = 'D' ORDER BY TABLE_NAME",
        lit(schema)
    );
    catalog(m, &q)
        .into_iter()
        .map(|r| DbObject {
            name: cell(&r, 0),
            text: cell(&r, 1),
            changed: stamp_secs(&cell(&r, 2)),
            ..DbObject::default()
        })
        .collect()
}

fn objects(m: &Main, schema: &str, kind: ObjKind) -> Vec<DbObject> {
    match kind {
        ObjKind::Tables => {
            let st = stats(m, schema);
            let mut out = files(m, schema, "'T', 'P'");
            for o in &mut out {
                if let Some((rows, _, size)) = st.get(&o.name) {
                    (o.rows, o.size) = (*rows, *size);
                }
            }
            out
        }
        ObjKind::Views => {
            let q = format!(
                "SELECT VIEW_NAME, OBJECT_NAME FROM QSYS2.SYSVIEWDEP WHERE VIEW_SCHEMA = {}",
                lit(schema)
            );
            let over: HashMap<String, String> = catalog(m, &q).into_iter().map(|r| (cell(&r, 0), cell(&r, 1))).collect();
            let mut out = files(m, schema, "'V'");
            for o in &mut out {
                o.over = over.get(&o.name).cloned().unwrap_or_default();
            }
            out
        }
        ObjKind::Indexes => {
            let q = format!(
                "SELECT INDEX_NAME, INDEX_TEXT, TABLE_NAME FROM QSYS2.SYSINDEXES WHERE INDEX_SCHEMA = {} ORDER BY INDEX_NAME",
                lit(schema)
            );
            catalog(m, &q)
                .into_iter()
                .map(|r| DbObject {
                    name: cell(&r, 0),
                    text: cell(&r, 1),
                    over: cell(&r, 2),
                    ..DbObject::default()
                })
                .collect()
        }
        ObjKind::Procedures => Vec::new(),
    }
}

fn columns(m: &Main, schema: &str, table: &str) -> Vec<Column> {
    let q = format!(
        "SELECT COLUMN_NAME, SYSTEM_COLUMN_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE, IS_NULLABLE, COLUMN_DEFAULT, \
         COLUMN_TEXT FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {} ORDER BY ORDINAL_POSITION",
        lit(schema),
        lit(table)
    );
    let decimal = |t: &str| matches!(t, "DECIMAL" | "NUMERIC");
    catalog(m, &q)
        .into_iter()
        .map(|r| {
            let typ = cell(&r, 2);
            Column {
                name: cell(&r, 0),
                system_name: cell(&r, 1),
                length: u32::try_from(num(&r, 3)).unwrap_or(0),
                scale: (decimal(&typ) && !cell(&r, 4).is_empty()).then(|| u32::try_from(num(&r, 4)).unwrap_or(0)),
                nullable: cell(&r, 5) == "Y",
                default: cell(&r, 6),
                text: cell(&r, 7),
                typ,
            }
        })
        .collect()
}

/// `SYSKEYCST` or `SYSKEYS` columns of one constraint or index, in order.
fn keys(m: &Main, view: &str, kind: &str, schema: &str, name: &str) -> String {
    let q = format!(
        "SELECT COLUMN_NAME FROM QSYS2.{view} WHERE {kind}_SCHEMA = {} AND {kind}_NAME = {} ORDER BY ORDINAL_POSITION",
        lit(schema),
        lit(name)
    );
    catalog(m, &q).iter().map(|r| cell(r, 0)).collect::<Vec<_>>().join(", ")
}

fn describe(m: &Main, schema: &str, name: &str) -> Option<TableDesc> {
    let q = format!(
        "SELECT TABLE_NAME, TABLE_TEXT, LAST_ALTERED_TIMESTAMP, TABLE_OWNER, SYSTEM_TABLE_NAME, SYSTEM_TABLE_SCHEMA \
         FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {}",
        lit(schema),
        lit(name)
    );
    let row = catalog(m, &q).into_iter().next()?;
    let (rows, deleted, size) = stats(m, schema).get(&cell(&row, 0)).copied().unwrap_or_default();
    let q = format!(
        "SELECT CONSTRAINT_NAME, CONSTRAINT_SCHEMA, CONSTRAINT_TYPE FROM QSYS2.SYSCST \
         WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {}",
        lit(schema),
        lit(name)
    );
    let constraints = catalog(m, &q)
        .into_iter()
        .map(|r| Constraint {
            columns: keys(m, "SYSKEYCST", "CONSTRAINT", &cell(&r, 1), &cell(&r, 0)),
            name: cell(&r, 0),
            kind: cell(&r, 2),
            references: String::new(),
        })
        .collect();
    let q = format!(
        "SELECT INDEX_NAME, INDEX_SCHEMA, IS_UNIQUE, INDEX_TEXT FROM QSYS2.SYSINDEXES \
         WHERE TABLE_SCHEMA = {} AND TABLE_NAME = {} ORDER BY INDEX_NAME",
        lit(schema),
        lit(name)
    );
    let indexes = catalog(m, &q)
        .into_iter()
        .map(|r| Index {
            columns: keys(m, "SYSKEYS", "INDEX", &cell(&r, 1), &cell(&r, 0)),
            name: cell(&r, 0),
            schema: cell(&r, 1),
            unique: cell(&r, 2) != "D",
            text: cell(&r, 3),
        })
        .collect();
    let system_name = cell(&row, 4);
    Some(TableDesc {
        schema: schema.to_string(),
        created: created(m, &cell(&row, 5), &system_name, "*FILE"),
        owner: cell(&row, 3),
        system_name,
        deleted,
        columns: columns(m, schema, name),
        constraints,
        indexes,
        table: DbObject {
            name: cell(&row, 0),
            text: cell(&row, 1),
            changed: stamp_secs(&cell(&row, 2)),
            rows,
            size,
            ..DbObject::default()
        },
        ..TableDesc::default()
    })
}

/// What a statement that ran says (as Enter SQL Statements words it).
fn completion(m: &Main, r: &IsqlReply) -> Option<Message> {
    let count = r.count.to_string();
    let (tbl, lib) = (r.table.0.as_str(), r.table.1.as_str());
    let (kind, id, data): (Kind, &str, Vec<&str>) = match r.statement.as_str() {
        "SELECT" | "EXPLAIN" => return None,
        "UPDATE" | "DELETE" | "MERGE" if r.count == 0 => (Kind::Info, "SQL0100", vec![r.statement.as_str()]),
        "INSERT" => (Kind::Done, "SQL7956", vec![tbl, lib, &count]),
        "UPDATE" => (Kind::Done, "SQL7957", vec![tbl, lib, &count]),
        "DELETE" => (Kind::Done, "SQL7955", vec![tbl, lib, &count]),
        "COMMIT" => (Kind::Done, "SQL7960", vec![]),
        "ROLLBACK" => (Kind::Done, "SQL7961", vec![]),
        other => (Kind::Done, "SQL7977", vec![other]),
    };
    Some(Message::new(kind, id, &text_of(m, id, &data)).with_data(&data))
}

/// An `IsqlReply` as the page's outcome.
fn outcome(m: &Main, r: IsqlReply, elapsed_ms: u64) -> SqlOutcome {
    let mut o = SqlOutcome {
        elapsed_ms,
        ..SqlOutcome::default()
    };
    if r.sqlcode < 0 || (r.sqlcode > 0 && r.sqlcode != 100 && !r.msgid.is_empty()) {
        let id = if r.msgid.is_empty() { "SQL0901" } else { r.msgid.as_str() };
        let text = if r.text.is_empty() { text_of(m, id, &[]) } else { r.text.clone() };
        let kind = if r.sqlcode < 0 { Kind::Error } else { Kind::Info };
        o.messages.push(Message::new(kind, id, &text));
        if r.sqlcode < 0 {
            return o;
        }
    }
    o.query = !r.columns.is_empty() || r.statement == "SELECT";
    if !o.query {
        o.messages.extend(completion(m, &r));
    }
    o.columns = r.columns.into_iter().map(|(n, _)| n).collect();
    o.rows = r.rows;
    o.truncated = r.truncated;
    o
}

/// One statement as the user: the session started with the naming and
/// isolation, the schema set, the statement run, then the session's
/// pending changes committed.
fn run_sql(m: &Main, user: &str, statement: &str, opts: &SqlOptions, max_rows: Option<usize>) -> SqlOutcome {
    let naming = if opts.naming.eq_ignore_ascii_case("*SYS") { "*SYS" } else { "*SQL" };
    let iso = opts.isolation.trim().to_ascii_uppercase();
    let started = match m.ask(|l| l.isql_start(naming.into(), iso)) {
        Ok(s) => s,
        Err(e) => {
            let (id, text) = e.split_once(' ').unwrap_or(("SQL0901", e.as_str()));
            return SqlOutcome {
                messages: vec![Message::new(Kind::Error, id, text)],
                ..SqlOutcome::default()
            };
        }
    };
    let schema = if opts.schema.trim().is_empty() { user } else { opts.schema.as_str() };
    let _ = run(m, &format!("SET SCHEMA {}", schema.trim().to_ascii_uppercase()), naming, 0);
    let max = max_rows.map_or(u32::MAX, |n| u32::try_from(n).unwrap_or(u32::MAX));
    let t0 = Instant::now();
    let r = run(m, statement.trim().trim_end_matches(';'), naming, max);
    let ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
    if started {
        let _ = m.ask(|l| l.isql_end(true));
    }
    outcome(m, r, ms)
}

impl Db for LiveDb {
    fn schemas(&self, user: &str) -> Vec<Schema> {
        self.link.read(user, |m| schemas(m))
    }

    fn sql_schemas(&self, user: &str) -> Vec<String> {
        let u = user.trim().to_ascii_uppercase();
        let mut names: Vec<String> = self.schemas(user).into_iter().map(|s| s.name).collect();
        // The user's own schema first, else QGPL.
        let first = if names.contains(&u) { u } else { "QGPL".to_string() };
        if let Some(i) = names.iter().position(|n| *n == first) {
            let n = names.remove(i);
            names.insert(0, n);
        }
        names
    }

    fn objects(&self, user: &str, schema: &str, kind: ObjKind) -> Vec<DbObject> {
        let s = schema.to_string();
        self.link.read(user, move |m| objects(m, &s, kind))
    }

    fn table(&self, user: &str, schema: &str, table: &str) -> Option<TableDesc> {
        let (s, t) = (schema.to_string(), table.to_string());
        self.link.read(user, move |m| describe(m, &s, &t))
    }

    fn ddl(&self, _user: &str, schema: &str, table: &str) -> Result<String, Message> {
        Err(Message::new(
            Kind::Error,
            "SQL0084",
            &format!(
                "Generate SQL for {}.{} is not available: this system has no QSQGNDDL API.",
                schema.trim(),
                table.trim()
            ),
        ))
    }

    fn run_sql(&self, user: &str, statement: &str, opts: &SqlOptions, max_rows: Option<usize>) -> SqlOutcome {
        let (u, s, o) = (user.trim().to_ascii_uppercase(), statement.to_string(), opts.clone());
        let r = self.link.read(user, move |m| Some(run_sql(m, &u, &s, &o, max_rows)));
        r.unwrap_or_else(|| SqlOutcome {
            messages: vec![Message::new(Kind::Error, "CPF9897", "Navigator could not run the statement.")],
            ..SqlOutcome::default()
        })
    }

    fn explain(&self, _user: &str, _statement: &str, _opts: &SqlOptions) -> Result<Vec<String>, Message> {
        Err(Message::new(Kind::Error, "SQL0084", "EXPLAIN is not supported on this system."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(stamp_secs("2026-11-01 00:00:00.000000"), 1_793_491_200);
        assert_eq!(stamp_secs("2026-11-01-01.00.00.000000"), 1_793_491_200 + 3600);
        assert_eq!(stamp_secs(""), 0);
        assert_eq!(cyy_secs("1261101120000", -240), 1_793_491_200 + 16 * 3600);
        assert_eq!(lit("o'k"), "'O''K'");
    }
}
