// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL functions as a statement runs them. A function is called inside
//! the query that names it, in the database, so its body runs here: a
//! `RETURN` expression, or a compound statement of variables, `SET`,
//! `IF`, `CASE`, loops, `SELECT|VALUES ... INTO`, `SIGNAL` and `RETURN`.
//! Each expression is the routine's text with its variables as markers,
//! run as a query (`pl::rewrite`), as the compiled procedures do.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use store::txn::Txn;

use super::find;
use crate::ast::{
    AssignSource, Compound, CreateRoutine, Ident, ObjectName, PlExpr, PlKind, PlSql, PlStmt, Query, Returns,
    RoutineBody, Statement, TypeSpec,
};
use crate::error::{Result, SqlError};
use crate::exec::SqlContext;
use crate::exec::eval::{Env, cast};
use crate::pl::rewrite::{self, Names};
use crate::value::Value;

/// How deeply functions may call functions.
const MAX_DEPTH: u32 = 64;

thread_local! {
    /// Functions in progress on this thread.
    static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// Definitions and queries parsed, by their text.
    static PARSED: RefCell<HashMap<String, Rc<CreateRoutine>>> = RefCell::new(HashMap::new());
    static QUERIES: RefCell<HashMap<String, Rc<Query>>> = RefCell::new(HashMap::new());
}

fn unsupported(what: &str) -> SqlError {
    SqlError::Unsupported { what: what.into() }
}

/// Whether a function body holds only what [`call`] runs.
///
/// # Errors
///
/// `SQL0084` naming the first statement it does not.
pub fn check_body(s: &PlStmt) -> Result<()> {
    let all = |v: &[PlStmt]| v.iter().try_for_each(check_body);
    match &s.kind {
        PlKind::Compound(c) => {
            if c.atomic {
                return Err(unsupported("BEGIN ATOMIC in a function: savepoints"));
            }
            if !c.cursors.is_empty() || !c.handlers.is_empty() {
                return Err(unsupported("cursors and handlers in an SQL function"));
            }
            all(&c.body)
        }
        PlKind::Set(_) | PlKind::Return(_) | PlKind::Leave(_) | PlKind::Iterate(_) | PlKind::Signal(_) => Ok(()),
        PlKind::If { branches, otherwise } => {
            for (_, b) in branches {
                all(b)?;
            }
            otherwise.as_deref().map_or(Ok(()), all)
        }
        PlKind::Case { whens, otherwise, .. } => {
            for (_, b) in whens {
                all(b)?;
            }
            otherwise.as_deref().map_or(Ok(()), all)
        }
        PlKind::Loop(b) | PlKind::While(_, b) | PlKind::Repeat(b, _) => all(b),
        PlKind::For { body, .. } => all(body),
        PlKind::Sql(q) if !q.into.is_empty() && matches!(q.stmt, Statement::Select(_)) => Ok(()),
        _ => Err(unsupported("that statement in an SQL function: functions read data and compute")),
    }
}

/// Call a user function `name` (possibly `SCHEMA.NAME`) on `args`, or
/// `None` when no function of that name and number of arguments is in
/// the schema or the path. `literal[i]` says argument `i` is a string
/// constant: a `VARCHAR`, which no `CHAR` parameter takes (IBM's promotion).
pub fn call(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &str, args: &[Value], literal: &[bool]) -> Option<Result<Value>> {
    let on = match name.split_once('.') {
        Some((s, n)) => ObjectName::qualified(s, n),
        None => ObjectName::new(name),
    };
    let found = match find::named(txn, ctx, &on, 2) {
        Ok(f) => f,
        Err(e) => return Some(Err(e)),
    };
    // The best fit: each argument scored against its parameter, the
    // highest total wins, the first of equals.
    let score = |f: &find::Found| -> Option<u32> {
        let ps: Vec<_> = f.routine.parms.iter().filter(|p| p.role == 0).collect();
        if ps.len() != args.len() {
            return None;
        }
        let mut total = 0;
        for (i, (p, a)) in ps.iter().zip(args).enumerate() {
            let t = crate::pl::types::parse_type(&p.ty).ok()?;
            let s = fit(a, &t.data_type, literal.get(i).copied().unwrap_or(false));
            if s == 0 {
                return None;
            }
            total += s;
        }
        Some(total)
    };
    let mut best: Option<(u32, find::Found)> = None;
    for f in found {
        if let Some(s) = score(&f)
            && best.as_ref().is_none_or(|(b, _)| s > *b)
        {
            best = Some((s, f));
        }
    }
    let (_, f) = best?;
    if let Ok(d) = parsed(&f.routine.def.text) {
        remember(&d, &f.schema);
    }
    Some(run(txn, ctx, &f.routine.def.text, args))
}

