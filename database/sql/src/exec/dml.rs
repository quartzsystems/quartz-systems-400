// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSERT`, `UPDATE`, `DELETE` and `MERGE` through the open data path, so the
//! journal sees what it would from RPG. Under a yielder a change takes
//! turns between rows: what it wrote is committed to the store first
//! (`SqlContext::change_point`), and once others have run each row's
//! search condition is tested again as the row is now. A condition
//! with a subquery is decided as of the statement's start: read first,
//! its subqueries' answers kept, then written (`Recheck`); so is a
//! `MERGE`, its matches and actions chosen before it writes.

use std::rc::Rc;

use db::member::lob::LobRef;
use db::odp::Odp;
use db::odp::Usage;
use db::record;
use db::{KeyArg, Value as DbValue};
use dds::{DefaultValue, FieldDef};
use store::txn::Txn;

use super::answers::Answers;
use super::eval::{Env, Evaluator, Row, Schema};
use super::query::{has_subquery, matching_rows, row_of, run_query};
use super::{SqlContext, TableInfo};
use crate::ast::{Assignment, Delete, Expr, Insert, InsertSource, Update, Where};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// A column's default: a value, or an expression to evaluate.
enum Dft {
    Value(Value),
    Expr(String),
}

/// A column's default value, and whether it is the null value.
fn default_of(f: &FieldDef) -> Result<Option<Dft>> {
    Ok(match &f.default {
        Some(DefaultValue::Null) => Some(Dft::Value(Value::Null)),
        Some(DefaultValue::Quoted(q)) => Some(Dft::Value(Value::Str(q.clone()))),
        Some(DefaultValue::Hex(h)) => Some(Dft::Value(Value::Str(ccsid::from_ebcdic(h)))),
        // A DECFLOAT default may be past what a decimal holds: its text.
        Some(DefaultValue::Number(n)) if f.data_type == dds::DataType::DecFloat => {
            Some(Dft::Value(Value::Str(n.clone())))
        }
        Some(DefaultValue::Number(n)) => Some(Dft::Value(Value::Num(
            qdecimal::Decimal::parse(n).map_err(|e| SqlError::Conversion {
                what: e.to_string(),
            })?,
        ))),
        Some(DefaultValue::Expr(e)) => Some(Dft::Expr(e.clone())),
        None => None,
    })
}

/// The default of `f` as a value, an expression evaluated now.
fn default_value(txn: &mut Txn<'_>, ev: &mut Evaluator<'_>, f: &FieldDef) -> Result<Option<Value>> {
    Ok(match default_of(f)? {
        Some(Dft::Value(v)) => Some(v),
        Some(Dft::Expr(text)) => {
            let e = crate::parse::parse_expr(&text, ev.ctx.naming)?;
            Some(ev.eval(txn, &e, &Env::default())?)
        }
        None => None,
    })
}

/// Fill what the system generates in a row about to be inserted: the
/// identity column's next value, row change timestamps and auditing
/// columns, and expression defaults. `overriding` is the statement's
/// `OVERRIDING SYSTEM VALUE` (true) or `USER VALUE` (false).
#[allow(clippy::too_many_arguments)]
fn fill_generated(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableInfo,
    o: &mut Odp,
    gn: &super::column::Generation,
    overriding: Option<bool>,
    per_field: &mut [Option<Value>],
) -> Result<()> {
    for (fi, f) in t.format.format.fields.iter().enumerate() {
        if let Some(id) = &f.identity {
            if per_field[fi].is_some() {
                if overriding == Some(false) {
                    // The value given is not used.
                    per_field[fi] = None;
                } else if id.always && overriding != Some(true) {
                    return Err(SqlError::IdentityValue {
                        column: f.name.to_string(),
                    });
                } else {
                    continue;
                }
            }
            let v = o.next_identity(txn)?.unwrap_or(id.start);
            per_field[fi] = Some(Value::int(v));
            ctx.seq.identity = per_field[fi].clone();
        } else if per_field[fi].is_none() && matches!(f.default, Some(DefaultValue::Expr(_))) {
            let mut ev = Evaluator { ctx, params };
            per_field[fi] = default_value(txn, &mut ev, f)?;
        }
    }
    gn.fill(ctx, &t.format.format.fields, per_field, true, overriding == Some(true))
}

