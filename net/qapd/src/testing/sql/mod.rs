// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's database: a small statement runner over tables in
//! memory, not QS/400's engine, giving the `Sql` service the answers
//! the real one gives for the cases a client meets. It runs the
//! statements [`parse`] knows, over one table each, with the
//! assignment rules of [`values`], and reports failures with the
//! SQLSTATE, SQLCODE and message a database gives.
//!
//! Its tables ([`fixtures`]):
//!
//! - `HLINV.ASSETS` (key `ASSET_TAG`, journaled): Navigator's sample
//!   columns and its twelve assets in service; `HLINV.LOCATIONS`; the
//!   view `HLINV.ASSETS_ACTIVE` (the eleven `ACTIVE`), the alias
//!   `HLINV.HW` and the logical file `HLINV.ASSETSL1` over `ASSETS`;
//! - `HLTEST.ALLTYPES` (key `ID`): one column of every type the
//!   protocol carries, `DECFLOAT` (`1.5E-30`, `Infinity`, `-Infinity`,
//!   `NaN`, `sNaN`), `CHAR(8) FOR BIT DATA` (CCSID 65535, sent as
//!   `Binary`), `TIMESTAMP(12)` and `BOOLEAN` among them, and a
//!   seventh row null but for `ID`;
//! - `HLTEST.JRN` (journaled) and `HLTEST.NOJRN` (not journaled; a
//!   change under a commit level other than `None` is `SQL7008`,
//!   SQLSTATE `55019`), both `ID INTEGER` (the key) and `NAME`;
//! - `HLTEST.SLOW`, 300 rows that take 10 ms each to fetch, and
//!   `HLTEST.SLOWLOG`, whose inserts take 10 ms a row: both look at the
//!   cancel between rows;
//! - `HLTEST.BIG`, 3000 rows of 1000 characters: a fetch of all of it
//!   is over 1 MiB;
//! - `HLPAY.SALARIES`, which only `ANN` may use: for `BOB`, `Prepare`
//!   and `ListColumns` answer `NotAuthorized` with `SQL0551` (the
//!   status the operations' table gives, rather than `SqlError`); the
//!   catalog still lists it;
//! - `QGPL.QCLSRC`, a physical file; `HLMANY.T001` to `T150` and the
//!   schemas `ARCH001` to `ARCH140`, so `ListTables` and `ListSchemas`
//!   take several pages.
//!
//! Inserting a key a table has is `SQL0803` (`23505`), so a multi-row
//! `Execute` fails at the chosen row. Simplifications of a test double:
//! no locks (other connections see changes before they are committed),
//! a rollback puts back the tables the transaction changed as they
//! were at its first change, cursors stay open across `Commit` and
//! `Rollback`, and unqualified names are in the default schema, else
//! the profile's schema (`Sql` naming) or `QGPL` then `HLINV` (`System`
//! naming).

pub mod fixtures;
pub mod parse;
pub mod values;

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;

use qap_proto::ops::sql::{
    Column, Columns, CommitLevel, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables, Naming, Prepared,
    Schema, Schemas, SetOptions, SqlFailure, SqlValue, Table as TableEntry, TableKind, Tables,
};
use qap_proto::{ServerMessage, Status};

use self::fixtures::Table;
use self::parse::{Expr, Filter, Stmt, TableRef};
use self::values::{assign, literal, same};
use super::lock;
use crate::host::Refusal;
use crate::sql::{CANCELLED, SqlResult, SqlStop, like, page};

/// The database every connection shares.
pub struct Db {
    /// Schemas and their descriptions.
    pub schemas: Vec<(String, String)>,
    /// Tables, views, aliases and files.
    pub tables: Vec<Table>,
}

impl Default for Db {
    fn default() -> Self {
        let (schemas, tables) = fixtures::tables();
        Self { schemas, tables }
    }
}

/// Looks a message up for the connection's job.
pub type Msg<'a> = &'a dyn Fn(&str, &[&str]) -> ServerMessage;

