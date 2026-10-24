// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Embedded SQL: what a compiled program's statements ask of the
//! database. The precompiler writes each statement as a [`Desc`] (its
//! kind, names and text with `?` for input host variables) into the
//! program; at run time the runtime adds the input values and the scope
//! (the module) to make a [`Request`], and an [`Embedded`] answers it
//! with the SQLCA and the rows. [`Session`] answers over a store: one
//! per job, cursors and prepared statements kept per scope.

use std::cell::{RefCell, RefMut};
use std::collections::HashMap;
use std::rc::Rc;

use db::cmt::{CmtScope, LckLvl};
use store::Store;
use store::obj::ops::NameContext;
use store::obj::{JobHandle, ObjectId};

use crate::ast::{InsertSource, Naming, ObjectName, QueryBody, Statement, TableRef, Where};
use crate::error::SqlError;
use crate::exec::cursor::Cursor;
use crate::exec::{Outcome, SqlContext, prepare};
use crate::sqlca::Sqlca;
use crate::value::Value;

pub mod describe;

/// What a statement does at run time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Op {
    /// `SELECT INTO`, `VALUES INTO`, `SET :hv =`: at most one row.
    #[default]
    Query,
    /// A statement without a result: `INSERT`, `UPDATE`, `DELETE`, ...
    Exec,
    /// `OPEN`, with the cursor's declaration.
    Open,
    /// `FETCH`.
    Fetch,
    /// `CLOSE`.
    Close,
    /// `PREPARE name FROM`: the text is the first input or the literal.
    Prepare,
    /// `EXECUTE name [USING]`.
    Execute,
    /// `EXECUTE IMMEDIATE`: the text is the first input or the literal.
    ExecImm,
    /// `GET DIAGNOSTICS`: the items, comma-separated, are the text.
    Diag,
    /// `DESCRIBE [INPUT] name INTO :sqlda`: one row per entry (type,
    /// length, name); `orient` is `INPUT` for the markers.
    Describe,
    /// The end of a scope (`CLOSQLCSR(*ENDMOD)`) or, for a scope that
    /// ends in `:`, of every scope of an activation group: its cursors
    /// close and its prepared statements go.
    End,
    /// A statement with nothing to do at run time.
    Nothing,
}

impl Op {
    fn letter(self) -> char {
        match self {
            Self::Query => 'Q',
            Self::Exec => 'X',
            Self::Open => 'O',
            Self::Fetch => 'F',
            Self::Close => 'C',
            Self::Prepare => 'P',
            Self::Execute => 'E',
            Self::ExecImm => 'I',
            Self::Diag => 'D',
            Self::Describe => 'B',
            Self::End => 'Z',
            Self::Nothing => 'N',
        }
    }

    fn of(c: char) -> Option<Self> {
        Some(match c {
            'Q' => Self::Query,
            'X' => Self::Exec,
            'O' => Self::Open,
            'F' => Self::Fetch,
            'C' => Self::Close,
            'P' => Self::Prepare,
            'E' => Self::Execute,
            'I' => Self::ExecImm,
            'D' => Self::Diag,
            'B' => Self::Describe,
            'Z' => Self::End,
            'N' => Self::Nothing,
            _ => return None,
        })
    }
}

/// A statement as the precompiler leaves it in the program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the statement's clauses
pub struct Desc {
    /// What it does.
    pub op: Op,
    /// The cursor or the prepared statement it names.
    pub name: String,
    /// For `OPEN` of a cursor declared for a prepared statement: that
    /// statement's name.
    pub stmt: String,
    /// `WITH HOLD`.
    pub hold: bool,
    /// `SCROLL`.
    pub scroll: bool,
    /// `FETCH ... FOR n ROWS`: the count is the last input.
    pub rows: bool,
    /// `FETCH` orientation (`NEXT`, `PRIOR`, `RELATIVE ?`, ...).
    pub orient: String,
    /// `*SQL` naming.
    pub sql_naming: bool,
    /// `PREPARE ... INTO`: described as it is prepared.
    pub into: bool,
    /// The commitment control the program was precompiled with
    /// (`COMMIT(*CHG|*CS|*ALL|*RR)`); `None` for `*NONE`.
    pub commit: Option<LckLvl>,
    /// The statement text (the cursor's query for `OPEN`).
    pub text: String,
}