/// Write `values` (one per field, `None` for the default) into a record.
/// A LOB's value goes to the LOB tree of the member the record is in.
pub fn build_record(
    txn: &mut Txn<'_>,
    t: &TableInfo,
    values: &[Option<Value>],
    base: Option<(&[u8], &[u8])>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let fmt = &t.format;
    let mut lobs = None;
    let (mut rec, mut nulls) = match base {
        Some((r, n)) => (r.to_vec(), n.to_vec()),
        None => (
            vec![0u8; usize::try_from(fmt.format.record_len).expect("fits")],
            vec![0u8; usize::from(fmt.null_map_len)],
        ),
    };
    for (i, f) in fmt.format.fields.iter().enumerate() {
        let v = match (&values[i], base) {
            (Some(v), _) => v.clone(),
            (None, Some(_)) => continue,
            (None, None) => match default_of(f)? {
                Some(Dft::Value(d)) => d,
                Some(Dft::Expr(_)) => {
                    return Err(SqlError::Database {
                        what: format!("default of {} was not evaluated", f.name),
                        msgid: None,
                    });
                }
                None if f.allow_null => Value::Null,
                None => {
                    return Err(SqlError::NullNotAllowed {
                        column: f.name.to_string(),
                    });
                }
            },
        };
        if v.is_null() && !f.allow_null {
            return Err(SqlError::NullNotAllowed {
                column: f.name.to_string(),
            });
        }
        let dbv = if f.data_type.is_lob() && !v.is_null() {
            let owner = match lobs {
                Some(o) => o,
                None => *lobs.insert(t.lob_owner(txn)?),
            };
            DbValue::Char(db::member::lob::put(txn, owner, &v.lob_bytes(f)?)?)
        } else {
            v.to_field(f)?
        };
        record::write_field(f, &mut rec, &dbv).map_err(|e| SqlError::Conversion {
            what: e.to_string(),
        })?;
        if let Some(ni) = fmt.null_index(i) {
            record::set_null(&mut nulls, ni, matches!(dbv, DbValue::Null));
        }
    }
    // A view made WITH CHECK OPTION takes only rows it would show.
    if let Some(view) = super::view::check_option(txn, t)
        && let Some(log) = &fmt.logical
        && !db::lf::selected(fmt, log, &rec, &nulls)?
    {
        return Err(SqlError::CheckOption { view });
    }
    Ok((rec, nulls))
}

/// Free the LOB values an update replaced or a delete left: only in a
/// table that is not journaled (so under no commitment control), where
/// no rollback or journal entry can want them back; a failed statement
/// undoes the free with its transaction. A journaled table's are freed
/// by `RGZPFM`.
fn release_lobs(txn: &mut Txn<'_>, t: &TableInfo, old: &[u8], new: Option<&[u8]>) -> Result<()> {
    if !t.has_lobs() || t.desc.attr.journal != store::obj::ObjectId::NONE {
        return Ok(());
    }
    let mut owner = None;
    for f in t.format.format.fields.iter().filter(|f| f.data_type.is_lob()) {
        let was = LobRef::decode(record::field_bytes(f, old))?;
        if !matches!(was, LobRef::Tree { .. }) || new.is_some_and(|n| record::field_bytes(f, n) == record::field_bytes(f, old)) {
            continue;
        }
        let o = match owner {
            Some(o) => o,
            None => *owner.insert(t.lob_owner(txn)?),
        };
        db::member::lob::free(txn, o, &was)?;
    }
    Ok(())
}