/// A failure before its message is looked up.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fail {
    /// `SqlError`: SQLSTATE, SQLCODE, message and its data.
    Sql(&'static str, i32, &'static str, Vec<String>),
    /// Another status, its message and data.
    Refused(Status, &'static str, Vec<String>),
}

impl Fail {
    fn stop<T>(self, msg: Msg<'_>, row: Option<u32>) -> SqlStop<T> {
        match self {
            Fail::Sql(sqlstate, sqlcode, id, data) => {
                let data: Vec<&str> = data.iter().map(String::as_str).collect();
                SqlStop::failed(
                    SqlFailure {
                        sqlstate: sqlstate.into(),
                        sqlcode,
                        row,
                    },
                    msg(id, &data),
                )
            }
            Fail::Refused(status, id, data) => SqlStop::Refused(refusal(msg, status, id, &data)),
        }
    }
}

fn refusal(msg: Msg<'_>, status: Status, id: &str, data: &[String]) -> Refusal {
    let data: Vec<&str> = data.iter().map(String::as_str).collect();
    Refusal {
        status,
        message: msg(id, &data),
    }
}

fn not_found(name: &str, schema: &str) -> Fail {
    Fail::Sql("42704", -204, "SQL0204", vec![name.into(), schema.into()])
}

/// A prepared statement.
#[derive(Debug, Clone)]
struct Prep {
    stmt: Stmt,
    /// The table it reads or changes, by index.
    table: usize,
    columns: Vec<Column>,
    parameters: Vec<Column>,
}

/// An open cursor.
#[derive(Debug, Clone)]
struct Cursor {
    rows: Vec<Vec<SqlValue>>,
    pos: usize,
    slow: Option<std::time::Duration>,
}

/// One connection's statements, cursors, options and transaction.
#[derive(Debug, Default)]
pub struct Conn {
    /// `SetOptions`.
    pub options: SetOptions,
    statements: HashMap<u32, Prep>,
    cursors: HashMap<u32, Cursor>,
    next: u32,
    /// The tables changed in the open transaction, as they were.
    undo: HashMap<usize, Vec<Vec<SqlValue>>>,
    /// A transaction is open: a change ran under a commit level.
    pending: bool,
}

impl Db {
    /// The table `r` names, for `me`, under `o`.
    fn resolve(&self, r: &TableRef, o: &SetOptions, me: &str) -> Result<usize, Fail> {
        if r.slash && o.naming == Naming::Sql {
            return Err(Fail::Sql("42833", -5016, "SQL5016", vec![format!("{}/{}", r.schema.clone().unwrap_or_default(), r.name)]));
        }
        let candidates: Vec<String> = match (&r.schema, &o.default_schema) {
            (Some(s), _) => vec![s.clone()],
            (None, Some(d)) => vec![d.to_ascii_uppercase()],
            (None, None) if o.naming == Naming::Sql => vec![me.to_string()],
            (None, None) => vec!["QGPL".into(), "HLINV".into()],
        };
        let found = candidates.iter().find_map(|s| self.tables.iter().position(|t| t.schema == *s && t.name == r.name));
        let Some(i) = found else {
            return Err(not_found(&r.name, candidates.first().map_or("", String::as_str)));
        };
        let t = &self.tables[i];
        if t.only.is_some_and(|u| u != me) {
            return Err(Fail::Refused(Status::NotAuthorized, "SQL0551", vec![format!("{}.{}", t.schema, t.name)]));
        }
        Ok(i)
    }

    /// The table a view, alias or logical file reads; itself otherwise.
    fn base(&self, i: usize) -> usize {
        match &self.tables[i].over {
            Some((name, _)) => self.tables.iter().position(|t| t.schema == self.tables[i].schema && t.name == *name).unwrap_or(i),
            None => i,
        }
    }

    /// The rows table `i` shows.
    fn rows(&self, i: usize) -> Vec<Vec<SqlValue>> {
        let base = &self.tables[self.base(i)];
        match &self.tables[i].over {
            Some((_, Some((c, v)))) => base.rows.iter().filter(|r| same(&r[*c], v)).cloned().collect(),
            _ => base.rows.clone(),
        }
    }
}

