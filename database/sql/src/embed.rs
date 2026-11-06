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

mod arrays;
mod atomic;
pub mod describe;
mod package;

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
    /// length, name); `orient` is `INPUT` for the markers, `CURSOR` for
    /// the open cursor `name`'s columns (a `CALL`'s `RS#n`).
    Describe,
    /// The end of a scope (`CLOSQLCSR(*ENDMOD)`) or, for a scope that
    /// ends in `:`, of every scope of an activation group: its cursors
    /// close and its prepared statements go.
    End,
    /// A statement with nothing to do at run time.
    Nothing,
    /// The prepared statement `name` goes from the scope, as a CLI
    /// statement handle is freed; its open cursors stay.
    Deallocate,
    /// The procedure the last `CALL` planned has returned: the inputs
    /// are its outcome ([`crate::exec::routine::Called`]); its result
    /// sets go to the caller.
    Called,
    /// An SQL package action for a requester (the host server's
    /// extended dynamic support, a DRDA bind): see `embed::package`.
    Package,
    /// A statement the job ran at a remote server (DRDA): the text,
    /// its SQLCODE, SQLSTATE and row count as the inputs; the database
    /// monitor records it, nothing runs.
    Routed,
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
            Self::Deallocate => 'R',
            Self::Called => 'K',
            Self::Package => 'G',
            Self::Routed => 'W',
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
            'R' => Self::Deallocate,
            'K' => Self::Called,
            'G' => Self::Package,
            'W' => Self::Routed,
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
    /// `SENSITIVE`: changes made after `OPEN` show in the rows fetched.
    pub sensitive: bool,
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
    /// `OPEN` of a cursor declared `WITH RETURN`: its result set goes
    /// to the procedure's caller, or its client.
    pub returns: Option<crate::ast::ReturnTo>,
    /// A routine's own evaluation (an `IF` condition, a `SET`): it
    /// leaves the diagnostics of the last statement alone.
    pub quiet: bool,
    /// The requester performs the calls a `CALL` plans
    /// ([`crate::exec::routine::CallPlan`]); without it `CALL` is refused.
    pub calls: bool,
    /// The inputs and outputs that are LOB locators: `I` or `O` and the
    /// position, comma-separated (`I0,O1`). Kept after the orientation
    /// in the text form; a program precompiled before has none.
    pub locs: String,
    /// A statement of an `ATOMIC` compound in a routine without
    /// commitment control: run under the routine's own definition.
    pub atomic: bool,
    /// An SQL procedure's statement: arrays may be used
    /// (`crate::exec::array`).
    pub arrays: bool,
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
        let to_caller = self.returns == Some(crate::ast::ReturnTo::Caller);
        let to_client = self.returns == Some(crate::ast::ReturnTo::Client);
        for (on, c) in [
            (self.hold, 'H'),
            (self.scroll, 'S'),
            (self.rows, 'R'),
            (self.sql_naming, 'N'),
            (self.into, 'D'),
            (to_caller, 'W'),
            (to_client, 'U'),
            (self.quiet, 'V'),
            (self.calls, 'L'),
            (self.sensitive, 'Y'),
            (self.atomic, 'M'),
            (self.arrays, 'Q'),
        ] {
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
        let locs = if self.locs.is_empty() { String::new() } else { format!(";{}", self.locs) };
        format!("{flags};{};{};{}{locs}|{}", self.name, self.stmt, self.orient, self.text)
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
            locs: parts.next().unwrap_or("").to_string(),
            hold: rest.contains('H'),
            scroll: rest.contains('S'),
            rows: rest.contains('R'),
            sql_naming: rest.contains('N'),
            into: rest.contains('D'),
            returns: if rest.contains('W') {
                Some(crate::ast::ReturnTo::Caller)
            } else if rest.contains('U') {
                Some(crate::ast::ReturnTo::Client)
            } else {
                None
            },
            quiet: rest.contains('V'),
            calls: rest.contains('L'),
            sensitive: rest.contains('Y'),
            atomic: rest.contains('M'),
            arrays: rest.contains('Q'),
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
    /// For `CALL`: the call the requester is to make, then report with
    /// [`Op::Called`].
    pub call: Option<Box<crate::exec::routine::CallPlan>>,
}

impl Reply {
    fn of(sqlca: Sqlca) -> Self {
        Self { sqlca, rows: Vec::new(), call: None }
    }
}