/// The field index of each named column, or every field when none.
fn column_indexes(t: &TableInfo, columns: &[crate::ast::Ident]) -> Result<Vec<usize>> {
    if columns.is_empty() {
        return Ok((0..t.format.format.fields.len()).collect());
    }
    columns
        .iter()
        .map(|c| {
            t.field(&c.value)
                .map(|(i, _)| i)
                .ok_or_else(|| SqlError::ColumnNotFound {
                    name: c.value.clone(),
                })
        })
        .collect()
}

/// `INSERT`: rows inserted.
pub fn insert(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    ins: &Insert,
) -> Result<u64> {
    let t = ctx.resolve_table(txn, &ins.table)?;
    let idx = column_indexes(&t, &ins.columns)?;
    let mut rows: Vec<Vec<Option<Value>>> = Vec::new();
    match &ins.source {
        InsertSource::DefaultValues => rows.push(Vec::new()),
        InsertSource::Values(vals) => {
            let mut ev = Evaluator { ctx, params };
            let env = Env::default();
            for r in vals {
                if r.len() != idx.len() {
                    return Err(SqlError::ValueCount {
                        what: format!("{} values for {} columns", r.len(), idx.len()),
                    });
                }
                let mut row = Vec::with_capacity(r.len());
                for (e, &fi) in r.iter().zip(&idx) {
                    row.push(match e {
                        Expr::Default => None,
                        e => Some(eval_for(txn, &mut ev, e, &env, &t.format.format.fields[fi])?),
                    });
                }
                rows.push(row);
            }
        }
        InsertSource::Query(q) => {
            let rs = run_query(txn, ctx, params, q, &Env::default())?;
            for r in rs.rows {
                if r.len() != idx.len() {
                    return Err(SqlError::ValueCount {
                        what: format!("{} columns selected for {} columns", r.len(), idx.len()),
                    });
                }
                rows.push(r.into_iter().map(Some).collect());
            }
        }
    }
    write_rows(txn, ctx, params, &t, &idx, ins.overriding, rows)
}

/// Write rows of values for the fields `idx` into `t`, `None` for a
/// field's default: rows written.
pub(crate) fn write_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableInfo,
    idx: &[usize],
    overriding: Option<bool>,
    rows: Vec<Vec<Option<Value>>>,
) -> Result<u64> {
    let t = t.clone();
    let gn = super::column::Generation::of(txn, &t)?;
    let mut o = ctx.open(txn, &t, Usage::OUTPUT)?;
    let mut n = 0u64;
    let mut result = Ok(());
    ctx.hold_open();
    for r in rows {
        if let Err(e) = ctx.poll_interrupt().and_then(|()| ctx.change_point(txn)) {
            result = Err(e);
            break;
        }
        let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
        for (k, &fi) in idx.iter().enumerate() {
            per_field[fi] = r.get(k).cloned().flatten();
        }
        let written = fill_generated(txn, ctx, params, &t, &mut o, &gn, overriding, &mut per_field)
            .and_then(|()| build_record(txn, &t, &per_field, None))
            .and_then(|(rec, nulls)| {
                o.write(txn, &rec, &nulls)?;
                Ok((rec, nulls))
            });
        match written {
            Err(e) => {
                result = Err(e);
                break;
            }
            Ok((rec, nulls)) => {
                if let Some(c) = ctx.captured.as_mut() {
                    c.push((0, rec, nulls));
                }
            }
        }
        n += 1;
    }
    ctx.release_open();
    o.close(txn);
    result.map(|()| n)
}

/// The value of `e` for field `f`: into a `DECFLOAT`, an exponent
/// literal keeps the exponent written.
fn eval_for(txn: &mut Txn<'_>, ev: &mut Evaluator<'_>, e: &Expr, env: &Env, f: &FieldDef) -> Result<Value> {
    if f.data_type == dds::DataType::DecFloat { ev.eval_decfloat(txn, e, env) } else { ev.eval(txn, e, env) }
}