/// The column `name` of `t`, by index.
fn column(t: &Table, name: &str) -> Result<usize, Fail> {
    t.columns
        .iter()
        .position(|c| c.column.name == name)
        .ok_or_else(|| Fail::Sql("42703", -206, "SQL0206", vec![name.into()]))
}

/// A marker's description: the column it stands for, nameless.
fn marker(t: &Table, c: usize) -> Column {
    Column {
        name: String::new(),
        label: String::new(),
        nullable: true,
        ..t.columns[c].column.clone()
    }
}

/// The value of `e`, with `params`, assigned to a column of `col`.
fn assigned(col: &values::Col, e: &Expr, params: &[SqlValue]) -> Result<SqlValue, Fail> {
    let v = match e {
        Expr::Param(n) => params.get(*n).cloned().ok_or_else(|| Fail::Sql("07001", -313, "SQL0313", Vec::new()))?,
        Expr::Lit(l) => literal(l),
    };
    assign(col, v).map_err(|(state, code, id)| Fail::Sql(state, code, id, vec![col.column.name.clone()]))
}

/// The value of `e`, with `params`, assigned to column `c` of `t`.
fn value(t: &Table, c: usize, e: &Expr, params: &[SqlValue]) -> Result<SqlValue, Fail> {
    assigned(&t.columns[c], e, params)
}

/// A `WHERE`'s columns and the values they must equal.
fn wanted(t: &Table, filter: &Filter, params: &[SqlValue]) -> Result<Vec<(usize, SqlValue)>, Fail> {
    filter
        .iter()
        .map(|(name, e)| {
            let c = column(t, name)?;
            let mut col = t.columns[c].clone();
            col.column.nullable = true;
            Ok((c, assigned(&col, e, params)?))
        })
        .collect()
}

/// Whether row `r` has the values `w` wants.
fn passes(r: &[SqlValue], w: &[(usize, SqlValue)]) -> bool {
    w.iter().all(|(c, v)| same(&r[*c], v))
}

impl Conn {
    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// `SetOptions`.
    ///
    /// # Errors
    /// `InUse` (`SQL0752`) while a transaction is open.
    pub fn set_options(&mut self, o: &SetOptions, msg: Msg<'_>) -> Result<(), Refusal> {
        if self.pending {
            return Err(refusal(msg, Status::InUse, "SQL0752", &[]));
        }
        self.options = o.clone();
        Ok(())
    }

    /// `Prepare` as `me`.
    pub fn prepare(&mut self, db: &Mutex<Db>, me: &str, text: &str, msg: Msg<'_>) -> SqlResult<Prepared> {
        self.prepare_in(&lock(db), me, text).map_err(|f| f.stop(msg, None)).map(|p| {
            let statement = self.id();
            let reply = Prepared {
                statement,
                columns: p.columns.clone(),
                parameters: p.parameters.clone(),
            };
            self.statements.insert(statement, p);
            reply
        })
    }

    fn prepare_in(&self, db: &Db, me: &str, text: &str) -> Result<Prep, Fail> {
        let stmt = parse::parse(text).map_err(|tok| Fail::Sql("42601", -104, "SQL0104", vec![tok]))?;
        let table = db.resolve(stmt.table(), &self.options, me)?;
        let t = &db.tables[table];
        let mut parameters = Vec::new();
        let mut columns = Vec::new();
        let markers = |filter: &Filter, out: &mut Vec<Column>| -> Result<(), Fail> {
            for (name, e) in filter {
                let c = column(t, name)?;
                if matches!(e, Expr::Param(_)) {
                    out.push(marker(t, c));
                }
            }
            Ok(())
        };
        match &stmt {
            Stmt::Select { cols, filter, .. } => {
                columns = match cols {
                    None => t.columns.iter().map(|c| c.column.clone()).collect(),
                    Some(names) => names.iter().map(|n| column(t, n).map(|c| t.columns[c].column.clone())).collect::<Result<_, _>>()?,
                };
                markers(filter, &mut parameters)?;
            }
            Stmt::Insert { cols, values, .. } => {
                let targets: Vec<usize> = match cols {
                    None => (0..t.columns.len()).collect(),
                    Some(names) => names.iter().map(|n| column(t, n)).collect::<Result<_, _>>()?,
                };
                if targets.len() != values.len() {
                    return Err(Fail::Sql("42802", -117, "SQL0117", Vec::new()));
                }
                for (c, e) in targets.iter().zip(values) {
                    if matches!(e, Expr::Param(_)) {
                        parameters.push(marker(t, *c));
                    }
                }
            }
            Stmt::Update { sets, filter, .. } => {
                markers(sets, &mut parameters)?;
                markers(filter, &mut parameters)?;
            }
            Stmt::Delete { filter, .. } => markers(filter, &mut parameters)?,
        }
        Ok(Prep {
            stmt,
            table,
            columns,
            parameters,
        })
    }