/// The lock level of a `COMMIT` option: `*CHG` (`*UR`), `*CS`, `*ALL`
/// (`*RS`) and `*RR` (kept as `*ALL`); `None` for `*NONE` (`*NC`).
#[must_use]
pub fn commit_level(opt: &str) -> Option<LckLvl> {
    match opt.trim().to_ascii_uppercase().as_str() {
        "*CHG" | "*UR" => Some(LckLvl::Chg),
        "*CS" => Some(LckLvl::Cs),
        "*ALL" | "*RS" | "*RR" => Some(LckLvl::All),
        _ => None,
    }
}

impl Desc {
    /// The text form: op and flags, the names and the orientation
    /// separated by `;`, then `|` and the statement text.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut flags = String::new();
        flags.push(self.op.letter());
        for (on, c) in [(self.hold, 'H'), (self.scroll, 'S'), (self.rows, 'R'), (self.sql_naming, 'N'), (self.into, 'D')] {
            if on {
                flags.push(c);
            }
        }
        flags.push_str(match self.commit {
            None => "",
            Some(LckLvl::Chg) => "G",
            Some(LckLvl::Cs) => "T",
            Some(LckLvl::All) => "A",
        });
        format!("{flags};{};{};{}|{}", self.name, self.stmt, self.orient, self.text)
    }

    /// From the text form.
    #[must_use]
    pub fn decode(s: &str) -> Option<Self> {
        let (head, text) = s.split_once('|')?;
        let mut parts = head.split(';');
        let flags = parts.next()?;
        let mut chars = flags.chars();
        let op = Op::of(chars.next()?)?;
        let rest: String = chars.collect();
        Some(Self {
            op,
            name: parts.next()?.to_string(),
            stmt: parts.next()?.to_string(),
            orient: parts.next()?.to_string(),
            hold: rest.contains('H'),
            scroll: rest.contains('S'),
            rows: rest.contains('R'),
            sql_naming: rest.contains('N'),
            into: rest.contains('D'),
            commit: if rest.contains('G') {
                Some(LckLvl::Chg)
            } else if rest.contains('T') {
                Some(LckLvl::Cs)
            } else if rest.contains('A') {
                Some(LckLvl::All)
            } else {
                None
            },
            text: text.to_string(),
        })
    }

    /// The naming mode.
    #[must_use]
    pub fn naming(&self) -> Naming {
        if self.sql_naming { Naming::Sql } else { Naming::Sys }
    }
}

/// One statement to run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    /// Who runs it: cursors and prepared statements belong to a scope.
    pub scope: String,
    /// The statement.
    pub desc: Desc,
    /// The input host variables' values, in order.
    pub inputs: Vec<Value>,
}

/// What came back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reply {
    /// The SQLCA.
    pub sqlca: Sqlca,
    /// The rows for the output host variables.
    pub rows: Vec<Vec<Value>>,
}

impl Reply {
    fn of(sqlca: Sqlca) -> Self {
        Self { sqlca, rows: Vec::new() }
    }
}

/// A value between processes: a tag (0 null, 1 number, 2 text, 3
/// date, 4 time, 5 timestamp) and its text.
pub type WireValue = (u8, String);

/// A reply between processes: SQLCODE, SQLSTATE, SQLERRD, SQLWARN,
/// SQLERRMC and the rows.
pub type WireReply = (i32, String, Vec<i32>, String, String, Vec<Vec<WireValue>>);