/// Evaluate the `SET` assignments against a row: field index to value.
fn assignments(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    t: &TableInfo,
    set: &[Assignment],
    env: &Env,
) -> Result<Vec<(usize, Option<Value>)>> {
    let mut out = Vec::new();
    for a in set {
        let values: Vec<Option<Value>> = if a.values.len() == 1 && a.columns.len() > 1 {
            match ev.eval(txn, &a.values[0], env)? {
                v if a.columns.len() == 1 => vec![Some(v)],
                _ => {
                    return Err(SqlError::Unsupported {
                        what: "a row subquery in SET".into(),
                    });
                }
            }
        } else {
            let mut v = Vec::with_capacity(a.values.len());
            for (k, e) in a.values.iter().enumerate() {
                let field = a.columns.get(k).and_then(|c| t.field(&c.value)).map(|(_, f)| f);
                v.push(match (e, field) {
                    (Expr::Default, _) => None,
                    (e, Some(f)) => Some(eval_for(txn, ev, e, env, f)?),
                    (e, None) => Some(ev.eval(txn, e, env)?),
                });
            }
            v
        };
        for (c, v) in a.columns.iter().zip(values) {
            let (i, f) = t.field(&c.value).ok_or_else(|| SqlError::ColumnNotFound {
                name: c.value.clone(),
            })?;
            let v = match v {
                Some(v) => Some(v),
                None if f.identity.is_some() => {
                    return Err(SqlError::Unsupported {
                        what: "SET DEFAULT on an identity column".into(),
                    });
                }
                None => Some(match default_value(txn, ev, f)? {
                    Some(d) => d,
                    None if f.allow_null => Value::Null,
                    None => {
                        return Err(SqlError::NullNotAllowed {
                            column: f.name.to_string(),
                        });
                    }
                }),
            };
            out.push((i, v));
        }
    }
    Ok(out)
}

/// `UPDATE`: rows changed.
pub fn update(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    u: &Update,
) -> Result<u64> {
    change(txn, ctx, params, &u.table, u.alias.as_ref(), &u.where_clause, Some(&u.set))
}

/// `UPDATE` (with `set`) or `DELETE`: the rows that match, then each
/// changed through the path. A condition with a subquery is decided as
/// of the statement's start: its subqueries' answers are kept while the
/// rows are read ([`super::answers`]), before anything is written.
fn change(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    table: &crate::ast::ObjectName,
    alias: Option<&crate::ast::Ident>,
    w: &Where,
    set: Option<&[Assignment]>,
) -> Result<u64> {
    let t = ctx.resolve_table(txn, table)?;
    let alias = alias.map_or_else(|| table.name.value.clone(), |a| a.value.clone());
    let mark = ctx.yields();
    if matches!(w, Where::Cond(c) if has_subquery(c)) {
        ctx.answers = Some(Answers::recording());
    }
    let read = match w {
        Where::All => matching_rows(txn, ctx, params, &t, &alias, None, &Env::default()),
        Where::Cond(c) => matching_rows(txn, ctx, params, &t, &alias, Some(c), &Env::default()),
        Where::CurrentOf(c) => current_row(txn, ctx, &t, &alias, &c.value),
    };
    let answers = ctx.answers.take();
    let (schema, rows) = read?;
    let cond = match w {
        Where::Cond(c) => Some(Recheck { cond: c, mark, answers }),
        _ => None,
    };
    let mut o = ctx.open(txn, &t, Usage::UPDATE)?;
    ctx.hold_open();
    let result = change_rows(txn, ctx, params, &t, &schema, rows, &mut o, set, cond);
    ctx.release_open();
    o.close(txn);
    result
}

/// How a change tests its condition again on a row it locked once
/// others have run (since `mark`, a yield count). Without `answers` it
/// is tested as the row is now. With them (a subquery's, kept as of the
/// start) only a row another job changed meanwhile is tested, its
/// subqueries answered as kept; one with no answer kept (it reads the
/// row's columns, or was never reached) leaves the first decision
/// standing.
struct Recheck<'a> {
    cond: &'a Expr,
    mark: u64,
    answers: Option<Answers>,
}