    /// `Execute` as `me`: a query opens a cursor; a change runs once per
    /// parameter row, stopping at the first that fails.
    pub fn execute(&mut self, db: &Mutex<Db>, req: &Execute, cancelled: &dyn Fn() -> bool, msg: Msg<'_>) -> SqlResult<Executed> {
        let Some(p) = self.statements.get(&req.statement).cloned() else {
            return Err(SqlStop::Refused(refusal(msg, Status::NotFound, "SQL0518", &[format!("S{}", req.statement)])));
        };
        if let Stmt::Select { cols, filter, .. } = &p.stmt {
            let params = req.rows.first().cloned().unwrap_or_default();
            return self.open(db, &p, cols.as_deref(), filter, &params).map_err(|f| f.stop(msg, None));
        }
        let several = !req.rows.is_empty();
        let rows = if several { req.rows.clone() } else { vec![Vec::new()] };
        let mut updated = 0i64;
        for (n, params) in rows.iter().enumerate() {
            if cancelled() {
                return Err(SqlStop::cancelled(Executed { updated, cursor: None }, msg(CANCELLED, &[])));
            }
            if params.len() != p.parameters.len() {
                let row = several.then(|| u32::try_from(n).unwrap_or(u32::MAX));
                return Err(Fail::Sql("07001", -313, "SQL0313", Vec::new()).stop(msg, row));
            }
            let slow = lock(db).tables[p.table].slow;
            if let Some(d) = slow {
                thread::sleep(d);
            }
            match self.change(&mut lock(db), &p, params) {
                Ok(k) => updated += k,
                Err(f) => {
                    let row = several.then(|| u32::try_from(n).unwrap_or(u32::MAX));
                    return Err(f.stop(msg, row));
                }
            }
        }
        Ok(Executed { updated, cursor: None })
    }

    fn open(&mut self, db: &Mutex<Db>, p: &Prep, cols: Option<&[String]>, filter: &Filter, params: &[SqlValue]) -> Result<Executed, Fail> {
        if params.len() != p.parameters.len() {
            return Err(Fail::Sql("07001", -313, "SQL0313", Vec::new()));
        }
        let db = lock(db);
        let t = &db.tables[p.table];
        let picks: Vec<usize> = match cols {
            None => (0..t.columns.len()).collect(),
            Some(names) => names.iter().map(|n| column(t, n)).collect::<Result<_, _>>()?,
        };
        let w = wanted(t, filter, params)?;
        let rows: Vec<Vec<SqlValue>> = db
            .rows(p.table)
            .into_iter()
            .filter(|r| passes(r, &w))
            .map(|r| picks.iter().map(|c| r[*c].clone()).collect())
            .collect();
        let slow = t.slow;
        drop(db);
        let cursor = self.id();
        self.cursors.insert(cursor, Cursor { rows, pos: 0, slow });
        Ok(Executed {
            updated: 0,
            cursor: Some(cursor),
        })
    }