/// Run a function from its definition text.
fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, text: &str, args: &[Value]) -> Result<Value> {
    let def = parsed(text)?;
    if def.options.null_input && args.iter().any(Value::is_null) {
        return Ok(Value::Null);
    }
    let Some(Returns::Scalar(ret, _)) = &def.returns else {
        return Err(unsupported("calling a table function in an expression"));
    };
    let depth = DEPTH.with(std::cell::Cell::get);
    if depth >= MAX_DEPTH {
        return Err(SqlError::TooComplex);
    }
    DEPTH.with(|d| d.set(depth + 1));
    let r = (|| {
        let mut it = Interp { txn, ctx, slots: Vec::new(), frames: Vec::new() };
        let mut frame = Frame { label: Some(def.name.name.value.clone()), vars: Vec::new() };
        for (p, a) in def.parms.iter().zip(args) {
            it.slots.push((p.ty.clone(), cast(a, &p.ty)?));
            frame.vars.push((p.name.as_ref().map(|n| n.value.clone()).unwrap_or_default(), it.slots.len() - 1));
        }
        it.frames.push(frame);
        let body = match &def.body {
            RoutineBody::Sql(b) => b,
            RoutineBody::Sourced(src, _) => {
                let n = src.to_string();
                return crate::exec::func::call(n.rsplit('.').next().unwrap_or(&n), args, it.ctx).and_then(|v| cast(&v, ret));
            }
            RoutineBody::External => return Err(unsupported("external functions")),
        };
        match it.stmt(body)? {
            Flow::Return(v) => cast(&v, ret),
            // A function that ends without RETURN (SQL2F03F5).
            _ => Err(SqlError::Coded { code: -578, state: "2F005", what: format!("function {} ended without a RETURN", def.name) }),
        }
    })();
    DEPTH.with(|d| d.set(depth));
    r
}

fn parsed(text: &str) -> Result<Rc<CreateRoutine>> {
    if let Some(d) = PARSED.with(|p| p.borrow().get(text).cloned()) {
        return Ok(d);
    }
    let Statement::CreateRoutine(r) = crate::parse::parse(text, crate::ast::Naming::Sql)? else {
        return Err(unsupported("a function definition that is not CREATE FUNCTION"));
    };
    let r: Rc<CreateRoutine> = Rc::new(*r);
    PARSED.with(|p| {
        let mut p = p.borrow_mut();
        if p.len() > 256 {
            p.clear();
        }
        p.insert(text.to_string(), Rc::clone(&r));
    });
    Ok(r)
}

fn query_of(text: &str) -> Result<Rc<Query>> {
    if let Some(q) = QUERIES.with(|c| c.borrow().get(text).cloned()) {
        return Ok(q);
    }
    let Statement::Select(q) = crate::parse::parse(text, crate::ast::Naming::Sql)? else {
        return Err(unsupported("a statement that is not a query in a function"));
    };
    let q = Rc::new(q);
    QUERIES.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 1024 {
            c.clear();
        }
        c.insert(text.to_string(), Rc::clone(&q));
    });
    Ok(q)
}

struct Frame {
    label: Option<String>,
    vars: Vec<(String, usize)>,
}

enum Flow {
    Next,
    Return(Value),
    Leave(String),
    Iterate(String),
}

struct Interp<'a, 't> {
    txn: &'a mut Txn<'t>,
    ctx: &'a mut SqlContext,
    slots: Vec<(TypeSpec, Value)>,
    frames: Vec<Frame>,
}

impl Names for Interp<'_, '_> {
    fn variable(&self, name: &str) -> Option<usize> {
        self.frames.iter().rev().find_map(|f| f.vars.iter().rev().find(|(n, _)| n == name).map(|(_, k)| *k))
    }