/// `DELETE`: rows deleted.
pub fn delete(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    d: &Delete,
) -> Result<u64> {
    change(txn, ctx, params, &d.table, d.alias.as_ref(), &d.where_clause, None)
}

/// The row a cursor stands on, for `WHERE CURRENT OF`.
fn current_row(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    t: &TableInfo,
    alias: &str,
    cursor: &str,
) -> Result<(Rc<Schema>, Vec<Row>)> {
    let c = ctx
        .cursors
        .get(cursor)
        .ok_or_else(|| SqlError::CursorState {
            cursor: cursor.into(),
            what: "is not declared".into(),
        })?;
    let (table, rrn) = c.current(cursor)?;
    if table.id != t.id {
        return Err(SqlError::CursorState {
            cursor: cursor.into(),
            what: format!("reads {}, not {}", table.desc.name(), t.desc.name()),
        });
    }
    let schema = Rc::new(super::query::table_schema(t, alias));
    let m = db::Member::open(txn, &t.desc, &t.member)?;
    let rows = match m.read(txn, rrn)? {
        Some(s) if !s.deleted => vec![row_of(txn, t, &s)?],
        _ => Vec::new(),
    };
    Ok((schema, rows))
}

/// Update (with `set`) or delete each row by RRN through the path,
/// testing the condition again as [`Recheck`] says.
#[allow(clippy::too_many_arguments)]
fn change_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableInfo,
    schema: &Rc<Schema>,
    rows: Vec<Row>,
    o: &mut db::Odp,
    set: Option<&[Assignment]>,
    mut cond: Option<Recheck<'_>>,
) -> Result<u64> {
    let gn = if set.is_some() { super::column::Generation::of(txn, t)? } else { super::column::Generation::default() };
    let mut ev = Evaluator { ctx, params };
    let mut n = 0u64;
    for r in rows {
        ev.ctx.poll_interrupt()?;
        ev.ctx.change_point(txn)?;
        let rrn = u64::try_from(r[0].as_i64()?).expect("positive");
        let Some(current) = chain_waiting(txn, ev.ctx, t, o, rrn)? else {
            continue;
        };
        let slot = db::Slot {
            rrn,
            record: current.data.clone(),
            null_map: current.null_map.clone(),
            deleted: false,
        };
        let stale = cond.as_ref().is_some_and(|c| ev.ctx.yields() != c.mark);
        let now = if set.is_some() || stale { Some(row_of(txn, t, &slot)?) } else { None };
        if let (Some(c), Some(row)) = (cond.as_mut().filter(|_| stale), &now)
            && !still_holds(txn, &mut ev, c, schema, &r, row)?
        {
            // Changed by another job: no longer one to change.
            o.unlock();
            continue;
        }
        // OLD TABLE: the row as it was.
        if ev.ctx.capture_old
            && let Some(c) = ev.ctx.captured.as_mut()
        {
            c.push((rrn, current.data.clone(), current.null_map.clone()));
        }
        match set {
            None => {
                o.delete(txn)?;
                release_lobs(txn, t, &current.data, None)?;
            }
            Some(set) => {
                // Evaluate against the record as it is now.
                let row = now.expect("read for the assignments");
                let env = Env::default().with(schema.clone(), row);
                let assigned = assignments(txn, &mut ev, t, set, &env)?;
                let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
                for (i, v) in assigned {
                    per_field[i] = v;
                }
                for (i, f) in t.format.format.fields.iter().enumerate() {
                    if per_field[i].is_some() && f.identity.as_ref().is_some_and(|id| id.always) {
                        return Err(SqlError::IdentityValue { column: f.name.to_string() });
                    }
                }
                gn.fill(ev.ctx, &t.format.format.fields, &mut per_field, false, false)?;
                let (rec, nulls) =
                    build_record(txn, t, &per_field, Some((&current.data, &current.null_map)))?;
                o.update(txn, &rec, &nulls)?;
                release_lobs(txn, t, &current.data, Some(&rec))?;
                if !ev.ctx.capture_old
                    && let Some(c) = ev.ctx.captured.as_mut()
                {
                    c.push((rrn, rec, nulls));
                }
            }
        }
        n += 1;
    }
    Ok(n)
}