    /// One run of a change with one parameter set: rows changed.
    #[allow(clippy::many_single_char_names)] // table, column, row
    fn change(&mut self, db: &mut Db, p: &Prep, params: &[SqlValue]) -> Result<i64, Fail> {
        let shown = &db.tables[p.table];
        if matches!(shown.kind, TableKind::View | TableKind::LogicalFile) {
            return Err(Fail::Sql("42807", -150, "SQL0150", vec![shown.name.clone()]));
        }
        let ti = db.base(p.table);
        let t = &db.tables[ti];
        if self.options.commit != CommitLevel::None {
            if !t.journaled {
                return Err(Fail::Sql("55019", -7008, "SQL7008", vec![t.name.clone(), t.schema.clone()]));
            }
            self.pending = true;
            self.undo.entry(ti).or_insert_with(|| t.rows.clone());
        }
        let t = &mut db.tables[ti];
        match &p.stmt {
            Stmt::Insert { cols, values, .. } => {
                let targets: Vec<usize> = match cols {
                    None => (0..t.columns.len()).collect(),
                    Some(names) => names.iter().map(|n| column(t, n)).collect::<Result<_, _>>()?,
                };
                let mut row: Vec<SqlValue> = t.columns.iter().map(|_| SqlValue::Null).collect();
                for (c, e) in targets.iter().zip(values) {
                    row[*c] = value(t, *c, e, params)?;
                }
                for (c, v) in row.iter().enumerate() {
                    if *v == SqlValue::Null && !t.columns[c].column.nullable {
                        return Err(Fail::Sql("23502", -407, "SQL0407", vec![t.columns[c].column.name.clone()]));
                    }
                }
                if let Some(k) = t.key
                    && t.rows.iter().any(|r| same(&r[k], &row[k]))
                {
                    return Err(Fail::Sql("23505", -803, "SQL0803", vec![t.name.clone()]));
                }
                t.rows.push(row);
                Ok(1)
            }
            Stmt::Update { sets, filter, .. } => {
                // The SET markers come before the WHERE's.
                let split = sets.iter().filter(|(_, e)| matches!(e, Expr::Param(_))).count();
                let (sp, wp) = params.split_at(split.min(params.len()));
                let mut assigned = Vec::new();
                let mut k = 0;
                for (name, e) in sets {
                    let c = column(t, name)?;
                    let e = match e {
                        Expr::Param(_) => {
                            k += 1;
                            Expr::Param(k - 1)
                        }
                        e @ Expr::Lit(_) => e.clone(),
                    };
                    assigned.push((c, value(t, c, &e, sp)?));
                }
                let w = wanted(t, &renumber(filter), wp)?;
                let mut n = 0;
                for r in t.rows.iter_mut().filter(|r| passes(r, &w)) {
                    for (c, v) in &assigned {
                        r[*c] = v.clone();
                    }
                    n += 1;
                }
                Ok(n)
            }
            Stmt::Delete { filter, .. } => {
                let w = wanted(t, filter, params)?;
                let before = t.rows.len();
                t.rows.retain(|r| !passes(r, &w));
                Ok(i64::try_from(before - t.rows.len()).unwrap_or(i64::MAX))
            }
            Stmt::Select { .. } => Ok(0),
        }
    }

    /// `Fetch`: up to `max_rows` (0 for all); a slow table's rows look
    /// at the cancel one by one.
    pub fn fetch(&mut self, req: &Fetch, cancelled: &dyn Fn() -> bool, msg: Msg<'_>) -> SqlResult<Fetched> {
        let Some(c) = self.cursors.get_mut(&req.cursor) else {
            return Err(SqlStop::Refused(refusal(msg, Status::NotFound, "SQL0501", &[format!("C{}", req.cursor)])));
        };
        let want = if req.max_rows == 0 { usize::MAX } else { usize::try_from(req.max_rows).unwrap_or(usize::MAX) };
        let mut rows = Vec::new();
        while rows.len() < want && c.pos < c.rows.len() {
            if let Some(d) = c.slow {
                thread::sleep(d);
                if cancelled() {
                    self.cursors.remove(&req.cursor);
                    return Err(SqlStop::cancelled(Fetched { rows, end: false }, msg(CANCELLED, &[])));
                }
            }
            rows.push(c.rows[c.pos].clone());
            c.pos += 1;
        }
        let end = c.pos >= c.rows.len();
        Ok(Fetched { rows, end })
    }