    fn qualified(&self, label: &str, name: &str) -> Option<usize> {
        self.frames.iter().rev().find(|f| f.label.as_deref() == Some(label)).and_then(|f| f.vars.iter().find(|(n, _)| n == name).map(|(_, k)| *k))
    }
}

impl Interp<'_, '_> {
    fn slot(&self, name: &str) -> Result<usize> {
        let k = match name.split_once('.') {
            Some((l, v)) => self.qualified(l, v),
            None => self.variable(name),
        };
        k.ok_or_else(|| SqlError::Coded { code: -312, state: "42618", what: format!("variable {name} is not declared") })
    }

    /// The rows of a query text, its variables passed as markers.
    fn rows(&mut self, text: &str, columns: &HashSet<String>) -> Result<Vec<Vec<Value>>> {
        let r = rewrite::rewrite(text, self, columns)?;
        let params: Vec<Value> = r.vars.iter().map(|k| self.slots[*k].1.clone()).collect();
        let q = query_of(&r.text)?;
        let rs = crate::exec::query::run_query(self.txn, self.ctx, &params, &q, &Env::default())?;
        Ok(rs.rows)
    }

    fn value(&mut self, e: &PlExpr) -> Result<Value> {
        let rows = self.rows(&format!("VALUES ({})", e.text), &HashSet::new())?;
        Ok(rows.into_iter().next().and_then(|r| r.into_iter().next()).unwrap_or(Value::Null))
    }

    fn truth(&mut self, e: &PlExpr) -> Result<bool> {
        let rows = self.rows(&format!("VALUES CASE WHEN ({}) THEN 1 ELSE 0 END", e.text), &HashSet::new())?;
        Ok(rows.first().and_then(|r| r.first()).is_some_and(|v| v.as_i64().ok() == Some(1)))
    }

    fn assign(&mut self, name: &Ident, v: &Value) -> Result<()> {
        let k = self.slot(&name.value)?;
        let ty = self.slots[k].0.clone();
        self.slots[k].1 = cast(v, &ty)?;
        Ok(())
    }