/// The record at `rrn` read and locked for a change. One another job
/// holds is waited for up to the file's `WAITRCD`, taking turns so the
/// holder can end its unit of work; past it (or with no turns to take)
/// the lock error stands (`SQL0913`).
fn chain_waiting(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &TableInfo, o: &mut Odp, rrn: u64) -> Result<Option<db::Record>> {
    let limit = match t.desc.attr.wait_rcd {
        db::Wait::Immed => Some(std::time::Duration::ZERO),
        db::Wait::Seconds(s) => Some(std::time::Duration::from_secs(u64::from(s))),
        db::Wait::NoMax => None,
    };
    let start = std::time::Instant::now();
    loop {
        match o.chain(txn, &KeyArg::Rrn(rrn), true) {
            Err(e @ db::DbError::RecordLocked { .. }) => {
                if limit.is_some_and(|l| start.elapsed() >= l) || !ctx.wait_turn(txn)? {
                    return Err(e.into());
                }
            }
            r => return Ok(r?),
        }
    }
}

/// Whether the condition still holds for a row read as `was` and locked
/// as `now`, as [`Recheck`] says.
fn still_holds(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    c: &mut Recheck<'_>,
    schema: &Rc<Schema>,
    was: &Row,
    now: &Row,
) -> Result<bool> {
    let env = Env::default().with(schema.clone(), now.clone());
    let Some(mut answers) = c.answers.take() else {
        return Ok(ev.truth(txn, c.cond, &env)? == Some(true));
    };
    if was == now {
        c.answers = Some(answers);
        return Ok(true);
    }
    answers.replay();
    ev.ctx.answers = Some(answers);
    let r = ev.truth(txn, c.cond, &env);
    let answers = ev.ctx.answers.take().expect("put back");
    let holds = answers.missed() || r? == Some(true);
    c.answers = Some(answers);
    Ok(holds)
}

/// One change a `MERGE` decided as it read: the source row, and for a
/// target row matched its RRN and row as read.
struct MergeStep {
    source: Row,
    target: Option<Row>,
    action: usize,
}

/// `MERGE`: for each source row, the matching target rows take the
/// first `WHEN MATCHED` whose condition holds; a row without a match
/// takes the first `WHEN NOT MATCHED`. Returns rows changed.
///
/// Decided as of the statement's start: every source row is matched and
/// its action chosen first, taking turns as a query does, its
/// subqueries' answers kept ([`super::answers`]); then the changes are
/// written, taking turns between them. A target row another job changed
/// meanwhile is matched and its action chosen again as it is now, the
/// subqueries answered as kept (when one has no kept answer the first
/// decision stands).
pub fn merge(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    m: &crate::ast::Merge,
) -> Result<u64> {
    let t = ctx.resolve_table(txn, &m.target)?;
    let alias = m
        .alias
        .as_ref()
        .map_or_else(|| m.target.name.value.clone(), |a| a.value.clone());
    ctx.answers = Some(Answers::recording());
    let plan = merge_plan(txn, ctx, params, m, &t, &alias);
    let answers = ctx.answers.take();
    let (src_schema, tschema, steps) = plan?;
    let mut o = ctx.open(txn, &t, Usage::ALL)?;
    ctx.hold_open();
    let mut w = MergeWrite {
        t: &t,
        m,
        src_schema: &src_schema,
        tschema: &tschema,
        answers,
        changed: std::collections::HashSet::new(),
    };
    let result = w.run(txn, ctx, params, &mut o, steps);
    ctx.release_open();
    o.close(txn);
    result
}