/// A value between processes: a tag (0 null, 1 number, 2 text, 3
/// date, 4 time, 5 timestamp, 6 `DECFLOAT`, 7 binary as hexadecimal, 8
/// boolean `1` or `0`, 9 `DOUBLE` and 10 `REAL` in their shortest E
/// notation) and its text. A `DECFLOAT(16)` is sent with a `16:` prefix.
pub type WireValue = (u8, String);

/// A reply between processes: SQLCODE, SQLSTATE, SQLERRD, SQLWARN,
/// SQLERRMC, and the rows with the call plan (`CallPlan::encode`, empty
/// for none).
pub type WireReply = (i32, String, Vec<i32>, String, String, (Vec<Vec<WireValue>>, String));

/// A value as it crosses to another process.
#[must_use]
pub fn to_wire(v: &Value) -> WireValue {
    match v {
        // An array stays in the database: programs hold a handle.
        Value::Null | Value::Array(_) => (0, String::new()),
        Value::Num(d) => (1, d.to_string()),
        Value::Str(s) => (2, s.clone()),
        Value::DecFloat(d) if d.format() == qdecimal::DfFormat::D16 => (6, format!("16:{d}")),
        Value::DecFloat(d) => (6, d.to_string()),
        Value::Bin(b) => (7, crate::value::hex(b)),
        Value::Bool(b) => (8, if *b { "1" } else { "0" }.into()),
        Value::Double(x) => (9, db::record::float_text(*x)),
        Value::Real(x) => (10, if x.is_finite() { format!("{x:E}") } else { db::record::float_text(f64::from(*x)) }),
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
        5 => crate::value::Timestamp::parse_exact(t).map_or(Value::Null, Value::Timestamp),
        6 => match t.strip_prefix("16:") {
            Some(d) => qdecimal::DecFloat::parse(d, qdecimal::DfFormat::D16),
            None => qdecimal::DecFloat::parse(t, qdecimal::DfFormat::D34),
        }
        .map_or(Value::Null, Value::DecFloat),
        7 => crate::value::unhex(t).map_or(Value::Null, Value::Bin),
        8 => Value::Bool(t == "1"),
        9 => crate::value::parse_float(t).map_or(Value::Null, Value::Double),
        #[allow(clippy::cast_possible_truncation)] // a single's text
        10 => crate::value::parse_float(t).map_or(Value::Null, |x| Value::Real(x as f32)),
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
            (
                self.rows.iter().map(|r| r.iter().map(to_wire).collect()).collect(),
                self.call.as_ref().map_or_else(String::new, |p| p.encode()),
            ),
        )
    }

    /// A reply from another process.
    #[must_use]
    pub fn from_wire(w: &WireReply) -> Self {
        let (code, state, errd, warn, errmc, (rows, plan)) = w;
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
            call: crate::exec::routine::CallPlan::decode(plan).map(Box::new),
        }
    }
}

/// Whoever runs a job's embedded statements.
pub trait Embedded {
    /// Run one statement.
    fn run(&mut self, req: &Request) -> Reply;