    /// `CloseCursor`.
    ///
    /// # Errors
    /// `NotFound` (`SQL0501`) when the cursor is not open.
    pub fn close_cursor(&mut self, cursor: u32, msg: Msg<'_>) -> Result<(), Refusal> {
        match self.cursors.remove(&cursor) {
            Some(_) => Ok(()),
            None => Err(refusal(msg, Status::NotFound, "SQL0501", &[format!("C{cursor}")])),
        }
    }

    /// `Release`.
    ///
    /// # Errors
    /// `NotFound` (`SQL0518`) when there is no such statement.
    pub fn release(&mut self, statement: u32, msg: Msg<'_>) -> Result<(), Refusal> {
        match self.statements.remove(&statement) {
            Some(_) => Ok(()),
            None => Err(refusal(msg, Status::NotFound, "SQL0518", &[format!("S{statement}")])),
        }
    }

    /// `Commit`.
    pub fn commit(&mut self) {
        self.undo.clear();
        self.pending = false;
    }

    /// `Rollback`: the changed tables as they were.
    pub fn rollback(&mut self, db: &Mutex<Db>) {
        let mut db = lock(db);
        for (t, rows) in self.undo.drain() {
            db.tables[t].rows = rows;
        }
        self.pending = false;
    }

    /// The connection ends: roll back, forget its statements and
    /// cursors, options as at the start.
    pub fn end(&mut self, db: &Mutex<Db>) {
        self.rollback(db);
        self.statements.clear();
        self.cursors.clear();
        self.options = SetOptions::default();
    }
}

/// A filter's markers numbered from 0.
fn renumber(f: &Filter) -> Filter {
    let mut k = 0;
    f.iter()
        .map(|(c, e)| {
            let e = match e {
                Expr::Param(_) => {
                    k += 1;
                    Expr::Param(k - 1)
                }
                e @ Expr::Lit(_) => e.clone(),
            };
            (c.clone(), e)
        })
        .collect()
}

/// `ListSchemas`.
#[must_use]
pub fn list_schemas(db: &Db, req: &ListSchemas) -> Schemas {
    let all: Vec<Schema> = db
        .schemas
        .iter()
        .filter(|(n, _)| like(n, &req.pattern))
        .map(|(name, text)| Schema {
            name: name.clone(),
            text: text.clone(),
        })
        .collect();
    let (total, schemas) = page(all, req.start, req.max);
    Schemas { total, schemas }
}

/// `ListTables`.
///
/// # Errors
/// `NotFound` (`SQL0204`) for a schema that does not exist.
pub fn list_tables(db: &Db, req: &ListTables, msg: Msg<'_>) -> Result<Tables, Refusal> {
    if !db.schemas.iter().any(|(n, _)| *n == req.schema) {
        return Err(refusal(msg, Status::NotFound, "SQL0204", &[req.schema.clone(), "*LIB".into()]));
    }
    let all: Vec<TableEntry> = db
        .tables
        .iter()
        .filter(|t| t.schema == req.schema && like(&t.name, &req.pattern))
        .filter(|t| req.kinds.is_empty() || req.kinds.contains(&t.kind))
        .map(|t| TableEntry {
            schema: t.schema.clone(),
            name: t.name.clone(),
            kind: t.kind,
            text: t.text.clone(),
        })
        .collect();
    let (total, tables) = page(all, req.start, req.max);
    Ok(Tables { total, tables })
}

/// `ListColumns` as `me`.
///
/// # Errors
/// `NotFound` (`SQL0204`), or `NotAuthorized` (`SQL0551`).
pub fn list_columns(db: &Db, me: &str, req: &ListColumns, msg: Msg<'_>) -> Result<Columns, Refusal> {
    let Some(t) = db.tables.iter().find(|t| t.schema == req.schema && t.name == req.table) else {
        return Err(refusal(msg, Status::NotFound, "SQL0204", &[req.table.clone(), req.schema.clone()]));
    };
    if t.only.is_some_and(|u| u != me) {
        return Err(refusal(msg, Status::NotAuthorized, "SQL0551", &[format!("{}.{}", t.schema, t.name)]));
    }
    Ok(Columns {
        columns: t.columns.iter().map(|c| c.column.clone()).collect(),
    })
}