/// The source's and target's schemas and the steps, read before any
/// change.
fn merge_plan(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    m: &crate::ast::Merge,
    t: &TableInfo,
    alias: &str,
) -> Result<(Rc<Schema>, Rc<Schema>, Vec<MergeStep>)> {
    use crate::ast::{Query, QueryBody, Select, SelectItem};
    // The source as a query of its own.
    let src_alias = match &m.source {
        crate::ast::TableRef::Table { name, alias, .. } => alias
            .as_ref()
            .map_or_else(|| name.name.value.clone(), |a| a.value.clone()),
        crate::ast::TableRef::Subquery { alias, .. }
        | crate::ast::TableRef::Function { alias, .. }
        | crate::ast::TableRef::Recursive { alias, .. }
        | crate::ast::TableRef::DataChange { alias, .. } => alias.value.clone(),
        crate::ast::TableRef::Lateral(_) | crate::ast::TableRef::JoinUsing { .. } | crate::ast::TableRef::Join { .. } => {
            return Err(SqlError::Unsupported {
                what: "a join as the MERGE source".into(),
            });
        }
    };
    let src_query = Query::of(QueryBody::Select(Box::new(Select {
        distinct: false,
        items: vec![SelectItem::Star],
        from: vec![m.source.clone()],
        where_clause: None,
        group_by: Vec::new(),
        having: None,
    })));
    let src = run_query(txn, ctx, params, &src_query, &Env::default())?;
    let src_schema = Rc::new(Schema {
        cols: src
            .columns
            .iter()
            .map(|c| super::eval::ColInfo {
                name: c.name.clone(),
                table: Some(src_alias.clone()),
                field: None,
                nullable: c.nullable,
                hidden: false,
            })
            .collect(),
    });
    let mut tschema = Rc::new(super::query::table_schema(t, alias));
    let mut steps = Vec::new();
    for srow in src.rows {
        ctx.tick(txn)?;
        let outer = Env::default().with(src_schema.clone(), srow);
        let (ts, matches) = matching_rows(txn, ctx, params, t, alias, Some(&m.on), &outer)?;
        tschema = ts;
        let source = outer.frames[0].1.clone();
        if matches.is_empty() {
            let mut ev = Evaluator { ctx, params };
            if let Some(action) = merge_action(txn, &mut ev, m, false, &outer)? {
                steps.push(MergeStep { source, target: None, action });
            }
            continue;
        }
        for trow in matches {
            ctx.tick(txn)?;
            let env = outer.with(tschema.clone(), trow);
            let mut ev = Evaluator { ctx, params };
            if let Some(action) = merge_action(txn, &mut ev, m, true, &env)? {
                let target = Some(env.frames[1].1.clone());
                steps.push(MergeStep { source: source.clone(), target, action });
            }
        }
    }
    Ok((src_schema, tschema, steps))
}

/// The first `WHEN MATCHED` (or `WHEN NOT MATCHED`) action whose
/// condition holds in `env`.
fn merge_action(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    m: &crate::ast::Merge,
    matched: bool,
    env: &Env,
) -> Result<Option<usize>> {
    use crate::ast::MergeAction;
    for (i, a) in m.actions.iter().enumerate() {
        let cond = match a {
            MergeAction::Update { cond, .. } | MergeAction::Delete { cond } if matched => cond,
            MergeAction::Insert { cond, .. } if !matched => cond,
            _ => continue,
        };
        if let Some(c) = cond
            && ev.truth(txn, c, env)? != Some(true)
        {
            continue;
        }
        return Ok(Some(i));
    }
    Ok(None)
}

/// The writing half of a `MERGE`.
struct MergeWrite<'a> {
    t: &'a TableInfo,
    m: &'a crate::ast::Merge,
    src_schema: &'a Rc<Schema>,
    tschema: &'a Rc<Schema>,
    /// The subqueries' answers as of the start.
    answers: Option<Answers>,
    /// Target rows this statement changed.
    changed: std::collections::HashSet<u64>,
}