    fn stmts(&mut self, v: &[PlStmt]) -> Result<Flow> {
        for s in v {
            match self.stmt(s)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    /// A loop's body once: whether the loop goes on, or how it ends.
    fn round(&mut self, label: Option<&Ident>, body: &[PlStmt]) -> Result<Option<Flow>> {
        match self.stmts(body)? {
            Flow::Next => Ok(None),
            Flow::Leave(l) if Some(l.as_str()) == label.map(|x| x.value.as_str()) => Ok(Some(Flow::Next)),
            Flow::Iterate(l) if Some(l.as_str()) == label.map(|x| x.value.as_str()) => Ok(None),
            other => Ok(Some(other)),
        }
    }

    fn stmt(&mut self, s: &PlStmt) -> Result<Flow> {
        self.ctx.poll_interrupt()?;
        let flow = match &s.kind {
            PlKind::Compound(c) => self.compound(c, s.label.as_ref())?,
            PlKind::Set(assigns) => {
                for a in assigns {
                    match &a.source {
                        AssignSource::Exprs(es) => {
                            for (t, e) in a.targets.iter().zip(es) {
                                let v = self.value(e)?;
                                self.assign(t, &v)?;
                            }
                        }
                        AssignSource::Row(q) => {
                            let rows = self.rows(&format!("VALUES ({})", q.text), &HashSet::new())?;
                            let row = rows.into_iter().next().unwrap_or_default();
                            for (t, v) in a.targets.iter().zip(row) {
                                self.assign(t, &v)?;
                            }
                        }
                    }
                }
                Flow::Next
            }
            PlKind::Return(Some(e)) => Flow::Return(self.value(e)?),
            PlKind::Return(None) => Flow::Return(Value::Null),
            PlKind::If { branches, otherwise } => {
                for (c, b) in branches {
                    if self.truth(c)? {
                        return self.stmts(b);
                    }
                }
                match otherwise {
                    Some(o) => self.stmts(o)?,
                    None => Flow::Next,
                }
            }
            PlKind::Case { operand, whens, otherwise } => {
                for (w, b) in whens {
                    let hit = match operand {
                        Some(o) => self.truth(&PlExpr { text: format!("({}) = ({})", o.text, w.text), expr: w.expr.clone() })?,
                        None => self.truth(w)?,
                    };
                    if hit {
                        return self.stmts(b);
                    }
                }
                match otherwise {
                    Some(o) => self.stmts(o)?,
                    None => return Err(SqlError::Coded { code: -773, state: "20000", what: "no WHEN of the CASE statement held".into() }),
                }
            }
            PlKind::Loop(b) => loop {
                if let Some(f) = self.round(s.label.as_ref(), b)? {
                    break f;
                }
            },
            PlKind::While(c, b) => loop {
                if !self.truth(c)? {
                    break Flow::Next;
                }
                if let Some(f) = self.round(s.label.as_ref(), b)? {
                    break f;
                }
            },
            PlKind::Repeat(b, c) => loop {
                if let Some(f) = self.round(s.label.as_ref(), b)? {
                    break f;
                }
                if self.truth(c)? {
                    break Flow::Next;
                }
            },
            PlKind::For { var, query, body, .. } => self.for_loop(var, query, body, s.label.as_ref())?,
            PlKind::Leave(l) => Flow::Leave(l.value.clone()),
            PlKind::Iterate(l) => Flow::Iterate(l.value.clone()),
            PlKind::Signal(sg) => {
                let state = match &sg.state {
                    Some(crate::ast::SignalValue::State(st)) => st.clone(),
                    Some(crate::ast::SignalValue::Variable(e)) => self.value(e)?.as_text().trim_end().to_string(),
                    _ => return Err(unsupported("SIGNAL of a condition name in a function")),
                };
                let text = match &sg.message {
                    Some(m) => self.value(m)?.as_text().trim_end().to_string(),
                    None => format!("SQLSTATE {state} was signalled"),
                };
                return Err(SqlError::Signalled { code: -438, state, what: text.chars().take(70).collect() });
            }
            PlKind::Sql(q) => self.select_into(q)?,
            _ => return Err(unsupported("that statement in an SQL function")),
        };
        // A LEAVE of this statement's own label ends here.
        if let (Flow::Leave(l), Some(me)) = (&flow, &s.label)
            && *l == me.value
        {
            return Ok(Flow::Next);
        }
        Ok(flow)
    }

    fn compound(&mut self, c: &Compound, label: Option<&Ident>) -> Result<Flow> {
        self.frames.push(Frame { label: label.map(|l| l.value.clone()), vars: Vec::new() });
        let r = (|| {
            for v in &c.vars {
                for n in &v.names {
                    let init = match &v.default {
                        Some(d) => cast(&self.value(d)?, &v.ty)?,
                        None => Value::Null,
                    };
                    self.slots.push((v.ty.clone(), init));
                    let k = self.slots.len() - 1;
                    self.frames.last_mut().expect("frame").vars.push((n.value.clone(), k));
                }
            }
            self.stmts(&c.body)
        })();
        self.frames.pop();
        r
    }

    fn table_columns(&mut self, st: &Statement) -> HashSet<String> {
        let mut out = HashSet::new();
        for t in crate::embed::tables(st) {
            if let Ok(info) = self.ctx.resolve_table(self.txn, &t) {
                for f in &info.format.format.fields {
                    out.insert(f.name.to_string());
                    if let Some(a) = &f.alias {
                        out.insert(a.clone());
                    }
                }
            }
        }
        out
    }

    fn select_into(&mut self, q: &PlSql) -> Result<Flow> {
        let cols = self.table_columns(&q.stmt);
        let rows = self.rows(&q.text, &cols)?;
        match rows.len() {
            0 => {}
            1 => {
                for (t, v) in q.into.iter().zip(rows.into_iter().next().unwrap_or_default()) {
                    self.assign(t, &v)?;
                }
            }
            _ => return Err(SqlError::TooManyRows),
        }
        Ok(Flow::Next)
    }

    fn for_loop(&mut self, var: &Ident, q: &PlSql, body: &[PlStmt], label: Option<&Ident>) -> Result<Flow> {
        let cols = self.table_columns(&q.stmt);
        let r = rewrite::rewrite(&q.text, self, &cols)?;
        let params: Vec<Value> = r.vars.iter().map(|k| self.slots[*k].1.clone()).collect();
        let query = query_of(&r.text)?;
        let rs = crate::exec::query::run_query(self.txn, self.ctx, &params, &query, &Env::default())?;
        let mut frame = Frame { label: Some(var.value.clone()), vars: Vec::new() };
        let base = self.slots.len();
        for (i, c) in rs.columns.iter().enumerate() {
            let ty = crate::pl::types::parse_type(&c.data_type).unwrap_or(TypeSpec { data_type: crate::ast::DataType::Varchar(32_740), ccsid: None, bit_data: false });
            self.slots.push((ty, Value::Null));
            frame.vars.push((c.name.clone(), base + i));
        }
        self.frames.push(frame);
        let mut out = Flow::Next;
        for row in rs.rows {
            for (i, v) in row.into_iter().enumerate() {
                self.slots[base + i].1 = v;
            }
            match self.round(label, body) {
                Ok(None) => {}
                Ok(Some(f)) => {
                    out = f;
                    break;
                }
                Err(e) => {
                    self.frames.pop();
                    return Err(e);
                }
            }
        }
        self.frames.pop();
        Ok(out)
    }
}

/// How well a value fits a parameter type: 3 its own type, 2 or 1 by
/// promotion, 0 not at all.
fn fit(v: &Value, t: &crate::ast::DataType, literal: bool) -> u32 {
    use crate::ast::DataType as D;
    match (v, t) {
        (Value::Num(d), D::SmallInt | D::Integer | D::BigInt) => u32::from(d.scale() == 0) * 3,
        (Value::Num(d), D::Decimal(..) | D::Numeric(..)) => if d.scale() == 0 { 2 } else { 3 },
        (Value::Str(_), D::Char(_)) => if literal { 0 } else { 3 },
        (Value::Null, _)
        | (Value::Num(_), D::Real | D::Double | D::DecFloat(_))
        | (Value::Double(_) | Value::Real(_), D::DecFloat(_))
        | (Value::Str(_), D::Graphic(_) | D::Vargraphic(_) | D::Dbclob(_))
        | (Value::Date(_) | Value::Time(_) | Value::Timestamp(_), D::Char(_) | D::Varchar(_)) => 1,
        (Value::Double(_) | Value::Real(_), D::Real | D::Double)
        | (Value::DecFloat(_), D::DecFloat(_))
        | (Value::Str(_), D::Varchar(_) | D::Clob(_))
        | (Value::Date(_), D::Date)
        | (Value::Time(_), D::Time)
        | (Value::Timestamp(_), D::Timestamp(_))
        | (Value::Bin(_), D::Binary(_) | D::Varbinary(_) | D::Blob(_))
        | (Value::Bool(_), D::Boolean) => 3,
        _ => 0,
    }
}

thread_local! {
    /// The result types of the functions this process has made or run,
    /// by name and by `SCHEMA.NAME`: number of parameters and type.
    static RESULTS: RefCell<HashMap<String, Vec<(usize, TypeSpec)>>> = RefCell::new(HashMap::new());
}

/// Keep a function's result type for the type rules (`restype`).
pub fn remember(def: &CreateRoutine, schema: &str) {
    let Some(Returns::Scalar(t, _)) = &def.returns else { return };
    let n = def.parms.len();
    let name = &def.name.name.value;
    RESULTS.with(|r| {
        let mut r = r.borrow_mut();
        for key in [name.clone(), format!("{schema}.{name}")] {
            let e = r.entry(key).or_default();
            e.retain(|(k, _)| *k != n);
            e.push((n, t.clone()));
        }
    });
}

/// Forget a dropped function's result type.
pub fn forget(name: &str, schema: &str, parms: usize) {
    RESULTS.with(|r| {
        let mut r = r.borrow_mut();
        for key in [name.to_string(), format!("{schema}.{name}")] {
            if let Some(e) = r.get_mut(&key) {
                e.retain(|(k, _)| *k != parms);
            }
        }
    });
}

/// A user function's result type, when this process knows it (a
/// `restype` function family, asked after the built-ins).
pub fn result_type(name: &str, args: &[Option<crate::exec::restype::SqlType>], _: &[crate::ast::Expr]) -> Option<crate::exec::restype::SqlType> {
    RESULTS.with(|r| {
        r.borrow()
            .get(name)
            .and_then(|e| e.iter().find(|(k, _)| *k == args.len()).map(|(_, t)| crate::exec::restype::SqlType::of_data_type(&t.data_type, true)))
    })
}