    /// The `OUT` and `INOUT` values of the last `CALL` run, by marker,
    /// for a requester that makes the calls it plans; none otherwise.
    fn take_outs(&mut self) -> Vec<(usize, Value)> {
        Vec::new()
    }
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
    /// The condition the running handler took (`GET STACKED DIAGNOSTICS`).
    stacked: Sqlca,
    /// The last statement's kind, as `GET DIAGNOSTICS` `COMMAND_FUNCTION` gives it.
    last_cmd: String,
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
            TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
                from_ref(left, out);
                from_ref(right, out);
            }
            TableRef::Lateral(inner) => from_ref(inner, out),
            TableRef::Recursive { cte, .. } => {
                for b in cte.seed.iter().chain(&cte.step) {
                    from_body(b, out);
                }
            }
            TableRef::Function { .. } | TableRef::DataChange { .. } => {}
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
            stacked: Sqlca::default(),
            last_cmd: String::new(),
        }
    }

    /// Run a statement inside one store transaction, with the scope's
    /// cursors in the context.
    fn with_txn(&mut self, scope: &str, f: impl FnOnce(&mut store::txn::Txn<'_>, &mut SqlContext, &mut Scope) -> Reply) -> Reply {
        let mut sc = self.scopes.remove(scope).unwrap_or_default();
        std::mem::swap(&mut self.ctx.cursors, &mut sc.cursors);
        let lender = &self.store;
        // Borrowed through the lender, so a long read can give it back
        // between batches (`SqlContext::yielder`).
        let reply = match store::txn::Txn::lent(Box::new(|| lender.store())) {
            Ok(mut txn) => {
                self.ctx.nulls_eliminated = false;
                let _ = crate::exec::warn::take();
                let mut r = f(&mut txn, &mut self.ctx, &mut sc);
                self.ctx.release_held(&mut txn);
                if self.ctx.nulls_eliminated && r.sqlca.sqlcode == 0 {
                    r.sqlca.sqlstate = "01003".into();
                    r.sqlca.sqlwarn[0] = 'W';
                    r.sqlca.sqlwarn[2] = 'W';
                }
                if let Some(w) = crate::exec::warn::take() {
                    crate::exec::warn::apply(w, &mut r.sqlca);
                }
                match txn.commit() {
                    Ok(()) => r,
                    Err(e) => Reply::of(Sqlca::from_error(&SqlError::from(e))),
                }
            }
            Err(e) => Reply::of(Sqlca::from_error(&SqlError::from(e))),
        };
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
    /// `*NONE` its changes are not under commitment control. The level
    /// is the statement's, or the unit's after `SET TRANSACTION`; the
    /// definition takes it between units of work, and keeps the unit's
    /// while changes are pending.
    fn commitment(&mut self, req: &Request) -> Option<Reply> {
        let group = format!("*SQL{}", req.scope.split(':').next().unwrap_or(""));
        self.ctx.session.set_cmt_group(Some(group.clone()));
        let level = match self.ctx.isolation {
            Some(iso) => crate::exec::lock_level(iso),
            None => req.desc.commit,
        };
        let own = req.desc.atomic && req.desc.commit.is_none();
        let level = if own { Some(level.unwrap_or(LckLvl::Chg)) } else { level };
        self.ctx.no_commit = level.is_none();
        let lcklvl = level?;
        if matches!(req.desc.op, Op::End | Op::Nothing | Op::Diag | Op::Deallocate | Op::Package) {
            return None;
        }
        if own {
            return self.atomic_def(&group, lcklvl).map(Reply::of);
        }
        if let Some(def) = self.ctx.session.commit_def() {
            def.borrow_mut().set_lcklvl(lcklvl);
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

    #[allow(clippy::too_many_lines)] // one arm per operation
    fn run_inner(&mut self, req: &Request) -> Reply {
        let d = &req.desc;
        let naming = d.naming();
        match d.op {
            Op::Nothing | Op::Routed => Reply::default(),
            Op::End => self.end(&req.scope),
            Op::Deallocate => self.deallocate(&req.scope, &d.name),
            Op::Called => self.called(req),
            Op::Package => self.package(req),
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
                            call: None,
                        },
                        _ => Reply {
                            sqlca: code(-811, "21000"),
                            rows: rs.rows.into_iter().take(1).collect(),
                            call: None,
                        },
                    },
                    Ok(o) => Reply::of(Sqlca::from_outcome(&o)),
                    Err(e) => Reply::of(Sqlca::from_error(&e)),
                }
            }),
            // A blocked INSERT: the rows' values one after another, the
            // count last; the statement once per row, SQLERRD(3) the rows.
            Op::Exec if d.rows => self.with_txn(&req.scope, |txn, ctx, _| {
                let (count, values) = match req.inputs.split_last() {
                    Some((c, v)) => (usize::try_from(c.as_i64().unwrap_or(0)).unwrap_or(0), v),
                    None => (0, &[][..]),
                };
                if count == 0 || values.len() % count != 0 {
                    return Reply::of(code(-302, "22003"));
                }
                let width = values.len() / count;
                let mut done = 0;
                for row in values.chunks(width) {
                    let r = exec_text(txn, ctx, &d.text, naming, row);
                    if r.sqlca.is_error() {
                        let mut r = r;
                        r.sqlca.sqlerrd[2] = done;
                        return r;
                    }
                    done += r.sqlca.sqlerrd[2];
                }
                let mut ca = Sqlca::rows(u64::try_from(done).unwrap_or(0));
                ca.sqlerrd[2] = done;
                Reply::of(ca)
            }),
            Op::Exec => self.with_txn(&req.scope, |txn, ctx, _| exec_text(txn, ctx, &d.text, naming, &req.inputs)),
            Op::ExecImm => {
                let text = Self::dynamic_text(req);
                let inputs: &[Value] = if req.inputs.is_empty() { &[] } else { &req.inputs[1..] };
                self.with_txn(&req.scope, |txn, ctx, _| {
                    match crate::exec::dynamic::prepare_dynamic(&text, naming) {
                        Ok(p) if matches!(p.statement, Statement::Select(_)) => return Reply::of(code(-84, "42612")),
                        Err(e) => return Reply::of(Sqlca::from_error(&e)),
                        Ok(_) => {}
                    }
                    exec_text(txn, ctx, &text, naming, inputs)
                })
            }
            Op::Prepare => {
                let text = Self::dynamic_text(req);
                self.with_txn(&req.scope, |txn, ctx, sc| {
                    sc.prepared.remove(&d.name);
                    let p = match crate::exec::dynamic::prepare_dynamic(&text, naming) {
                        Ok(p) => p,
                        Err(e) => return Reply::of(Sqlca::from_error(&e)),
                    };
                    if let Statement::Call(c) = &p.statement {
                        ctx.naming = naming;
                        if let Err(e) = crate::exec::routine::check_prepare(txn, ctx, c) {
                            return Reply::of(Sqlca::from_error(&e));
                        }
                    }
                    // A query's tables are read: checked as they are prepared.
                    let query = matches!(p.statement, Statement::Select(_) | Statement::DeclareCursor(_));
                    for t in tables(&p.statement) {
                        ctx.naming = naming;
                        match ctx.resolve_file(txn, &t) {
                            Err(e) if !crate::exec::catalog::is_view(&t) => return Reply::of(Sqlca::from_error(&e)),
                            Ok(id) if query => {
                                if let Err(e) = db::auth::check_open(txn, id, db::Usage::INPUT) {
                                    return Reply::of(Sqlca::from_error(&SqlError::from(e)));
                                }
                            }
                            _ => {}
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
                    "DECLARE {} {}{}CURSOR {}FOR {query}",
                    d.name,
                    if d.sensitive { "SENSITIVE " } else { "" },
                    if d.scroll { "SCROLL " } else { "" },
                    if d.hold { "WITH HOLD " } else { "" }
                );
                let r = exec_text(txn, ctx, &decl, naming, &[]);
                if r.sqlca.is_error() {
                    return r;
                }
                let r = exec_text(txn, ctx, &format!("OPEN {}", d.name), naming, &req.inputs);
                if let Some(c) = ctx.cursors.get_mut(&d.name) {
                    c.returns = d.returns;
                }
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

    /// `DESCRIBE [INPUT|OUTPUT]` of a statement the scope prepared, or
    /// `DESCRIBE TABLE` of the table the first input names. `orient` is
    /// the form, then the `USING` choice the program applies.
    fn describe(&mut self, req: &Request) -> Reply {
        let d = &req.desc;
        let form = d.orient.split_whitespace().next().unwrap_or("");
        if form == "CURSOR" {
            // The columns of an open cursor: a procedure's result set.
            return match self.scopes.get(&req.scope).and_then(|sc| sc.cursors.get(&d.name)).filter(|c| c.open) {
                Some(c) => Reply { sqlca: code(0, "00000"), rows: describe::cursor_rows(&c.columns), call: None },
                None => Reply::of(code(-501, "24501")),
            };
        }
        if form == "TABLE" {
            let text = req.inputs.first().map(|v| v.as_text().trim().to_string()).unwrap_or_default();
            return self.with_txn(&req.scope, |txn, ctx, _| {
                ctx.naming = d.naming();
                let name = match crate::parse::parse_object_name(&text, d.naming()) {
                    Ok(n) => n,
                    Err(e) => return Reply::of(Sqlca::from_error(&e)),
                };
                match describe::table_entries(txn, ctx, &name) {
                    Ok(es) => Reply { sqlca: code(0, "00000"), rows: describe::rows(es), call: None },
                    Err(e) => Reply::of(Sqlca::from_error(&e)),
                }
            });
        }
        self.with_txn(&req.scope, |txn, ctx, sc| {
            let Some(text) = sc.prepared.get(&d.name).cloned() else {
                return Reply::of(code(-516, "26501"));
            };
            describe::reply(txn, ctx, &text, d.naming(), form == "INPUT")
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

    /// A procedure returned ([`Op::Called`]): the cursors it left open
    /// `WITH RETURN` become the caller's result sets, `RS#1` on, in the
    /// order they were opened (a nested call leaves those `TO CLIENT`
    /// for the outermost); an SQL procedure's other cursors close. The
    /// reply is the `CALL`'s outcome: the procedure's error or warning,
    /// else `SQL0466` with the sets, one row each (its name, the
    /// procedure's cursor, its column count, the procedure);
    /// `SQLERRD(1)` is the return status.
    fn called(&mut self, req: &Request) -> Reply {
        if req.desc.name == crate::exec::routine::midcall::ANSWER {
            // Only the storage service has statements waiting for one.
            return Reply::of(code(-84, "42612"));
        }
        let Some(c) = crate::exec::routine::Called::from_inputs(&req.inputs) else {
            return Reply::of(code(-84, "42612"));
        };
        let mut sets: Vec<(String, Cursor)> = Vec::new();
        for sc in self.scopes.values_mut() {
            let names: Vec<String> = sc
                .cursors
                .iter()
                .filter(|(_, k)| k.open && k.returns.is_some() && k.seq > c.seq)
                .filter(|(_, k)| !(c.nested && k.returns == Some(crate::ast::ReturnTo::Client)))
                .map(|(n, _)| n.clone())
                .collect();
            for n in names {
                let Some(mut k) = sc.cursors.remove(&n) else { continue };
                if c.sqlcode < 0 {
                    // A failed procedure returns no result sets.
                    continue;
                }
                k.returns = None;
                sets.push((n, k));
            }
        }
        if !c.scope.is_empty()
            && let Some(sc) = self.scopes.get_mut(&c.scope)
        {
            for k in sc.cursors.values_mut().filter(|k| k.returns.is_none()) {
                k.open = false;
                k.rows.clear();
            }
        }
        sets.sort_by_key(|(_, k)| k.seq);
        let mut sqlca = Sqlca::default();
        if usize::from(c.max) < sets.len() {
            sets.truncate(c.max.into());
            sqlca = code(464, "0100E");
        }
        let target = self.scopes.entry(req.scope.clone()).or_default();
        target.cursors.retain(|n, _| !n.starts_with("RS#"));
        let mut rows = Vec::new();
        for (i, (n, k)) in sets.into_iter().enumerate() {
            let name = format!("RS#{}", i + 1);
            rows.push(vec![
                Value::Str(name.clone()),
                Value::Str(n),
                Value::int(i64::try_from(k.columns.len()).unwrap_or(0)),
                Value::Str(c.routine.clone()),
            ]);
            target.cursors.insert(name, k);
        }
        if c.sqlcode < 0 {
            sqlca = Sqlca { sqlcode: c.sqlcode, sqlstate: c.sqlstate, sqlerrmc: c.message, ..Sqlca::default() };
        } else if c.sqlcode > 0 && c.sqlcode != 100 {
            sqlca = Sqlca { sqlcode: c.sqlcode, sqlstate: c.sqlstate, sqlerrmc: c.message, ..Sqlca::default() };
            sqlca.sqlwarn[0] = 'W';
        } else if !rows.is_empty() && sqlca.sqlcode == 0 {
            sqlca = code(466, "0100C");
            sqlca.sqlerrmc = format!("{} result sets", rows.len());
        }
        sqlca.sqlerrd[0] = c.status;
        sqlca.sqlerrd[1] = i32::try_from(rows.len()).unwrap_or(0);
        Reply { sqlca, rows, call: None }
    }

    /// A result set a `CALL` left in `scope` (`RS#1`, ...), taken whole:
    /// its columns and the rows not fetched yet. A server streams it so
    /// (DRDA's `OPNQRYRM` per set); embedded SQL fetches it as a cursor of
    /// that name instead.
    pub fn take_result_set(&mut self, scope: &str, name: &str) -> Option<crate::exec::ResultSet> {
        let c = self.scopes.get_mut(scope)?.cursors.remove(name)?;
        let rows = c.rows.into_iter().skip(c.pos).collect();
        Some(crate::exec::ResultSet { columns: c.columns, rows })
    }

    /// A prepared statement goes: -514 when the scope has none of
    /// that name.
    fn deallocate(&mut self, scope: &str, name: &str) -> Reply {
        match self.scopes.get_mut(scope).and_then(|sc| sc.prepared.remove(name)) {
            Some(_) => Reply::default(),
            None => Reply::of(code(-514, "26501")),
        }
    }

    /// `GET DIAGNOSTICS`: the items of the last statement.
    fn diagnostics(&mut self, items: &str) -> Reply {
        // A handler starts: its condition is stacked.
        if items == "*STACK" {
            self.stacked = self.last.clone();
            return Reply::default();
        }
        // It ends: no condition is stacked.
        if items == "*UNSTACK" {
            self.stacked = Sqlca::default();
            return Reply::default();
        }
        let (ca, items) = match items.strip_prefix("*STACKED,") {
            Some(rest) => (&self.stacked, rest),
            None => (&self.last, items),
        };
        let row = items
            .split(',')
            .map(|i| match i.trim() {
                "ROW_COUNT" | "DB2_ROW_COUNT" => Value::int(i64::from(ca.sqlerrd[2])),
                "NUMBER" => Value::int(1),
                "DB2_RETURNED_SQLCODE" => Value::int(i64::from(ca.sqlcode)),
                "RETURNED_SQLSTATE" => Value::Str(ca.sqlstate.clone()),
                "DB2_MESSAGE_ID" => Value::Str(if ca.sqlcode == 0 { String::new() } else { ca.msgid() }),
                "MESSAGE_TEXT" => Value::Str(ca.sqlerrmc.clone()),
                "MESSAGE_LENGTH" => Value::int(i64::try_from(ca.sqlerrmc.len()).unwrap_or(0)),
                "MORE" => Value::Str("N".into()),
                "RETURN_STATUS" | "DB2_RETURN_STATUS" => Value::int(i64::from(ca.sqlerrd[0])),
                "DB2_NUMBER_RESULT_SETS" => Value::int(i64::from(if ca.sqlcode == 466 { ca.sqlerrd[1] } else { 0 })),
                "DB2_SQLERRD1" | "DB2_SQLERRD2" | "DB2_SQLERRD3" | "DB2_SQLERRD4" | "DB2_SQLERRD5" | "DB2_SQLERRD6" => {
                    let i = usize::from(i.trim().as_bytes()[11] - b'1');
                    Value::int(i64::from(ca.sqlerrd[i]))
                }
                "COMMAND_FUNCTION" => Value::Str(self.last_cmd.clone()),
                "DYNAMIC_FUNCTION" => Value::Str(String::new()),
                _ => Value::Null,
            })
            .collect();
        Reply {
            sqlca: Sqlca::default(),
            rows: vec![row],
            call: None,
        }
    }
}

/// Whether a statement may take turns at its safe points
/// (`SqlContext::yielder`): a query, a cursor's open or a statement run
/// (`INSERT`, `UPDATE`, `DELETE`; any other runs to its end), at any
/// level: under `*ALL` the records read are read-locked as they are
/// read, so its view holds across the turns.
#[must_use]
pub fn may_yield(desc: &Desc) -> bool {
    matches!(desc.op, Op::Query | Op::Open | Op::Exec | Op::ExecImm | Op::Execute)
}

impl Embedded for Session {
    fn run(&mut self, req: &Request) -> Reply {
        let watched = crate::dbmon::begin(&mut self.ctx);
        let started = (crate::dbmon::record::now(&self.ctx), std::time::Instant::now());
        // A remote server's statement: only the monitor hears of it.
        if req.desc.op == Op::Routed {
            let r = Reply::of(routed_sqlca(&req.inputs));
            if watched {
                self.monitored(req, &r, started);
            }
            return r;
        }
        let r = self.run_watched(req);
        if watched {
            self.monitored(req, &r, started);
        }
        r
    }
}

impl Session {
    fn run_watched(&mut self, req: &Request) -> Reply {
        let kept = if may_yield(&req.desc) { None } else { self.ctx.yielder.take() };
        self.ctx.routine.can_call = req.desc.calls;
        self.ctx.routine.pending = None;
        self.ctx.arrays = req.desc.arrays;
        let mut r = match self.commitment(req) {
            Some(r) => r,
            None if arrays::has_arrays(&req.desc.locs) => self.with_arrays(req),
            None => self.with_locators(req),
        };
        self.ctx.arrays = false;
        if !r.sqlca.is_error() {
            r.call = self.ctx.routine.pending.take();
            if req.desc.atomic
                && req.desc.commit.is_none()
                && req.desc.text.starts_with("RELEASE")
                && let Some(ca) = self.atomic_released()
            {
                r = Reply::of(ca);
            }
        }
        self.ctx.routine.can_call = false;
        if kept.is_some() {
            self.ctx.yielder = kept;
        }
        // A routine's own evaluation leaves the diagnostics alone, unless
        // it fails: then the handler that takes it sees its condition.
        if req.desc.op != Op::Diag && (!req.desc.quiet || r.sqlca.is_error()) {
            self.last = r.sqlca.clone();
            self.last_cmd = command_function(&req.desc);
        }
        r
    }

    /// Report a watched statement to the database monitor; write its
    /// records when one holds enough.
    fn monitored(&mut self, req: &Request, r: &Reply, (start, clock): (crate::value::Timestamp, std::time::Instant)) {
        let d = &req.desc;
        let prepared = |name: &str| self.scopes.get(&req.scope).and_then(|s| s.prepared.get(name)).cloned().unwrap_or_default();
        let dyn_inputs = if req.inputs.is_empty() { &[][..] } else { &req.inputs[1..] };
        let (op, text, inputs, dynamic): (&'static str, String, &[Value], bool) = match d.op {
            Op::Query | Op::Exec => (crate::dbmon::record::operation(&d.text), d.text.clone(), &req.inputs, false),
            Op::Routed => (crate::dbmon::record::operation(&d.text), d.text.clone(), &[], false),
            Op::ExecImm => ("EI", Self::dynamic_text(req), dyn_inputs, true),
            Op::Prepare => (if d.into { "PD" } else { "PR" }, Self::dynamic_text(req), &[], true),
            Op::Execute => {
                let t = prepared(&d.name);
                (crate::dbmon::record::operation(&t), t, &req.inputs, true)
            }
            Op::Open => {
                let t = if d.stmt.is_empty() { d.text.clone() } else { prepared(&d.stmt) };
                ("OP", t, &req.inputs, !d.stmt.is_empty())
            }
            Op::Fetch => ("FE", format!("FETCH {}", d.name), &[], false),
            Op::Close => ("CL", format!("CLOSE {}", d.name), &[], false),
            Op::Describe => ("DE", format!("DESCRIBE {}", d.name), &[], false),
            Op::Diag | Op::End | Op::Nothing | Op::Deallocate | Op::Called | Op::Package => {
                self.ctx.dbmon.trace = None;
                return;
            }
        };
        let rows = matches!(op, "IN" | "UP" | "DL" | "MG").then(|| i64::from(r.sqlca.sqlerrd[2]));
        let micros = u64::try_from(clock.elapsed().as_micros()).unwrap_or(u64::MAX);
        let end = crate::dbmon::record::now(&self.ctx);
        let o = crate::dbmon::Observed {
            op,
            text: &text,
            cursor: if matches!(d.op, Op::Open | Op::Fetch | Op::Close) { &d.name } else { "" },
            statement: if matches!(d.op, Op::Prepare | Op::Execute) { &d.name } else { &d.stmt },
            start,
            end,
            micros,
            sqlca: &r.sqlca,
            rows,
            inputs,
            dynamic,
        };
        if crate::dbmon::finish(&mut self.ctx, &o) {
            self.flush_monitors(false);
        }
    }

    /// Write what the database monitors hold (all of it with `force`).
    pub fn flush_monitors(&mut self, force: bool) {
        let lender = &self.store;
        if let Ok(mut txn) = store::txn::Txn::lent(Box::new(|| lender.store())) {
            match crate::dbmon::flush(&mut txn, &mut self.ctx, force) {
                Ok(()) => {
                    if let Err(e) = txn.commit() {
                        tracing::warn!("database monitor records not written: {e}");
                    }
                }
                Err(e) => tracing::warn!("database monitor records not written: {e}"),
            }
        }
    }
}

/// The SQLCA of a routed statement's inputs: SQLCODE, SQLSTATE, rows.
fn routed_sqlca(inputs: &[Value]) -> Sqlca {
    let code = inputs.first().and_then(|v| v.as_i64().ok()).unwrap_or(0);
    let mut ca = Sqlca {
        sqlcode: i32::try_from(code).unwrap_or(-1),
        sqlstate: inputs.get(1).map_or_else(|| "00000".into(), |v| v.as_text().trim().to_string()),
        ..Sqlca::default()
    };
    ca.sqlerrd[2] = inputs.get(2).and_then(|v| v.as_i64().ok()).and_then(|n| i32::try_from(n).ok()).unwrap_or(0);
    ca
}

/// The positions `locs` marks with `kind` (`I` inputs, `O` outputs).
fn loc_positions(locs: &str, kind: char) -> Vec<usize> {
    locs.split(',').filter_map(|p| p.strip_prefix(kind)).filter_map(|n| n.parse().ok()).collect()
}

impl Session {
    /// A statement with LOB locators: `FREE LOCATOR` and `HOLD LOCATOR`
    /// on the handles given; input locators stand for their values,
    /// output LOBs come back as new handles (`SQL7034` without
    /// commitment control: a locator lives to the end of the unit of
    /// work).
    fn with_locators(&mut self, req: &Request) -> Reply {
        let d = &req.desc;
        if d.op == Op::Exec {
            let words: Vec<String> = d.text.split_whitespace().take(2).map(str::to_ascii_uppercase).collect();
            if words.len() == 2 && words[1] == "LOCATOR" && matches!(words[0].as_str(), "FREE" | "HOLD") {
                let free = words[0] == "FREE";
                for v in &req.inputs {
                    let h = v.as_i64().ok().and_then(|h| u32::try_from(h).ok()).unwrap_or(0);
                    let r = if free { self.ctx.locators.free(h) } else { self.ctx.locators.hold(h) };
                    if let Err(e) = r {
                        return Reply::of(Sqlca::from_error(&e));
                    }
                }
                return Reply::default();
            }
        }
        if d.locs.is_empty() {
            return self.run_inner(req);
        }
        let ins = loc_positions(&d.locs, 'I');
        let outs = loc_positions(&d.locs, 'O');
        let mut inputs = req.inputs.clone();
        for &k in &ins {
            let Some(v) = inputs.get_mut(k) else { continue };
            let h = v.as_i64().ok().and_then(|h| u32::try_from(h).ok()).unwrap_or(0);
            match self.ctx.locators.get(h) {
                Ok(x) => *v = x.clone(),
                Err(e) => return Reply::of(Sqlca::from_error(&e)),
            }
        }
        if !outs.is_empty() && req.desc.commit.is_none() && self.ctx.isolation.is_none() {
            return Reply::of(code(-7034, "42926"));
        }
        let sub = Request {
            scope: req.scope.clone(),
            desc: req.desc.clone(),
            inputs,
        };
        let mut r = self.run_inner(&sub);
        if !r.sqlca.is_error() {
            for row in &mut r.rows {
                for &k in &outs {
                    if let Some(v) = row.get_mut(k)
                        && !v.is_null()
                    {
                        *v = Value::int(i64::from(self.ctx.locators.assign(std::mem::replace(v, Value::Null))));
                    }
                }
            }
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
        Ok(o) => {
            let ca = Sqlca::from_outcome(&o);
            Reply::of(ctx.warning.as_ref().map_or_else(|| ca.clone(), |w| w.apply(ca.clone())))
        }
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
    Reply { sqlca: ca, rows, call: None }
}

/// A statement's kind as `GET DIAGNOSTICS` names it.
fn command_function(d: &Desc) -> String {
    match d.op {
        Op::Query | Op::Exec => d.text.split_whitespace().next().unwrap_or("").to_ascii_uppercase(),
        Op::Open => "OPEN".into(),
        Op::Fetch => "FETCH".into(),
        Op::Close => "CLOSE".into(),
        Op::Prepare => "PREPARE".into(),
        Op::Execute => "EXECUTE".into(),
        Op::ExecImm => "EXECUTE IMMEDIATE".into(),
        Op::Describe => "DESCRIBE".into(),
        Op::Deallocate => "DEALLOCATE".into(),
        Op::Called => "CALL".into(),
        Op::Diag | Op::End | Op::Nothing | Op::Package | Op::Routed => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_cross_processes() {
        let r = Reply {
            sqlca: code(-811, "21000"),
            rows: vec![vec![Value::int(5), Value::Str("x ".into()), Value::Null]],
            call: Some(Box::new(crate::exec::routine::CallPlan {
                program: "P".into(),
                library: "L".into(),
                parms: vec![crate::exec::routine::PlanParm { mode: 3, desc: 7, extra: 0, value: Value::int(3), marker: Some(1) }],
                ..Default::default()
            })),
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