impl MergeWrite<'_> {
    fn run(
        &mut self,
        txn: &mut Txn<'_>,
        ctx: &mut SqlContext,
        params: &[Value],
        o: &mut Odp,
        steps: Vec<MergeStep>,
    ) -> Result<u64> {
        use crate::ast::MergeAction;
        let t = self.t;
        let mut n = 0u64;
        for step in steps {
            ctx.poll_interrupt()?;
            ctx.change_point(txn)?;
            let outer = Env::default().with(self.src_schema.clone(), step.source);
            let mut ev = Evaluator { ctx, params };
            let Some(was) = step.target else {
                let MergeAction::Insert { columns, values, .. } = &self.m.actions[step.action] else {
                    continue;
                };
                let idx = column_indexes(t, columns)?;
                if values.len() != idx.len() {
                    return Err(SqlError::ValueCount {
                        what: format!("{} values for {} columns", values.len(), idx.len()),
                    });
                }
                let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
                for (k, &fi) in idx.iter().enumerate() {
                    per_field[fi] = match &values[k] {
                        Expr::Default => None,
                        e => Some(eval_for(txn, &mut ev, e, &outer, &t.format.format.fields[fi])?),
                    };
                }
                let (rec, nulls) = build_record(txn, t, &per_field, None)?;
                o.write(txn, &rec, &nulls)?;
                n += 1;
                continue;
            };
            let rrn = u64::try_from(was[0].as_i64()?).expect("positive");
            let Some(current) = chain_waiting(txn, ev.ctx, t, o, rrn)? else {
                continue;
            };
            let slot = db::Slot {
                rrn,
                record: current.data.clone(),
                null_map: current.null_map.clone(),
                deleted: false,
            };
            let now = row_of(txn, t, &slot)?;
            let env = outer.with(self.tschema.clone(), now);
            let mut action = Some(step.action);
            if env.frames[1].1 != was && !self.changed.contains(&rrn) {
                // Another job changed it: matched and chosen again.
                action = self.again(txn, &mut ev, &env, step.action)?;
            }
            match action.map(|a| &self.m.actions[a]) {
                Some(MergeAction::Update { set, .. }) => {
                    let assigned = assignments(txn, &mut ev, t, set, &env)?;
                    let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
                    for (i, v) in assigned {
                        per_field[i] = v;
                    }
                    let (rec, nulls) =
                        build_record(txn, t, &per_field, Some((&current.data, &current.null_map)))?;
                    o.update(txn, &rec, &nulls)?;
                    release_lobs(txn, t, &current.data, Some(&rec))?;
                    self.changed.insert(rrn);
                    n += 1;
                }
                Some(MergeAction::Delete { .. }) => {
                    o.delete(txn)?;
                    release_lobs(txn, t, &current.data, None)?;
                    n += 1;
                }
                _ => o.unlock(),
            }
        }
        Ok(n)
    }

    /// The `ON` condition and the action for a row as it is now, the
    /// subqueries answered as kept; `first` when one has no kept answer.
    fn again(
        &mut self,
        txn: &mut Txn<'_>,
        ev: &mut Evaluator<'_>,
        env: &Env,
        first: usize,
    ) -> Result<Option<usize>> {
        let mut answers = self.answers.take().unwrap_or_else(Answers::recording);
        answers.replay();
        ev.ctx.answers = Some(answers);
        let r = (|| {
            if ev.truth(txn, &self.m.on, env)? != Some(true) {
                return Ok(None);
            }
            merge_action(txn, ev, self.m, true, env)
        })();
        let answers = ev.ctx.answers.take().expect("put back");
        let missed = answers.missed();
        self.answers = Some(answers);
        if missed {
            return Ok(Some(first));
        }
        r
    }
}