/// A value as it crosses to another process.
#[must_use]
pub fn to_wire(v: &Value) -> WireValue {
    match v {
        Value::Null => (0, String::new()),
        Value::Num(d) => (1, d.to_string()),
        Value::Str(s) => (2, s.clone()),
        Value::Date(_) => (3, v.to_string()),
        Value::Time(_) => (4, v.to_string()),
        Value::Timestamp(t) => (5, t.to_string()),
    }
}

/// A value from another process.
#[must_use]
pub fn from_wire(w: &WireValue) -> Value {
    let (tag, t) = w;
    match tag {
        1 => qdecimal::Decimal::parse(t).map_or(Value::Null, Value::Num),
        2 => Value::Str(t.clone()),
        3 => crate::value::parse_date(t).map_or(Value::Null, Value::Date),
        4 => db::record::datetime::Time::parse(t, dds::TimeFmt::Iso).map_or(Value::Null, Value::Time),
        5 => crate::value::Timestamp::parse(t).map_or(Value::Null, Value::Timestamp),
        _ => Value::Null,
    }
}

impl Reply {
    /// The reply as it crosses to another process.
    #[must_use]
    pub fn to_wire(&self) -> WireReply {
        let c = &self.sqlca;
        (
            c.sqlcode,
            c.sqlstate.clone(),
            c.sqlerrd.to_vec(),
            c.sqlwarn.iter().collect(),
            c.sqlerrmc.clone(),
            self.rows.iter().map(|r| r.iter().map(to_wire).collect()).collect(),
        )
    }

    /// A reply from another process.
    #[must_use]
    pub fn from_wire(w: &WireReply) -> Self {
        let (code, state, errd, warn, errmc, rows) = w;
        let mut sqlca = Sqlca {
            sqlcode: *code,
            sqlstate: state.clone(),
            sqlerrmc: errmc.clone(),
            ..Sqlca::default()
        };
        for (i, e) in errd.iter().take(6).enumerate() {
            sqlca.sqlerrd[i] = *e;
        }
        for (i, c) in warn.chars().take(11).enumerate() {
            sqlca.sqlwarn[i] = c;
        }
        Self {
            sqlca,
            rows: rows.iter().map(|r| r.iter().map(from_wire).collect()).collect(),
        }
    }
}

/// Whoever runs a job's embedded statements.
pub trait Embedded {
    /// Run one statement.
    fn run(&mut self, req: &Request) -> Reply;
}

/// Whoever owns the store and lends it per statement.
pub trait StoreLend {
    /// The store, borrowed for one statement.
    fn store(&self) -> RefMut<'_, Store>;
}

impl StoreLend for Rc<RefCell<Store>> {
    fn store(&self) -> RefMut<'_, Store> {
        self.borrow_mut()
    }
}

/// A cursor's declaration and a scope's prepared statements.
#[derive(Default)]
struct Scope {
    cursors: HashMap<String, Cursor>,
    prepared: HashMap<String, String>,
}

/// A job's embedded SQL over a store. A scope is `group:module` (the
/// activation group's number); its cursors stay open and its
/// statements prepared until they are closed, the scope ends
/// ([`Op::End`]: the module returned under `CLOSQLCSR(*ENDMOD)`, or the
/// activation group ended) or the session ends.
pub struct Session {
    store: Box<dyn StoreLend>,
    /// The SQL context: naming, library list, commitment control.
    pub ctx: SqlContext,
    scopes: HashMap<String, Scope>,
    last: Sqlca,
}

/// An SQLCA with a code and state of its own.
fn code(sqlcode: i32, state: &str) -> Sqlca {
    Sqlca {
        sqlcode,
        sqlstate: state.into(),
        ..Sqlca::default()
    }
}

/// The tables a statement names at its top level.
#[must_use]
pub fn tables(st: &Statement) -> Vec<ObjectName> {
    fn from_ref(r: &TableRef, out: &mut Vec<ObjectName>) {
        match r {
            TableRef::Table { name, .. } => out.push(name.clone()),
            TableRef::Subquery { query, .. } => from_body(&query.body, out),
            TableRef::Join { left, right, .. } => {
                from_ref(left, out);
                from_ref(right, out);
            }
        }
    }
    fn from_body(b: &QueryBody, out: &mut Vec<ObjectName>) {
        match b {
            QueryBody::Select(s) => {
                for r in &s.from {
                    from_ref(r, out);
                }
            }
            QueryBody::Values(_) => {}
            QueryBody::Union { left, right, .. } => {
                from_body(left, out);
                from_body(right, out);
            }
            QueryBody::Nested(q) => from_body(&q.body, out),
        }
    }
    let mut out = Vec::new();
    match st {
        Statement::Select(q) => from_body(&q.body, &mut out),
        Statement::DeclareCursor(d) => from_body(&d.query.body, &mut out),
        Statement::Insert(i) => {
            out.push(i.table.clone());
            if let InsertSource::Query(q) = &i.source {
                from_body(&q.body, &mut out);
            }
        }
        Statement::Update(u) => out.push(u.table.clone()),
        Statement::Delete(d) => out.push(d.table.clone()),
        Statement::Merge(m) => out.push(m.target.clone()),
        _ => {}
    }
    out
}

impl Session {
    /// A session for `job` over `store`, resolving names in `names`.
    #[must_use]
    pub fn new(store: Box<dyn StoreLend>, qsys: ObjectId, job: JobHandle, names: NameContext) -> Self {
        Self::with_ctx(store, SqlContext::new(qsys, job, names))
    }

    /// A session with a context of its own making (shared record locks).
    #[must_use]
    pub fn with_ctx(store: Box<dyn StoreLend>, ctx: SqlContext) -> Self {
        Self {
            store,
            ctx,
            scopes: HashMap::new(),
            last: Sqlca::default(),
        }
    }

    /// Run a statement inside one store transaction, with the scope's
    /// cursors in the context.
    fn with_txn(&mut self, scope: &str, f: impl FnOnce(&mut store::txn::Txn<'_>, &mut SqlContext, &mut Scope) -> Reply) -> Reply {
        let mut sc = self.scopes.remove(scope).unwrap_or_default();
        std::mem::swap(&mut self.ctx.cursors, &mut sc.cursors);
        let store = &self.store;
        let mut st = store.store();
        let reply = match st.begin() {
            Ok(mut txn) => {
                self.ctx.nulls_eliminated = false;
                let mut r = f(&mut txn, &mut self.ctx, &mut sc);
                if self.ctx.nulls_eliminated && r.sqlca.sqlcode == 0 {
                    r.sqlca.sqlstate = "01003".into();
                    r.sqlca.sqlwarn[0] = 'W';
                    r.sqlca.sqlwarn[2] = 'W';
                }
                match txn.commit() {
                    Ok(()) => r,
                    Err(e) => Reply::of(Sqlca::from_error(&SqlError::from(e))),
                }
            }
            Err(e) => Reply::of(Sqlca::from_error(&SqlError::from(e))),
        };
        drop(st);
        std::mem::swap(&mut self.ctx.cursors, &mut sc.cursors);
        self.scopes.insert(scope.to_string(), sc);
        reply
    }

    /// The statement text of `PREPARE` and `EXECUTE IMMEDIATE`: the
    /// first input when there is one, else the literal.
    fn dynamic_text(req: &Request) -> String {
        match req.inputs.first() {
            Some(v) => v.as_text().trim_end().to_string(),
            None => req.desc.text.clone(),
        }
    }

    /// Commitment control for a statement: under `COMMIT(*CHG|*CS|*ALL)`
    /// the activation group's definition, started at its first
    /// statement when neither the group nor the job has one; under
    /// `*NONE` its changes are not under commitment control.
    fn commitment(&mut self, req: &Request) -> Option<Reply> {
        let group = format!("*SQL{}", req.scope.split(':').next().unwrap_or(""));
        self.ctx.session.set_cmt_group(Some(group.clone()));
        self.ctx.no_commit = req.desc.commit.is_none();
        let lcklvl = req.desc.commit?;
        if self.ctx.session.commit_def().is_some() || matches!(req.desc.op, Op::End | Op::Nothing | Op::Diag) {
            return None;
        }
        let now = self.ctx.clock();
        let store = &self.store;
        let mut st = store.store();
        let r = st.begin().map_err(SqlError::from).and_then(|mut txn| {
            self.ctx.session.strcmtctl_in(&mut txn, lcklvl, None, now, CmtScope::ActGrp(group)).map_err(SqlError::from)?;
            txn.commit().map_err(SqlError::from)
        });
        r.err().map(|e| Reply::of(Sqlca::from_error(&e)))
    }

    fn run_inner(&mut self, req: &Request) -> Reply {
        let d = &req.desc;
        let naming = d.naming();
        match d.op {
            Op::Nothing => Reply::default(),
            Op::End => self.end(&req.scope),
            Op::Diag => self.diagnostics(&d.text),
            Op::Describe => self.describe(req),
            Op::Query => self.with_txn(&req.scope, |txn, ctx, _| {
                let p = match prepare(&d.text, naming) {
                    Ok(p) => p,
                    Err(e) => return Reply::of(Sqlca::from_error(&e)),
                };
                match p.execute(txn, ctx, &req.inputs) {
                    Ok(Outcome::Rows(rs)) => match rs.rows.len() {
                        0 => Reply::of(Sqlca::no_data()),
                        1 => Reply {
                            sqlca: Sqlca::rows(1),
                            rows: rs.rows,
                        },
                        _ => Reply {
                            sqlca: code(-811, "21000"),
                            rows: rs.rows.into_iter().take(1).collect(),
                        },
                    },
                    Ok(o) => Reply::of(Sqlca::from_outcome(&o)),
                    Err(e) => Reply::of(Sqlca::from_error(&e)),
                }
            }),
            Op::Exec => self.with_txn(&req.scope, |txn, ctx, _| exec_text(txn, ctx, &d.text, naming, &req.inputs)),
            Op::ExecImm => {
                let text = Self::dynamic_text(req);
                let inputs: &[Value] = if req.inputs.is_empty() { &[] } else { &req.inputs[1..] };
                self.with_txn(&req.scope, |txn, ctx, _| {
                    if matches!(prepare(&text, naming).map(|p| p.statement), Ok(Statement::Select(_))) {
                        return Reply::of(code(-84, "42612"));
                    }
                    exec_text(txn, ctx, &text, naming, inputs)
                })
            }
            Op::Prepare => {
                let text = Self::dynamic_text(req);
                self.with_txn(&req.scope, |txn, ctx, sc| {
                    sc.prepared.remove(&d.name);
                    let p = match prepare(&text, naming) {
                        Ok(p) => p,
                        Err(e) => return Reply::of(Sqlca::from_error(&e)),
                    };
                    for t in tables(&p.statement) {
                        ctx.naming = naming;
                        if let Err(e) = ctx.resolve_file(txn, &t) {
                            return Reply::of(Sqlca::from_error(&e));
                        }
                    }
                    sc.prepared.insert(d.name.clone(), text.clone());
                    if d.into {
                        return describe::reply(txn, ctx, &text, naming, false);
                    }
                    Reply::default()
                })
            }
            Op::Execute => self.with_txn(&req.scope, |txn, ctx, sc| {
                let Some(text) = sc.prepared.get(&d.name).cloned() else {
                    return Reply::of(code(-518, "07003"));
                };
                exec_text(txn, ctx, &text, naming, &req.inputs)
            }),
            Op::Open => self.with_txn(&req.scope, |txn, ctx, sc| {
                if ctx.cursors.get(&d.name).is_some_and(|c| c.open) {
                    return Reply::of(code(-502, "24502"));
                }
                let query = if d.stmt.is_empty() {
                    d.text.clone()
                } else {
                    match sc.prepared.get(&d.stmt) {
                        Some(t) => t.clone(),
                        None => return Reply::of(code(-514, "26501")),
                    }
                };
                let decl = format!(
                    "DECLARE {} {}CURSOR {}FOR {query}",
                    d.name,
                    if d.scroll { "SCROLL " } else { "" },
                    if d.hold { "WITH HOLD " } else { "" }
                );
                let r = exec_text(txn, ctx, &decl, naming, &[]);
                if r.sqlca.is_error() {
                    return r;
                }
                let r = exec_text(txn, ctx, &format!("OPEN {}", d.name), naming, &req.inputs);
                if r.sqlca.sqlcode == 100 { Reply::default() } else { r }
            }),
            Op::Fetch => self.with_txn(&req.scope, |txn, ctx, _| fetch(txn, ctx, d, &req.inputs)),
            Op::Close => self.with_txn(&req.scope, |txn, ctx, _| {
                if !ctx.cursors.get(&d.name).is_some_and(|c| c.open) {
                    return Reply::of(code(-501, "24501"));
                }
                let r = exec_text(txn, ctx, &format!("CLOSE {}", d.name), naming, &[]);
                if r.sqlca.sqlcode == 100 { Reply::default() } else { r }
            }),
        }
    }

    /// `DESCRIBE [INPUT]` of a statement the scope prepared.
    fn describe(&mut self, req: &Request) -> Reply {
        let d = &req.desc;
        self.with_txn(&req.scope, |txn, ctx, sc| {
            let Some(text) = sc.prepared.get(&d.name).cloned() else {
                return Reply::of(code(-516, "26501"));
            };
            describe::reply(txn, ctx, &text, d.naming(), d.orient == "INPUT")
        })
    }

    /// The end of a scope, or of every scope of a group (`N:`): its
    /// cursors and prepared statements go.
    fn end(&mut self, scope: &str) -> Reply {
        if scope.ends_with(':') {
            self.scopes.retain(|k, _| !k.starts_with(scope));
        } else {
            self.scopes.remove(scope);
        }
        Reply::default()
    }

    /// `GET DIAGNOSTICS`: the items of the last statement.
    fn diagnostics(&self, items: &str) -> Reply {
        let ca = &self.last;
        let row = items
            .split(',')
            .map(|i| match i.trim() {
                "ROW_COUNT" => Value::int(i64::from(ca.sqlerrd[2])),
                "NUMBER" => Value::int(1),
                "DB2_RETURNED_SQLCODE" => Value::int(i64::from(ca.sqlcode)),
                "RETURNED_SQLSTATE" => Value::Str(ca.sqlstate.clone()),
                "DB2_MESSAGE_ID" => Value::Str(if ca.sqlcode == 0 { String::new() } else { ca.msgid() }),
                "MESSAGE_TEXT" => Value::Str(ca.sqlerrmc.clone()),
                "MESSAGE_LENGTH" => Value::int(i64::try_from(ca.sqlerrmc.len()).unwrap_or(0)),
                "MORE" => Value::Str("N".into()),
                _ => Value::Null,
            })
            .collect();
        Reply {
            sqlca: Sqlca::default(),
            rows: vec![row],
        }
    }
}

impl Embedded for Session {
    fn run(&mut self, req: &Request) -> Reply {
        let r = match self.commitment(req) {
            Some(r) => r,
            None => self.run_inner(req),
        };
        if req.desc.op != Op::Diag {
            self.last = r.sqlca.clone();
        }
        r
    }
}

/// Parse and run a statement without a result.
fn exec_text(txn: &mut store::txn::Txn<'_>, ctx: &mut SqlContext, text: &str, naming: Naming, inputs: &[Value]) -> Reply {
    let p = match prepare(text, naming) {
        Ok(p) => p,
        Err(e) => return Reply::of(Sqlca::from_error(&e)),
    };
    match &p.statement {
        // A positioned change needs its cursor open.
        Statement::Update(crate::ast::Update {
            where_clause: Where::CurrentOf(c),
            ..
        })
        | Statement::Delete(crate::ast::Delete {
            where_clause: Where::CurrentOf(c),
            ..
        }) if !ctx.cursors.get(&c.value).is_some_and(|k| k.open) => return Reply::of(code(-507, "24501")),
        // COMMIT and ROLLBACK need commitment control.
        Statement::Commit { .. } | Statement::Rollback { .. } if ctx.session.commit_def().is_none() => {
            return Reply::of(code(-7007, "51009"));
        }
        _ => {}
    }
    match p.execute(txn, ctx, inputs) {
        Ok(o) => Reply::of(Sqlca::from_outcome(&o)),
        Err(e) => Reply::of(Sqlca::from_error(&e)),
    }
}

/// `FETCH`: one row, or up to the count for `FOR n ROWS`.
fn fetch(txn: &mut store::txn::Txn<'_>, ctx: &mut SqlContext, d: &Desc, inputs: &[Value]) -> Reply {
    let Some(c) = ctx.cursors.get(&d.name) else {
        return Reply::of(code(-501, "24501"));
    };
    if !c.open {
        return Reply::of(code(-501, "24501"));
    }
    if !c.scroll && !matches!(d.orient.as_str(), "" | "NEXT") {
        return Reply::of(code(-231, "22006"));
    }
    let naming = d.naming();
    let (inputs, count) = if d.rows {
        let n = inputs.last().and_then(|v| v.as_i64().ok()).unwrap_or(1);
        (&inputs[..inputs.len().saturating_sub(1)], Some(n))
    } else {
        (inputs, None)
    };
    let orient = if d.orient.is_empty() { "NEXT" } else { d.orient.as_str() };
    let text = format!("FETCH {orient} FROM {}", d.name);
    let p = match prepare(&text, naming) {
        Ok(p) => p,
        Err(e) => return Reply::of(Sqlca::from_error(&e)),
    };
    if matches!(orient, "BEFORE" | "AFTER") {
        return match p.execute(txn, ctx, inputs) {
            Ok(_) => Reply::default(),
            Err(e) => Reply::of(Sqlca::from_error(&e)),
        };
    }
    let mut rows = Vec::new();
    let want = count.unwrap_or(1).max(1);
    for i in 0..want {
        let p = if i == 0 {
            p.clone()
        } else {
            match prepare(&format!("FETCH NEXT FROM {}", d.name), naming) {
                Ok(p) => p,
                Err(e) => return Reply::of(Sqlca::from_error(&e)),
            }
        };
        match p.execute(txn, ctx, inputs) {
            Ok(Outcome::Rows(rs)) if rs.rows.is_empty() => break,
            Ok(Outcome::Rows(rs)) => rows.extend(rs.rows),
            Ok(_) => break,
            Err(e) => return Reply::of(Sqlca::from_error(&e)),
        }
    }
    if rows.is_empty() {
        return Reply::of(Sqlca::no_data());
    }
    let mut ca = Sqlca::rows(rows.len() as u64);
    if count.is_some()
        && let Some(c) = ctx.cursors.get(&d.name)
        && c.pos >= c.rows.len()
    {
        ca.sqlerrd[4] = 100;
    }
    Reply { sqlca: ca, rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_cross_processes() {
        let r = Reply {
            sqlca: code(-811, "21000"),
            rows: vec![vec![Value::int(5), Value::Str("x ".into()), Value::Null]],
        };
        assert_eq!(Reply::from_wire(&r.to_wire()), r);
    }

    #[test]
    fn descriptors_round_trip() {
        let d = Desc {
            op: Op::Fetch,
            name: "C1".into(),
            hold: true,
            rows: true,
            orient: "RELATIVE ?".into(),
            into: true,
            text: "SELECT A | B FROM T".into(),
            ..Desc::default()
        };
        assert_eq!(Desc::decode(&d.encode()), Some(d));
    }
}
