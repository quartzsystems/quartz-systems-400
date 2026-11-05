// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running a query: scans with access path selection, joins, grouping,
//! projection, `DISTINCT`, ordering and row limits. Scans read their
//! paths in batches; joins keep only the rows that match (`join`);
//! `ORDER BY` is an external merge sort that spills past the job's row
//! budget (`sort`); what the other steps hold counts against the
//! statement's working-storage limit (`mem`).

use std::collections::{HashMap, HashSet};
use std::ops::Bound;
use std::rc::Rc;

use db::file::{self, FileKind, MemberEntry};
use db::format::FormatDef;
use db::member::{Core, Member, Slot};
use db::util::{Order, each_record};
use dds::FieldDef;
use store::obj::{LockState, ObjectId};
use store::txn::Txn;

use super::eval::{ColInfo, Env, Evaluator, Row, Schema, has_aggregate, sql_type};
use super::grouping;
use super::restype::{Scope, SqlType, type_in};
use super::join::{self, Item};
use super::sort::{KeySpec, Sorter};
use super::{ColumnDesc, ResultSet, SqlContext, TableInfo};
use crate::ast::{
    CmpOp, ColumnRef, Expr, JoinKind, Literal, OrderItem, Query, QueryBody, Select, SelectItem,
    TableRef, columns_of,
};
use crate::error::{Result, SqlError};
use crate::value::Value;

mod bitmap;

/// Path entries a keyed scan reads per batch.
const KEY_BATCH: usize = 256;

/// A table in `FROM`, with its schema and the predicates pushed to it.
pub(super) struct Scan {
    /// The file, or `None` for a catalog table.
    info: Option<TableInfo>,
    /// Rows of a catalog table.
    rows: Option<Rc<Vec<Row>>>,
    alias: String,
    schema: Rc<Schema>,
    preds: Vec<Expr>,
    /// Not the right side of an outer join: predicates may be pushed.
    pushable: bool,
}

/// The schema of a table's rows: the hidden RRN, then every field.
pub(crate) fn table_schema(t: &TableInfo, alias: &str) -> Schema {
    let mut cols = vec![ColInfo {
        name: "*RRN".into(),
        table: Some(alias.to_string()),
        field: None,
        nullable: false,
        hidden: true,
    }];
    for f in &t.format.format.fields {
        cols.push(ColInfo {
            // A column is named by its SQL name, as IBM describes it.
            name: f.alias.clone().unwrap_or_else(|| f.name.to_string()),
            table: Some(alias.to_string()),
            field: Some(f.clone()),
            nullable: f.allow_null,
            hidden: false,
        });
    }
    Schema { cols }
}

/// A record of `t` as a row: RRN first. LOB values are read from the
/// LOB tree of the member that holds them.
pub fn row_of(txn: &mut Txn<'_>, t: &TableInfo, slot: &Slot) -> Result<Row> {
    let fmt = &t.format;
    let mut row = Vec::with_capacity(fmt.format.fields.len() + 1);
    row.push(Value::int(i64::try_from(slot.rrn).expect("fits")));
    let mut owner = None;
    for (i, f) in fmt.format.fields.iter().enumerate() {
        let v = db::record::read_field(f, &slot.record, &slot.null_map, fmt.null_index(i))
            .map_err(|e| SqlError::Conversion {
                what: e.to_string(),
            })?;
        match v {
            db::record::Value::Char(r) if f.data_type.is_lob() => {
                let o = match owner {
                    Some(o) => o,
                    None => *owner.insert(t.lob_owner(txn)?),
                };
                let lob = db::member::lob::LobRef::decode(&r)?;
                row.push(crate::value::lob_value(f, &db::member::lob::read(txn, o, &lob)?));
            }
            v => row.push(Value::from_field(f, &v)?),
        }
    }
    Ok(row)
}

/// Run a query against the outer rows.
pub fn run_query(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
    outer: &Env,
) -> Result<ResultSet> {
    let (schema, produced) = run_body(txn, ctx, params, q, outer, false)?;
    Ok(result_set(&schema, &produced))
}

/// Plan a query and leave the plan text in `ctx.explain`.
pub fn explain(txn: &mut Txn<'_>, ctx: &mut SqlContext, params: &[Value], q: &Query) -> Result<()> {
    run_body(txn, ctx, params, q, &Env::default(), true).map(|_| ())
}

/// An output row with the source row it came from, for `ORDER BY`.
pub(crate) struct Produced {
    pub(crate) out: Row,
    pub(crate) source: Env,
}

pub(crate) fn run_body(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
    outer: &Env,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Produced>)> {
    let saved_skip = ctx.skip_locked;
    ctx.skip_locked |= q.skip_locked;
    let r = run_limited(txn, ctx, params, q, outer, explain_only);
    ctx.skip_locked = saved_skip;
    r
}

fn run_limited(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
    outer: &Env,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Produced>)> {
    // Views that run their query come in as derived tables.
    let expanded = super::view::expand(txn, ctx, q)?;
    let q = expanded.as_ref().unwrap_or(q);
    // The statement's outermost query meets the query options' limits.
    if !ctx.plan.governed && outer.frames.is_empty() && !explain_only {
        ctx.plan.governed = true;
        super::plan::govern::check(txn, ctx, q)?;
    }
    // The row goal: `OPTIMIZE FOR n ROWS`, else `FETCH FIRST` (with any
    // `OFFSET`) given as numbers.
    let literal = |e: &Option<Expr>| match e {
        Some(Expr::Literal(Literal::Num(n))) => n.parse::<u64>().ok(),
        Some(_) => None,
        None => Some(0),
    };
    let limit = match (q.fetch_first.is_some(), literal(&q.fetch_first), literal(&q.offset)) {
        (true, Some(f), Some(o)) => Some(f.saturating_add(o)),
        _ => None,
    };
    let goal = match q.optimize_for {
        Some(u64::MAX) => None,
        Some(n) => Some(n),
        None => limit,
    };
    let saved = (ctx.plan.goal, ctx.plan.limit);
    ctx.plan.goal = goal;
    ctx.plan.limit = if matches!(q.body, QueryBody::Select(_)) { limit.and_then(|l| usize::try_from(l).ok()) } else { None };
    let r = body(txn, ctx, params, &q.body, outer, q, explain_only);
    (ctx.plan.goal, ctx.plan.limit) = saved;
    let (schema, mut produced) = r?;
    if !q.order_by.is_empty() && !matches!(q.body, QueryBody::Select(_)) {
        order_rows(txn, ctx, params, &schema, &mut produced, &q.order_by)?;
        if explain_only {
            ctx.explain.push("SORT".into());
        }
    }
    if q.offset.is_some() || q.fetch_first.is_some() {
        let mut ev = Evaluator { ctx, params };
        let env = Env::default();
        let offset = match &q.offset {
            Some(e) => usize::try_from(ev.eval(txn, e, &env)?.as_i64()?.max(0)).expect("fits"),
            None => 0,
        };
        let first = match &q.fetch_first {
            Some(e) => {
                Some(usize::try_from(ev.eval(txn, e, &env)?.as_i64()?.max(0)).expect("fits"))
            }
            None => None,
        };
        produced = produced
            .into_iter()
            .skip(offset)
            .take(first.unwrap_or(usize::MAX))
            .collect();
        if explain_only {
            ctx.explain
                .push(format!("LIMIT offset {offset} first {first:?}"));
        }
    }
    Ok((schema, produced))
}

/// The visible columns of produced rows as a result set.
fn result_set(schema: &Schema, produced: &[Produced]) -> ResultSet {
    let visible: Vec<usize> = schema
        .cols
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.hidden)
        .map(|(i, _)| i)
        .collect();
    ResultSet {
        columns: schema.describe(),
        rows: produced
            .iter()
            .map(|p| visible.iter().map(|&i| p.out[i].clone()).collect())
            .collect(),
    }
}

/// Run a query for a cursor: the rows, and when the query reads one
/// table plainly, that table and each row's relative record number for
/// `WHERE CURRENT OF`.
pub fn run_cursor_query(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
) -> Result<(ResultSet, Option<TableInfo>, Vec<Option<u64>>)> {
    let (schema, produced) = run_body(txn, ctx, params, q, &Env::default(), false)?;
    let rs = result_set(&schema, &produced);
    let single = match &q.body {
        QueryBody::Select(sel)
            if sel.from.len() == 1 && sel.group_by.is_empty() && !sel.distinct =>
        {
            match &sel.from[0] {
                // A catalog view has no file: its cursor is read only.
                TableRef::Table { name, .. } if super::catalog::is_view(name) && ctx.resolve_table(txn, name).is_err() => None,
                TableRef::Table { name, alias, .. } => Some((
                    ctx.resolve_table(txn, name)?,
                    alias
                        .as_ref()
                        .map_or_else(|| name.name.value.clone(), |a| a.value.clone()),
                )),
                _ => None,
            }
        }
        _ => None,
    };
    let Some((table, alias)) = single else {
        let n = produced.len();
        return Ok((rs, None, vec![None; n]));
    };
    let rrns = produced
        .iter()
        .map(|p| {
            p.source.frames.iter().rev().find_map(|(schema, row)| {
                schema
                    .cols
                    .iter()
                    .position(|c| c.hidden && c.table.as_deref() == Some(alias.as_str()))
                    .and_then(|i| row[i].as_i64().ok())
                    .and_then(|n| u64::try_from(n).ok())
            })
        })
        .collect();
    Ok((rs, Some(table), rrns))
}

#[allow(clippy::too_many_lines)]
fn body(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    b: &QueryBody,
    outer: &Env,
    q: &Query,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Produced>)> {
    match b {
        QueryBody::Select(sel) => select(txn, ctx, params, sel, outer, &q.order_by, explain_only),
        QueryBody::Values(rows) => {
            let mut ev = Evaluator { ctx, params };
            let mut out = Vec::new();
            let width = rows.first().map_or(0, Vec::len);
            let mut scope = Scope::of_env(outer);
            scope.sequences = super::restype::sequences(txn, ev.ctx, rows.iter().flatten());
            scope.variables = super::restype::variables(txn, ev.ctx, &scope, rows.iter().flatten());
            let types = super::restype::values_types(rows, width, &scope)?;
            for r in rows {
                if r.len() != width {
                    return Err(SqlError::ValueCount {
                        what: "VALUES rows differ in length".into(),
                    });
                }
                let mut row = Vec::with_capacity(r.len());
                for (e, fit) in r.iter().zip(&types) {
                    let mode = ev.ctx.decfloat_rounding;
                    let v = ev.eval(txn, e, outer).and_then(|v| match fit {
                        Some(t) => super::restype::conform(v, t, mode),
                        None => Ok(v),
                    });
                    row.push(super::cast::mapped(ev.ctx, v)?);
                }
                out.push(row);
            }
            let schema = Rc::new(Schema {
                cols: (0..width)
                    .map(|i| ColInfo {
                        name: format!("{:05}", i + 1),
                        table: None,
                        field: types[i].as_ref().map(super::restype::field_of),
                        nullable: types[i].as_ref().is_none_or(|t| t.nullable),
                        hidden: false,
                    })
                    .collect(),
            });
            Ok((
                schema.clone(),
                out.into_iter()
                    .map(|r| Produced {
                        source: outer.with(schema.clone(), r.clone()),
                        out: r,
                    })
                    .collect(),
            ))
        }
        QueryBody::Union { left, right, all } => {
            let (ls, mut left_rows) = body(txn, ctx, params, left, outer, q, explain_only)?;
            let (rs, right_rows) = body(txn, ctx, params, right, outer, q, explain_only)?;
            if visible_count(&ls) != visible_count(&rs) {
                return Err(SqlError::ValueCount {
                    what: "UNION sides have different numbers of columns".into(),
                });
            }
            // Rows of the union carry only visible columns.
            let strip = |s: &Schema, rows: Vec<Produced>| -> Vec<Produced> {
                let idx: Vec<usize> = s
                    .cols
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| !c.hidden)
                    .map(|(i, _)| i)
                    .collect();
                rows.into_iter()
                    .map(|p| Produced {
                        out: idx.iter().map(|&i| p.out[i].clone()).collect(),
                        source: p.source,
                    })
                    .collect()
            };
            left_rows = strip(&ls, left_rows);
            let mut all_rows = left_rows;
            all_rows.extend(strip(&rs, right_rows));
            let mut cols: Vec<ColInfo> = ls.cols.iter().filter(|c| !c.hidden).cloned().collect();
            // Both sides' values in the type they have in common.
            let right_cols: Vec<&ColInfo> = rs.cols.iter().filter(|c| !c.hidden).collect();
            let fits = super::restype::union_types(&mut cols, &right_cols)?;
            for p in &mut all_rows {
                for (v, fit) in p.out.iter_mut().zip(&fits) {
                    if let Some(t) = fit {
                        let r = super::restype::conform(std::mem::replace(v, Value::Null), t, ctx.decfloat_rounding);
                        *v = super::cast::mapped(ctx, r)?;
                    }
                }
            }
            let schema = Rc::new(Schema { cols });
            let mut charge = ctx.charge();
            for p in &all_rows {
                charge.row(&p.out)?;
            }
            if !*all {
                let n = all_rows.len() as u64;
                ctx.dbmon.note(crate::dbmon::plan::PlanEvent::Sort { rows: n, reason: "F5" });
                distinct(&mut charge, &mut all_rows)?;
            }
            if explain_only {
                ctx.explain
                    .push(format!("UNION{}", if *all { " ALL" } else { "" }));
            }
            Ok((schema, all_rows))
        }
        QueryBody::Nested(inner) => run_body(txn, ctx, params, inner, outer, explain_only),
    }
}

fn visible_count(s: &Schema) -> usize {
    s.cols.iter().filter(|c| !c.hidden).count()
}

/// Keep the first of each set of equal rows; the keys count against
/// the statement's working storage.
fn distinct(charge: &mut super::mem::Charge, rows: &mut Vec<Produced>) -> Result<()> {
    let mut seen = HashSet::new();
    let mut kept = Vec::with_capacity(rows.len());
    for p in rows.drain(..) {
        let key = row_key(&p.out);
        if !seen.contains(&key) {
            charge.add(key.len() + 32)?;
            seen.insert(key);
            kept.push(p);
        }
    }
    *rows = kept;
    Ok(())
}

fn row_key(r: &Row) -> String {
    let mut s = String::new();
    for v in r {
        s.push_str(&v.key());
        s.push('\u{1}');
    }
    s
}

/// Split a condition into its `AND`ed parts.
fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::And(a, b) => {
            conjuncts(a, out);
            conjuncts(b, out);
        }
        other => out.push(other.clone()),
    }
}

/// The aliases a predicate references, or `None` when a column is
/// unknown here (an outer reference) or ambiguous.
fn aliases_of(e: &Expr, scans: &[Scan]) -> Option<HashSet<String>> {
    let mut out = HashSet::new();
    for c in columns_of(e) {
        let name = c.column.value.as_str();
        if let Some(t) = &c.table {
            let t = t.value.as_str();
            if !scans.iter().any(|s| s.alias == t) {
                return None;
            }
            out.insert(t.to_string());
        } else {
            let owners: Vec<&Scan> = scans
                .iter()
                .filter(|s| {
                    s.schema
                        .cols
                        .iter()
                        .any(|col| !col.hidden && col.name == name)
                })
                .collect();
            if owners.len() != 1 {
                return None;
            }
            out.insert(owners[0].alias.clone());
        }
    }
    if has_subquery(e) {
        return None;
    }
    Some(out)
}

/// A subquery without its correlating equalities, and each of them as
/// (inner expression, outer expression).
pub(crate) type Correlated = (Select, Vec<(Expr, Expr)>);

/// A subquery's `WHERE` split for decorrelation: the select without
/// its correlating equalities, and each as (inner expression, outer
/// expression). `None` when its `FROM` has more than tables and joins,
/// or no conjunct is an equality of an expression of its own columns and
/// one of only outer columns.
pub(crate) fn correlation(txn: &mut Txn<'_>, ctx: &SqlContext, sel: &Select) -> Result<Option<Correlated>> {
    let Some(w) = &sel.where_clause else { return Ok(None) };
    if sel.from.is_empty() || sel.from.iter().any(has_derived) {
        return Ok(None);
    }
    let mut scans = Vec::new();
    for t in &sel.from {
        collect_scans(txn, ctx, t, true, &mut scans)?;
    }
    // A column none of the subquery's tables has.
    let outer_only = |e: &Expr| {
        let cols = columns_of(e);
        !cols.is_empty()
            && !has_subquery(e)
            && cols.iter().all(|c| match &c.table {
                Some(t) => !scans.iter().any(|s| s.alias == t.value),
                None => !scans.iter().any(|s| s.schema.cols.iter().any(|col| !col.hidden && col.name == c.column.value)),
            })
    };
    let inner_only = |e: &Expr| aliases_of(e, &scans).is_some_and(|a| !a.is_empty());
    let mut parts = Vec::new();
    conjuncts(w, &mut parts);
    let mut kept = Vec::new();
    let mut pairs = Vec::new();
    for p in parts {
        if let Expr::Compare(a, CmpOp::Eq, b, None) = &p {
            if inner_only(a) && outer_only(b) {
                pairs.push(((**a).clone(), (**b).clone()));
                continue;
            }
            if inner_only(b) && outer_only(a) {
                pairs.push(((**b).clone(), (**a).clone()));
                continue;
            }
        }
        kept.push(p);
    }
    if pairs.is_empty() {
        return Ok(None);
    }
    let mut out = sel.clone();
    out.where_clause = kept.into_iter().reduce(|a, b| Expr::And(Box::new(a), Box::new(b)));
    Ok(Some((out, pairs)))
}

/// Whether a FROM item holds a derived table.
fn has_derived(t: &TableRef) -> bool {
    match t {
        TableRef::Table { .. } => false,
        TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => has_derived(left) || has_derived(right),
        _ => true,
    }
}

pub(crate) fn has_subquery(e: &Expr) -> bool {
    match e {
        Expr::Subquery(_) | Expr::InSubquery(..) | Expr::Exists(..) => true,
        Expr::Compare(a, _, b, q) => q.is_some() || has_subquery(a) || has_subquery(b),
        Expr::Neg(a)
        | Expr::Not(a)
        | Expr::IsNull(a, _)
        | Expr::Cast(a, _)
        | Expr::Duration(a, _) => has_subquery(a),
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) => {
            has_subquery(a) || has_subquery(b)
        }
        Expr::Between(a, b, c, _) => has_subquery(a) || has_subquery(b) || has_subquery(c),
        Expr::InList(a, l, _) => has_subquery(a) || l.iter().any(has_subquery),
        Expr::Like(a, p, _, _) => has_subquery(a) || has_subquery(p),
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            operand.as_deref().is_some_and(has_subquery)
                || whens
                    .iter()
                    .any(|(w, t)| has_subquery(w) || has_subquery(t))
                || otherwise.as_deref().is_some_and(has_subquery)
        }
        Expr::Function(f) => f.args.iter().any(has_subquery),
        Expr::Row(items) => items.iter().any(has_subquery),
        _ => false,
    }
}

/// Build the scans of a `FROM` tree, marking which may take predicates.
fn collect_scans(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    t: &TableRef,
    pushable: bool,
    out: &mut Vec<Scan>,
) -> Result<()> {
    match t {
        TableRef::Table { name, alias, .. } => {
            let alias = alias
                .as_ref()
                .map_or_else(|| name.name.value.clone(), |a| a.value.clone());
            // The name of a recursive expression while its step runs.
            if name.schema.is_none()
                && let Some(b) = ctx.ctes.iter().rev().find(|b| b.name == name.name.value)
            {
                out.push(Scan {
                    info: None,
                    rows: Some(b.rows.clone()),
                    schema: Rc::new(b.schema_as(&alias)),
                    alias,
                    preds: Vec::new(),
                    pushable,
                });
                return Ok(());
            }
            match ctx.resolve_table(txn, name) {
                Ok(info) => {
                    let schema = Rc::new(table_schema(&info, &alias));
                    out.push(Scan {
                        info: Some(info),
                        rows: None,
                        alias,
                        schema,
                        preds: Vec::new(),
                        pushable,
                    });
                }
                Err(e) => {
                    let Some((schema, rows)) =
                        super::catalog::virtual_table(txn, ctx, name, &alias)?
                    else {
                        return Err(e);
                    };
                    out.push(Scan {
                        info: None,
                        rows: Some(Rc::new(rows)),
                        alias,
                        schema: Rc::new(schema),
                        preds: Vec::new(),
                        pushable,
                    });
                }
            }
        }
        TableRef::Subquery { .. }
        | TableRef::Lateral(_)
        | TableRef::Function { .. }
        | TableRef::Recursive { .. }
        | TableRef::DataChange { .. } => {}
        TableRef::Join { left, right, kind, .. } | TableRef::JoinUsing { left, right, kind, .. } => {
            let (lp, rp) = match kind {
                JoinKind::Inner | JoinKind::Cross => (pushable, pushable),
                JoinKind::Left | JoinKind::LeftException => (pushable, false),
                JoinKind::Right | JoinKind::RightException => (false, pushable),
                JoinKind::Full => (false, false),
            };
            collect_scans(txn, ctx, left, lp, out)?;
            collect_scans(txn, ctx, right, rp, out)?;
        }
    }
    Ok(())
}

/// A keyed candidate: a path over the table's records.
struct Candidate {
    /// The member holding the path.
    member: MemberEntry,
    /// Its file.
    file: ObjectId,
    /// The format the keys are encoded for.
    fmt: FormatDef,
    /// Key field names with `DESCEND`.
    keys: Vec<(String, bool)>,
    label: String,
    /// A unique or primary key of the table's constraint tree: the
    /// constraint's number and the tree's key length.
    cst: Option<(u16, usize)>,
    /// The path's file and member, and its constraint, as the database
    /// monitor names them.
    ix: (crate::dbmon::plan::TableName, Option<String>),
}

/// The paths over a table: its own and those of dependent logical files
/// that select nothing and rename nothing.
/// Whether a file is held `*EXCL`: an index whose path is being built
/// (`ddl::build_index`), not to be read or built again by others.
fn in_build(txn: &mut Txn<'_>, file: ObjectId) -> bool {
    txn.store().locks_mut().holders(file).iter().any(|h| h.state == LockState::Exclusive)
}

fn candidates(txn: &mut Txn<'_>, t: &TableInfo) -> Vec<Candidate> {
    let mut out = Vec::new();
    let key_names = |fmt: &FormatDef| -> Vec<(String, bool)> {
        fmt.format
            .keys
            .iter()
            .map(|k| (k.name.to_string(), k.descend))
            .collect()
    };
    if t.format.key_len > 0 {
        out.push(Candidate {
            member: t.member,
            file: t.id,
            fmt: t.format.clone(),
            keys: key_names(&t.format),
            label: format!("{}/{} own key", t.desc.library_name, t.desc.name()),
            cst: None,
            ix: (table_name(t), None),
        });
    }
    if t.desc.attr.kind == FileKind::Logical {
        return out;
    }
    // Unique and primary keys of null-free fields: their tree entries
    // are the path's encoding behind the constraint's number.
    if let Ok(Some(a)) = db::cst::Active::load(txn, &t.desc) {
        for k in &a.keys {
            let c = &a.csts[k.index];
            if !c.kind.unique() || k.format.format.keys.iter().any(|kf| k.format.format.fields[kf.field].allow_null) {
                continue;
            }
            out.push(Candidate {
                member: t.member,
                file: t.id,
                fmt: k.format.clone(),
                keys: key_names(&k.format),
                label: format!("{}/{} constraint {}", t.desc.library_name, t.desc.name(), c.name),
                cst: Some((k.number, a.key_len)),
                ix: (table_name(t), Some(c.name.clone())),
            });
        }
    }
    for d in &t.desc.dependents {
        if d.base_member != t.member.object || in_build(txn, d.lf_file) {
            continue;
        }
        let Ok(lf) = file::describe(txn, d.lf_file) else {
            continue;
        };
        if lf.formats.len() != 1 || lf.attr.dynslt {
            continue;
        }
        let fmt = &lf.formats[0];
        let Some(log) = &fmt.logical else { continue };
        if fmt.key_len == 0 || !log.select_omit.is_empty() || log.join {
            continue;
        }
        // Every key field must be a base field of the same name.
        let plain = fmt.format.keys.iter().all(|k| {
            let f = &fmt.format.fields[k.field];
            t.format.format.field(&f.name).is_some_and(|(_, bf)| {
                bf.data_type == f.data_type && bf.length == f.length && bf.decimals == f.decimals
            }) && log
                .sources
                .iter()
                .any(|s| matches!(s, dds::FieldSource::Base { field, .. } if *field == f.name))
        });
        if !plain {
            continue;
        }
        let Some(entry) = lf.members.iter().find(|m| m.object == d.lf_member) else {
            continue;
        };
        out.push(Candidate {
            member: *entry,
            file: d.lf_file,
            fmt: fmt.clone(),
            keys: key_names(fmt),
            label: format!("{}/{} ({})", lf.library_name, lf.name(), entry.name),
            cst: None,
            ix: (
                crate::dbmon::plan::TableName { lib: lf.library_name.to_string(), file: lf.name().to_string(), member: entry.name.to_string() },
                None,
            ),
        });
    }
    out
}

/// A table as the database monitor names it.
fn table_name(t: &TableInfo) -> crate::dbmon::plan::TableName {
    crate::dbmon::plan::TableName {
        lib: t.desc.library_name.to_string(),
        file: t.desc.name().to_string(),
        member: t.member.name.to_string(),
    }
}

/// Tell the database monitor how `t` is read.
fn monitor_access(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &TableInfo, cands: &[Candidate], choice: Option<&Choice>, selection: bool) {
    use crate::dbmon::plan::PlanEvent;
    let rows = Member::open(txn, &t.desc, &t.member).map_or(0, |m| m.attr().record_count);
    let position = ctx.dbmon.trace.as_ref().map_or(1, |p| p.iter().filter(|e| !matches!(e, PlanEvent::Sort { .. })).count() + 1);
    let position = u32::try_from(position).unwrap_or(u32::MAX);
    let e = match choice {
        Some(ch) => {
            let c = &cands[ch.cand];
            let n = ch.eq.len() + usize::from(ch.lo.is_some() || ch.hi.is_some());
            PlanEvent::Index {
                table: table_name(t),
                index: c.ix.0.clone(),
                constraint: c.ix.1.clone(),
                rows,
                positioned: c.keys.iter().take(n).map(|(k, _)| k.clone()).collect(),
                ordering: ch.ordered.is_some(),
                position,
            }
        }
        None => PlanEvent::Scan { table: table_name(t), rows, indexes: !cands.is_empty(), selection, position },
    };
    ctx.dbmon.note(e);
}

/// A predicate `col op value` with a value free of columns.
fn simple_predicate(e: &Expr) -> Option<(String, CmpOp, Expr)> {
    let constant =
        |x: &Expr| columns_of(x).is_empty() && !has_subquery(x) && !matches!(x, Expr::Default);
    match e {
        Expr::Compare(a, op, b, None) => {
            if let Expr::Column(c) = &**a
                && constant(b)
            {
                return Some((c.column.value.clone(), *op, (**b).clone()));
            }
            if let Expr::Column(c) = &**b
                && constant(a)
            {
                let flipped = match op {
                    CmpOp::Lt => CmpOp::Gt,
                    CmpOp::Le => CmpOp::Ge,
                    CmpOp::Gt => CmpOp::Lt,
                    CmpOp::Ge => CmpOp::Le,
                    o => *o,
                };
                return Some((c.column.value.clone(), flipped, (**a).clone()));
            }
            None
        }
        _ => None,
    }
}

/// How a candidate serves the predicates and the ordering.
struct Choice {
    cand: usize,
    eq: Vec<Expr>,
    lo: Option<Expr>,
    hi: Option<Expr>,
    /// Reads in key order satisfy `ORDER BY`; `Some(desc)`.
    ordered: Option<bool>,
    score: u32,
}

/// What the choice of a path weighs beside the predicates.
struct Weigh {
    /// Rows in the table's member.
    rows: u64,
    /// The member whose column statistics apply; `None` for a logical
    /// file, which has none.
    member: Option<ObjectId>,
    /// The query's row goal.
    goal: Option<u64>,
}

/// Tables smaller than this read alike by any plan: the key is used
/// whenever it serves.
const SCAN_FLOOR: u64 = 10_000;
/// A keyed read expected to bring back more than this share of a large
/// table reads the table in arrival order instead.
const SCAN_SHARE: f64 = 0.2;
/// A row goal at most this small takes the path that gives the order.
const SMALL_GOAL: u64 = 1_000;

/// The simple predicates of `preds`: `col op value`, and `BETWEEN` as
/// two.
fn simple_predicates(preds: &[Expr]) -> Vec<(String, CmpOp, Expr)> {
    let mut simple: Vec<(String, CmpOp, Expr)> = Vec::new();
    for p in preds {
        if let Some(s) = simple_predicate(p) {
            simple.push(s);
        } else if let Expr::Between(a, l, h, false) = p
            && let Expr::Column(col) = &**a
            && columns_of(l).is_empty()
            && columns_of(h).is_empty()
        {
            simple.push((col.column.value.clone(), CmpOp::Ge, (**l).clone()));
            simple.push((col.column.value.clone(), CmpOp::Le, (**h).clone()));
        }
    }
    simple
}

fn choose(cands: &[Candidate], preds: &[Expr], order: &[OrderItem], alias: &str, w: &Weigh) -> Option<Choice> {
    let mut all: Vec<(Choice, u64, bool)> = Vec::new();
    let simple = simple_predicates(preds);
    for (ci, c) in cands.iter().enumerate() {
        let mut eq = Vec::new();
        let (mut lo, mut hi) = (None, None);
        let (mut lo_op, mut hi_op) = (CmpOp::Ge, CmpOp::Le);
        // The share of rows the positioned keys select.
        let mut share = 1.0;
        let mut known = false;
        let stats_of = |name: &str| w.member.and_then(|m| super::plan::stats::get(m, name));
        for (name, descend) in &c.keys {
            if let Some((_, _, v)) = simple
                .iter()
                .find(|(n, op, _)| n == name && *op == CmpOp::Eq)
            {
                let st = stats_of(name);
                known |= st.is_some();
                share *= super::plan::estimate::compare(st.as_ref(), CmpOp::Eq, super::plan::estimate::constant(v).as_ref());
                eq.push(v.clone());
                continue;
            }
            if !descend {
                for (n, op, v) in &simple {
                    if n != name {
                        continue;
                    }
                    match op {
                        CmpOp::Gt | CmpOp::Ge if lo.is_none() => {
                            lo = Some(v.clone());
                            lo_op = *op;
                        }
                        CmpOp::Lt | CmpOp::Le if hi.is_none() => {
                            hi = Some(v.clone());
                            hi_op = *op;
                        }
                        _ => {}
                    }
                }
                if lo.is_some() || hi.is_some() {
                    let st = stats_of(name);
                    known |= st.is_some();
                    let (lv, hv) = (lo.as_ref().map(super::plan::estimate::constant), hi.as_ref().map(super::plan::estimate::constant));
                    share *= super::plan::estimate::range(
                        st.as_ref(),
                        lv.as_ref().map(|v| (v.as_ref(), lo_op == CmpOp::Ge)),
                        hv.as_ref().map(|v| (v.as_ref(), hi_op == CmpOp::Le)),
                    );
                }
            }
            break;
        }
        let ordered = order_matches(c, order, alias);
        let score = u32::try_from(eq.len()).expect("few") * 10
            + u32::from(lo.is_some() || hi.is_some()) * 5
            + u32::from(ordered.is_some()) * 3;
        if score > 0 {
            let est = super::plan::estimate::rows(w.rows, share);
            all.push((Choice { cand: ci, eq, lo, hi, ordered, score }, est, known));
        }
    }
    // The best score, then the fewest rows expected; a small row goal
    // first takes a path that gives the order asked for.
    let small_goal = w.goal.is_some_and(|g| g <= SMALL_GOAL);
    let mut best: Option<(Choice, u64, bool)> = None;
    for c in all {
        let better = match &best {
            None => true,
            Some(b) => {
                let (co, bo) = (small_goal && c.0.ordered.is_some(), small_goal && b.0.ordered.is_some());
                (co, c.0.score, std::cmp::Reverse(c.1)) > (bo, b.0.score, std::cmp::Reverse(b.1))
            }
        };
        if better {
            best = Some(c);
        }
    }
    let (choice, est, known) = best?;
    // Statistics say the key would read much of a large table, and its
    // order is not needed: read the table in arrival order.
    #[allow(clippy::cast_precision_loss)]
    let wide = known && w.rows >= SCAN_FLOOR && est as f64 > w.rows as f64 * SCAN_SHARE;
    if wide && choice.ordered.is_none() {
        return None;
    }
    Some(choice)
}

/// Whether `ORDER BY` is the candidate's key order; `Some(desc)`.
fn order_matches(c: &Candidate, order: &[OrderItem], alias: &str) -> Option<bool> {
    if order.is_empty() || order.len() > c.keys.len() {
        return None;
    }
    let mut dir: Option<bool> = None;
    for (item, (name, descend)) in order.iter().zip(&c.keys) {
        let Expr::Column(col) = &item.expr else {
            return None;
        };
        if col.column.value != *name || col.table.as_ref().is_some_and(|t| t.value != alias) {
            return None;
        }
        if item.nulls_first.is_some() {
            return None;
        }
        // Reading backward flips every key; the request must flip them all.
        let d = item.desc != *descend;
        if dir.is_some_and(|x| x != d) {
            return None;
        }
        dir = Some(d);
    }
    dir
}

/// Scan one table: by its chosen path within bounds, else every record,
/// then its pushed predicates. While it reads it holds its members, and
/// yields only between batches ([`SqlContext::own_point`]), reading them
/// again after; the predicates hold nothing and yield between rows. A
/// scan under a correlated subquery yields the same way: the frames
/// above it hold only rows already read.
fn scan_table(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    scan: &Scan,
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Vec<Row>, bool)> {
    ctx.hold_open();
    let r = scan_table_in(txn, ctx, params, scan, outer, order, explain_only);
    ctx.release_open();
    let (rows, ordered) = r?;
    let mut ev = Evaluator { ctx, params };
    let rows = filter_rows(txn, &mut ev, scan, outer, rows)?;
    // Read triggers, once the members are let go (`reads`).
    if !explain_only
        && let Some(t) = &scan.info
        && reads_fire(txn, t)?
    {
        super::reads::fire(txn, ctx, t, &rows)?;
    }
    Ok((rows, ordered))
}

/// Whether reading `t` fires read triggers (its own, or a logical
/// file's based-on files').
pub(super) fn reads_fire(txn: &mut Txn<'_>, t: &super::TableInfo) -> Result<bool> {
    if t.desc.header.sql.is_none() && t.desc.attr.kind != FileKind::Logical {
        return Ok(false);
    }
    let m = Member::open(txn, &t.desc, &t.member)?;
    Ok(m.base_read_triggers || m.trgs.as_ref().is_some_and(|a| a.any(db::trg::Event::Read)))
}

#[allow(clippy::too_many_lines)]
fn scan_table_in(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    scan: &Scan,
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Vec<Row>, bool)> {
    let mut charge = ctx.charge();
    let mut ev = Evaluator { ctx, params };
    let mut rows = Vec::new();
    let mut ordered = false;
    let Some(t) = &scan.info else {
        if explain_only {
            ev.ctx
                .explain
                .push(format!("SCAN QSYS2 catalog ({})", scan.alias));
            return Ok((Vec::new(), false));
        }
        let all = scan
            .rows
            .as_ref()
            .map(|r| r.as_ref().clone())
            .unwrap_or_default();
        return Ok((all, false));
    };
    let cands = candidates(txn, t);
    let rows_now = Member::open(txn, &t.desc, &t.member).map_or(0, |m| m.attr().record_count);
    let physical = t.desc.attr.kind != FileKind::Logical;
    let weigh = Weigh { rows: rows_now, member: physical.then_some(t.member.object), goal: ev.ctx.plan.goal };
    if physical && rows_now >= super::plan::stats::ASK_FLOOR {
        // Columns the planner would weigh: ask the collection job for them.
        for (name, _, _) in simple_predicates(&scan.preds) {
            if let Some((_, f)) = t.field(&name)
                && super::plan::stats::get(t.member.object, &f.name.to_string()).is_none()
            {
                super::plan::stats::request(t.id, t.member.object, &f.name.to_string());
            }
        }
    }
    let choice = choose(&cands, &scan.preds, order, &scan.alias, &weigh);
    let label = format!("{}/{} ({})", t.desc.library_name, t.desc.name(), scan.alias);
    // Reading a table needs what a read open needs: `*OBJOPR` and `*READ`.
    db::auth::check_open(txn, t.id, db::Usage::INPUT)?;
    if !explain_only && ev.ctx.dbmon.trace.is_some() {
        monitor_access(txn, ev.ctx, t, &cands, choice.as_ref(), !scan.preds.is_empty());
    }
    if !explain_only {
        let mut objects = vec![t.id, t.member.object];
        if let Some(ch) = &choice {
            objects.extend([cands[ch.cand].file, cands[ch.cand].member.object]);
        }
        ev.ctx.hold(txn, &objects, &label)?;
    }
    if !explain_only && reads_fire(txn, t)? {
        super::reads::callable(ev.ctx, t)?;
    }
    let open_pf = |txn: &mut Txn<'_>| -> Result<Option<Member>> {
        if t.desc.attr.kind == FileKind::Logical {
            Ok(None)
        } else {
            Ok(Some(Member::open(txn, &t.desc, &t.member)?))
        }
    };
    let mut base_pf = open_pf(txn)?;
    // Under `*ALL` each record read is read-locked as it is read, before
    // any yield: no other job changes it while the unit of work runs.
    let lock = ev.ctx.locks_reads().then(|| (t.desc.name().to_string(), t.member.name.to_string()));
    let lock_read = |ctx: &mut SqlContext, member: Option<ObjectId>, rrn: u64| -> Result<()> {
        match (&lock, member) {
            (Some((file, mbr)), Some(m)) => ctx.lock_read(m, rrn, file, mbr),
            _ => Ok(()),
        }
    };
    // A row goal the statement must meet no more than: stop once it has
    // that many rows that pass, when the order asked for is the read's.
    let goal = ev.ctx.plan.stop.take();
    let needed = ev.ctx.plan.columns.take();
    let order_kept = order.is_empty() || choice.as_ref().is_some_and(|c| c.ordered.is_some());
    let stop = Stop(goal.filter(|_| order_kept && !scan.preds.iter().any(has_subquery)));
    if let Some(ch) = choice {
        let c = &cands[ch.cand];
        ordered = ch.ordered.is_some();
        // The query needs only key fields of a physical file's path: the
        // keys give them, the records are not read.
        let key_only = base_pf.is_some()
            && needed.as_ref().is_some_and(|names| {
                let fields: Option<Vec<String>> = names.iter().map(|n| t.field(n).map(|(_, f)| f.name.to_string())).collect();
                fields.is_some_and(|f| super::plan::keyonly::covers(&c.fmt, &f))
            });
        if explain_only {
            ev.ctx.explain.push(format!(
                "SCAN {label} keyed via {} eq={} range={} {}{}",
                c.label,
                ch.eq.len(),
                if ch.lo.is_some() || ch.hi.is_some() {
                    "yes"
                } else {
                    "no"
                },
                if ordered { "ordered" } else { "" },
                if key_only { " index only" } else { "" }
            ));
        }
        // Key bounds from the predicate values.
        let mut lo_vals = Vec::new();
        for e in &ch.eq {
            lo_vals.push(ev.eval(txn, e, outer)?);
        }
        let mut hi_vals = lo_vals.clone();
        if let Some(l) = &ch.lo {
            lo_vals.push(ev.eval(txn, l, outer)?);
        }
        if let Some(h) = &ch.hi {
            hi_vals.push(ev.eval(txn, h, outer)?);
        }
        let encode = |vals: &[Value]| -> Result<Vec<u8>> {
            let mut dbv = Vec::new();
            for (i, v) in vals.iter().enumerate() {
                let k = &c.fmt.format.keys[i];
                dbv.push(v.to_field(&c.fmt.format.fields[k.field])?);
            }
            db::path::values_key(&c.fmt, &dbv).map_err(|e| SqlError::Conversion {
                what: e.to_string(),
            })
        };
        let (mut lo, mut hi) = (encode(&lo_vals)?, encode(&hi_vals)?);
        let hi_len = match c.cst {
            Some((number, len)) => {
                lo.splice(0..0, number.to_be_bytes());
                hi.splice(0..0, number.to_be_bytes());
                len
            }
            None => usize::from(c.fmt.key_len),
        };
        hi.resize(hi_len, 0xFF);
        if explain_only {
            return Ok((Vec::new(), ordered));
        }
        let desc = file::describe(txn, c.file)?;
        let mut pm = Member::open(txn, &desc, &c.member)?;
        if pm.attr().path_invalid && in_build(txn, c.file) {
            return Err(SqlError::Locked { what: format!("{} is being built", c.label) });
        }
        pm.ensure_path(txn, &mut |_, _| {})?;
        let backward = ch.ordered == Some(true);
        // The path in batches, each read as far as the bounds allow.
        let path_of = |pm: &Member| {
            if c.cst.is_some() { pm.cst_tree() } else { pm.path() }.ok_or_else(|| SqlError::Database {
                what: format!("{} has no access path", c.label),
                msgid: None,
            })
        };
        let mut path = path_of(&pm)?;
        let mut batch = if backward {
            path.scan_back(txn, Bound::Included(hi.as_slice()), KEY_BATCH)?
        } else {
            path.scan(
                txn,
                Bound::Included(lo.as_slice()),
                Bound::Included(hi.as_slice()),
                KEY_BATCH,
            )?
        };
        'keys: while let Some(last) = batch.last().cloned() {
            for k in &batch {
                let past = if backward {
                    k.key.as_slice() < lo.as_slice() && !k.key.starts_with(&lo)
                } else {
                    k.key.as_slice() > hi.as_slice()
                };
                if past {
                    break 'keys;
                }
                if key_only {
                    let skip = if c.cst.is_some() { 2 } else { 0 };
                    if let Some(vals) = super::plan::keyonly::decode(&c.fmt, &k.key[skip..]) {
                        ev.ctx.poll_interrupt()?;
                        let held = Some(t.member.object);
                        if ev.ctx.skips(held, k.rrn) {
                            continue;
                        }
                        lock_read(ev.ctx, held, k.rrn)?;
                        let mut r: Row = vec![Value::Null; t.format.format.fields.len() + 1];
                        r[0] = Value::int(i64::try_from(k.rrn).expect("fits"));
                        for (fi, v) in vals {
                            let name = &c.fmt.format.fields[fi].name;
                            if let Some((ti, tf)) = t.format.format.field(name) {
                                r[ti + 1] = Value::from_field(tf, &v)?;
                            }
                        }
                        if !stop.keeps(txn, &mut ev, scan, outer, &r)? {
                            continue;
                        }
                        charge.row(&r)?;
                        rows.push(r);
                        if stop.done(rows.len()) {
                            break 'keys;
                        }
                        continue;
                    }
                }
                let slots = match &base_pf {
                    Some(pf) => pf
                        .read(txn, k.rrn)?
                        .filter(|s| !s.deleted)
                        .into_iter()
                        .collect(),
                    None => pm.read_entries(txn, k)?,
                };
                let held = if base_pf.is_some() { Some(t.member.object) } else { base_of(&pm, k.format) };
                for s in slots {
                    ev.ctx.poll_interrupt()?;
                    if ev.ctx.skips(held, s.rrn) {
                        continue;
                    }
                    lock_read(ev.ctx, held, s.rrn)?;
                    let r = row_of(txn, t, &s)?;
                    if !stop.keeps(txn, &mut ev, scan, outer, &r)? {
                        continue;
                    }
                    charge.row(&r)?;
                    rows.push(r);
                    if stop.done(rows.len()) {
                        break 'keys;
                    }
                }
            }
            if ev.ctx.own_point(txn)? {
                // Others ran: the members as they are now.
                base_pf = open_pf(txn)?;
                pm = Member::open(txn, &desc, &c.member)?;
                pm.ensure_path(txn, &mut |_, _| {})?;
                path = path_of(&pm)?;
            }
            batch = if backward {
                path.scan_back_from(txn, Bound::Excluded(&last), KEY_BATCH)?
            } else {
                path.scan_from(txn, Bound::Excluded(&last), KEY_BATCH)?
            };
        }
    } else {
        let several = if base_pf.is_some() { bitmap::describe(&cands, &scan.preds) } else { None };
        if explain_only {
            match several {
                Some(d) => ev.ctx.explain.push(format!("SCAN {label} {d}")),
                None => ev.ctx.explain.push(format!("SCAN {label} full")),
            }
            return Ok((Vec::new(), false));
        }
        if several.is_some()
            && let Some(mut pf) = base_pf.take()
            && let Some((list, _)) = bitmap::rrns(txn, &mut ev, &cands, &scan.preds, outer)?
        {
            // The records by number, in arrival order, a yield between batches.
            for chunk in list.chunks(KEY_BATCH) {
                for &rrn in chunk {
                    ev.ctx.poll_interrupt()?;
                    let Some(s) = pf.read(txn, rrn)?.filter(|s| !s.deleted) else { continue };
                    if ev.ctx.skips(Some(t.member.object), s.rrn) {
                        continue;
                    }
                    lock_read(ev.ctx, Some(t.member.object), s.rrn)?;
                    let r = row_of(txn, t, &s)?;
                    if !stop.keeps(txn, &mut ev, scan, outer, &r)? {
                        continue;
                    }
                    charge.row(&r)?;
                    rows.push(r);
                }
                if stop.done(rows.len()) {
                    break;
                }
                if ev.ctx.own_point(txn)?
                    && let Some(m) = open_pf(txn)?
                {
                    pf = m;
                }
            }
            return Ok((rows, false));
        }
        if base_pf.is_none() && t.desc.attr.kind != FileKind::Logical {
            base_pf = open_pf(txn)?;
        }
        if let Some(mut pf) = base_pf {
            // In batches by RRN, a yield between them.
            let mut from = 1;
            loop {
                let slots = pf.scan(txn, from, KEY_BATCH)?;
                let Some(end) = slots.last().map(|s| s.rrn) else {
                    break;
                };
                for s in slots {
                    ev.ctx.poll_interrupt()?;
                    if ev.ctx.skips(Some(t.member.object), s.rrn) {
                        continue;
                    }
                    lock_read(ev.ctx, Some(t.member.object), s.rrn)?;
                    let r = row_of(txn, t, &s)?;
                    if !stop.keeps(txn, &mut ev, scan, outer, &r)? {
                        continue;
                    }
                    charge.row(&r)?;
                    rows.push(r);
                }
                if stop.done(rows.len()) {
                    break;
                }
                from = end + 1;
                if ev.ctx.own_point(txn)?
                    && let Some(m) = open_pf(txn)?
                {
                    pf = m;
                }
            }
        } else {
            let mut m = Member::open(txn, &t.desc, &t.member)?;
            let mut slots = Vec::new();
            each_record(txn, &mut m, &Order::default(), &mut |_, s| {
                slots.push(s);
                Ok(true)
            })?;
            let held = base_of(&m, 0);
            for s in &slots {
                ev.ctx.poll_interrupt()?;
                if ev.ctx.skips(held, s.rrn) {
                    continue;
                }
                lock_read(ev.ctx, held, s.rrn)?;
                let r = row_of(txn, t, s)?;
                charge.row(&r)?;
                rows.push(r);
            }
            let _ = Core::load;
        }
    }
    Ok((rows, ordered))
}

/// Every column `sel` and its `ORDER BY` name, when that is all they
/// read: no `*`, no subquery.
fn needed_columns(sel: &Select, order: &[OrderItem]) -> Option<Vec<String>> {
    let mut exprs: Vec<&Expr> = Vec::new();
    for i in &sel.items {
        match i {
            SelectItem::Expr(e, _) => exprs.push(e),
            _ => return None,
        }
    }
    exprs.extend(sel.where_clause.iter());
    exprs.extend(sel.group_by.iter());
    exprs.extend(sel.having.iter());
    exprs.extend(order.iter().map(|o| &o.expr));
    let mut names = Vec::new();
    for e in exprs {
        if has_subquery(e) || super::olap::has_over(e) {
            return None;
        }
        // `RRN(t)` reads the record's number, which the key has too.
        for c in columns_of(e) {
            if !names.contains(&c.column.value) {
                names.push(c.column.value.clone());
            }
        }
    }
    Some(names)
}

/// Where a scan with a row goal stops: rows that pass its predicates.
struct Stop(Option<usize>);

impl Stop {
    /// Whether `r` passes (always, without a goal).
    fn keeps(&self, txn: &mut Txn<'_>, ev: &mut Evaluator<'_>, scan: &Scan, outer: &Env, r: &Row) -> Result<bool> {
        if self.0.is_none() || scan.preds.is_empty() {
            return Ok(true);
        }
        let env = outer.with(scan.schema.clone(), r.clone());
        for p in &scan.preds {
            if ev.truth(txn, p, &env)? != Some(true) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Whether `n` rows are enough.
    fn done(&self, n: usize) -> bool {
        self.0.is_some_and(|g| n >= g)
    }
}

/// The physical member a record read through `m` lives in, by format;
/// `None` for a join logical file, whose records are not locked one by one.
fn base_of(m: &Member, format: u16) -> Option<ObjectId> {
    if !m.is_logical() {
        return Some(m.id());
    }
    let fi = usize::from(format);
    match m.logical(fi) {
        Ok(l) if !l.join => m.bases.get(fi).map(|b| b.member),
        _ => None,
    }
}

/// Keep the rows that pass every pushed predicate (all of them: bounds
/// may be loose).
fn filter_rows(
    txn: &mut Txn<'_>,
    ev: &mut Evaluator<'_>,
    scan: &Scan,
    outer: &Env,
    rows: Vec<Row>,
) -> Result<Vec<Row>> {
    if scan.preds.is_empty() {
        return Ok(rows);
    }
    let mut kept = Vec::with_capacity(rows.len());
    for r in rows {
        ev.ctx.tick(txn)?;
        let env = outer.with(scan.schema.clone(), r);
        let mut ok = true;
        for p in &scan.preds {
            if ev.truth(txn, p, &env)? != Some(true) {
                ok = false;
                break;
            }
        }
        if ok {
            kept.push(env.frames.last().expect("pushed").1.clone());
        }
    }
    Ok(kept)
}

/// Rows of a `FROM` item: (schema, rows, ordered by the ORDER BY).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
pub(super) fn from_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableRef,
    scans: &[Scan],
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>, bool)> {
    match t {
        TableRef::Table { name, alias, .. } => {
            let alias = alias
                .as_ref()
                .map_or_else(|| name.name.value.clone(), |a| a.value.clone());
            let scan = scans.iter().find(|s| s.alias == alias).expect("collected");
            let (rows, ordered) = scan_table(txn, ctx, params, scan, outer, order, explain_only)?;
            Ok((scan.schema.clone(), rows, ordered))
        }
        TableRef::Subquery {
            query,
            alias,
            columns,
        } => {
            let (inner, produced) = run_body(txn, ctx, params, query, outer, explain_only)?;
            let rows: Vec<Row> = produced.into_iter().map(|p| p.out).collect();
            let visible: Vec<usize> = inner
                .cols
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.hidden)
                .map(|(i, _)| i)
                .collect();
            let mut cols: Vec<ColInfo> = visible
                .iter()
                .enumerate()
                .map(|(n, &i)| ColInfo {
                    name: columns
                        .get(n)
                        .map_or_else(|| inner.cols[i].name.clone(), |c| c.value.clone()),
                    table: Some(alias.value.clone()),
                    ..inner.cols[i].clone()
                })
                .collect();
            // The row's place in the derived table's order, for ORDER OF.
            cols.push(super::from::order_column(&alias.value));
            let rows = rows
                .into_iter()
                .enumerate()
                .map(|(k, r)| {
                    let mut v: Row = visible.iter().map(|&i| r[i].clone()).collect();
                    v.push(Value::int(i64::try_from(k).expect("fits")));
                    v
                })
                .collect();
            if explain_only {
                ctx.explain.push(format!("DERIVED TABLE ({})", alias.value));
            }
            Ok((Rc::new(Schema { cols }), rows, false))
        }
        TableRef::Join { left, right, kind, on } if matches!(**right, TableRef::Lateral(_)) => {
            super::from::lateral_join(txn, ctx, params, left, right, *kind, on.as_ref(), scans, outer, order, explain_only)
        }
        TableRef::JoinUsing { left, right, kind, columns } => {
            super::from::join_using(txn, ctx, params, left, right, *kind, columns, scans, outer, order, explain_only)
        }
        TableRef::Lateral(inner) => from_rows(txn, ctx, params, inner, scans, outer, &[], explain_only),
        TableRef::Function { name, args, alias, columns } => {
            super::from::table_function(txn, ctx, params, name, args, alias, columns, outer, explain_only)
        }
        TableRef::Recursive { cte, alias, columns } => {
            let (s, rows) = super::cte::run(txn, ctx, params, cte, &alias.value, columns, outer, explain_only)?;
            Ok((s, rows, false))
        }
        TableRef::DataChange { kind, statement, alias, columns } => {
            super::from::data_change(txn, ctx, params, *kind, statement, alias, columns, explain_only)
        }
        TableRef::Join {
            left,
            right,
            kind,
            on,
        } => {
            let (ls, left_rows, lord) =
                from_rows(txn, ctx, params, left, scans, outer, order, explain_only)?;
            let (rs, right_rows, _) =
                from_rows(txn, ctx, params, right, scans, outer, &[], explain_only)?;
            let mut cols = ls.cols.clone();
            cols.extend(rs.cols.iter().cloned().map(|mut c| {
                if matches!(
                    kind,
                    JoinKind::Left | JoinKind::LeftException | JoinKind::Full
                ) {
                    c.nullable = true;
                }
                c
            }));
            if matches!(
                kind,
                JoinKind::Right | JoinKind::RightException | JoinKind::Full
            ) {
                for c in &mut cols[..ls.cols.len()] {
                    c.nullable = true;
                }
            }
            let schema = Rc::new(Schema { cols });
            if explain_only {
                let (l, r) = (estimated(txn, ctx, left, scans), estimated(txn, ctx, right, scans));
                let mut parts = Vec::new();
                if let Some(e) = on {
                    conjuncts(e, &mut parts);
                }
                let inputs = join::method::Inputs {
                    left: l,
                    right: r,
                    equality: join::equality_in(&parts, &ls, &rs),
                    hash_bytes: r.saturating_mul(48),
                    room: u64::try_from(ctx.charge().room()).unwrap_or(u64::MAX),
                    goal: ctx.plan.goal,
                    ..join::method::Inputs::default()
                };
                ctx.explain.push(format!("JOIN {kind:?} {} (est. {l} x {r} rows)", join::method::choose(&inputs).name()));
                return Ok((schema, Vec::new(), lord));
            }
            let mut ev = Evaluator { ctx, params };
            let rows = join::join(
                txn,
                &mut ev,
                &schema,
                &ls,
                &rs,
                &left_rows,
                &right_rows,
                *kind,
                on.as_ref(),
                outer,
            )?;
            Ok((
                schema,
                rows,
                lord && matches!(
                    kind,
                    JoinKind::Inner | JoinKind::Left | JoinKind::LeftException
                ),
            ))
        }
    }
}

/// Rows a `FROM` item is expected to give, for a plan made without
/// reading: a table's rows at the share its pushed predicates keep.
fn estimated(txn: &mut Txn<'_>, ctx: &SqlContext, t: &TableRef, scans: &[Scan]) -> u64 {
    use super::plan::{estimate, stats};
    match t {
        TableRef::Table { name, alias, .. } => {
            let alias = alias.as_ref().map_or_else(|| name.name.value.clone(), |a| a.value.clone());
            let Some(scan) = scans.iter().find(|s| s.alias == alias) else { return 1 };
            let Some(info) = &scan.info else {
                return scan.rows.as_ref().map_or(1, |r| r.len() as u64);
            };
            let rows = Member::open(txn, &info.desc, &info.member).map_or(0, |m| m.attr().record_count);
            let mut share = 1.0;
            for (col, op, v) in simple_predicates(&scan.preds) {
                let st = info.field(&col).and_then(|(_, f)| stats::get(info.member.object, &f.name.to_string()));
                share *= estimate::compare(st.as_ref(), op, estimate::constant(&v).as_ref());
            }
            let others = scan.preds.len().saturating_sub(simple_predicates(&scan.preds).len());
            share *= estimate::OTHER.powi(i32::try_from(others).unwrap_or(i32::MAX));
            let _ = ctx;
            estimate::rows(rows, share)
        }
        TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
            estimated(txn, ctx, left, scans).max(estimated(txn, ctx, right, scans))
        }
        TableRef::Lateral(inner) => estimated(txn, ctx, inner, scans),
        _ => 1000,
    }
}

/// The name of a select-list item.
fn item_name(e: &Expr, alias: Option<&crate::ast::Ident>, n: usize) -> String {
    if let Some(a) = alias {
        return a.value.clone();
    }
    match e {
        Expr::Column(c) => c.column.value.clone(),
        Expr::Default => "DEFAULT".into(),
        _ => format!("{n:05}"),
    }
}

#[allow(clippy::too_many_lines)]
fn select(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    sel: &Select,
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Produced>)> {
    // Conjuncts only a derived table needs go into it.
    if sel.where_clause.is_some() && sel.from.iter().any(|t| matches!(t, TableRef::Subquery { .. })) {
        let mut probe = Vec::new();
        for t in &sel.from {
            collect_scans(txn, ctx, t, true, &mut probe)?;
        }
        let elsewhere = |n: &str| probe.iter().any(|s| s.schema.cols.iter().any(|c| !c.hidden && c.name == n));
        if let Some(pushed) = super::plan::pushdown::push(sel, &elsewhere) {
            if explain_only {
                ctx.explain.push("PREDICATES pushed into derived tables".into());
            }
            return select(txn, ctx, params, &pushed, outer, order, explain_only);
        }
    }
    // The rows the query takes at most: for this select only.
    let limit = ctx.plan.limit.take();
    // The tables and what each may filter.
    let folded = fold_laterals(&sel.from);
    let from_items = folded.as_slice();
    let mut scans = Vec::new();
    // Which FROM item each scan belongs to.
    let mut item_of = Vec::new();
    for (i, t) in from_items.iter().enumerate() {
        collect_scans(txn, ctx, t, true, &mut scans)?;
        item_of.resize(scans.len(), i);
    }
    // A condition's distinct types follow their rules (`udt::typing`),
    // before a row is read.
    if super::udt::registry::any() {
        let joined = Schema { cols: scans.iter().flat_map(|s| s.schema.cols.iter().cloned()).collect() };
        let mut scope = Scope::of_env(outer);
        scope.frames.push(&joined);
        for c in sel.where_clause.iter().chain(&sel.having) {
            super::udt::typing::check(c, &scope)?;
        }
    }
    let mut rest = Vec::new();
    if let Some(w) = &sel.where_clause {
        let mut parts = Vec::new();
        conjuncts(w, &mut parts);
        for p in parts {
            match aliases_of(&p, &scans) {
                Some(set) if set.len() == 1 => {
                    let alias = set.into_iter().next().expect("one");
                    let s = scans.iter_mut().find(|s| s.alias == alias).expect("named");
                    if s.pushable {
                        s.preds.push(p);
                    } else {
                        rest.push(p);
                    }
                }
                _ => rest.push(p),
            }
        }
    }
    // Only a lone table reads in ORDER BY order.
    let order_hint: &[OrderItem] =
        if from_items.len() == 1 && sel.group_by.is_empty() && !sel.distinct {
            order
        } else {
            &[]
        };
    // FROM: each item with its joins, then the items joined depth first
    // with each remaining conjunct tested once the items it names are
    // bound (`join::product`); no cross product is built.
    let mut items = Vec::with_capacity(from_items.len());
    let mut item_charge = ctx.charge();
    let mut ordered = false;
    // A lone table whose rows go out as they are read stops at the limit.
    let streams = from_items.len() == 1
        && matches!(from_items[0], TableRef::Table { .. })
        && rest.is_empty()
        && sel.group_by.is_empty()
        && sel.having.is_none()
        && !sel.distinct
        && !sel.items.iter().any(|i| matches!(i, SelectItem::Expr(e, _) if has_aggregate(e) || super::olap::has_over(e) || has_subquery(e)))
        && !order.iter().any(|o| super::olap::has_over(&o.expr));
    // The columns a lone table must give, when they can be told.
    let needed = if from_items.len() == 1 && matches!(from_items[0], TableRef::Table { .. }) { needed_columns(sel, order) } else { None };
    for (i, t) in from_items.iter().enumerate() {
        ctx.plan.stop = if streams { limit } else { None };
        ctx.plan.columns.clone_from(&needed);
        let (s, r, o) = from_rows(txn, ctx, params, t, &scans, outer, order_hint, explain_only)?;
        ctx.plan.stop = None;
        ctx.plan.columns = None;
        for row in &r {
            item_charge.row(row)?;
        }
        if i == 0 {
            ordered = o;
        }
        items.push(Item { schema: s, rows: r });
    }
    if items.len() > 1 {
        ordered = false;
    }
    // A derived table's columns are not among the scans: its conjuncts
    // wait for every item.
    let last = from_items.len().saturating_sub(1);
    let derived = from_items.iter().any(has_derived);
    let mut levels: Vec<Vec<Expr>> = vec![Vec::new(); from_items.len().max(1)];
    for p in &rest {
        let level = match aliases_of(p, &scans) {
            Some(set) if !derived && !set.is_empty() => set
                .iter()
                .filter_map(|a| scans.iter().position(|s| &s.alias == a))
                .map(|i| item_of[i])
                .max()
                .unwrap_or(last),
            _ => last,
        };
        levels[level].push(p.clone());
    }
    if explain_only && items.len() > 1 {
        ctx.explain.push(format!("JOIN {} items depth first", items.len()));
    }
    if explain_only {
        if !rest.is_empty() {
            ctx.explain
                .push(format!("FILTER {} conjuncts after join", rest.len()));
        }
        if !sel.group_by.is_empty()
            || sel
                .items
                .iter()
                .any(|i| matches!(i, SelectItem::Expr(e, _) if has_aggregate(e)))
        {
            ctx.explain
                .push(format!("GROUP BY {} expressions", sel.group_by.len()));
        }
        if sel.distinct {
            ctx.explain.push("DISTINCT".into());
        }
        if !order.is_empty() && !ordered {
            ctx.explain.push("SORT".into());
        } else if ordered {
            ctx.explain.push("ORDER BY satisfied by the key".into());
        }
    }
    let mut ev = Evaluator { ctx, params };
    // The rows of FROM that pass the remaining WHERE.
    if items.is_empty() {
        items.push(Item {
            schema: Rc::new(Schema::default()),
            rows: vec![Vec::new()],
        });
    }
    let (schema, rows, mut charge) = join::product(txn, &mut ev, outer, items, &levels)?;
    drop(item_charge);
    // Grouping: one pass per grouping set.
    let grouped = !sel.group_by.is_empty()
        || sel
            .items
            .iter()
            .any(|i| matches!(i, SelectItem::Expr(e, _) if has_aggregate(e)))
        || sel.having.as_ref().is_some_and(has_aggregate);
    let with_sets = grouping::has_sets(&sel.group_by);
    let sets = if with_sets {
        grouping::sets(&sel.group_by)?
    } else {
        vec![grouping::GroupingSet { keys: sel.group_by.clone(), absent: Vec::new() }]
    };
    // Each output row's environment and grouping set.
    let mut envs: Vec<(Env, usize)> = Vec::new();
    if grouped {
        let rows = Rc::new(rows);
        for (si, set) in sets.iter().enumerate() {
            let having = sel.having.as_ref().map(|h| if with_sets { grouping::rewrite(h, set) } else { h.clone() });
            let mut groups: Vec<(Row, Vec<Row>)> = Vec::new();
            let mut index: HashMap<String, usize> = HashMap::new();
            for r in rows.iter() {
                ev.ctx.tick(txn)?;
                let env = outer.with(schema.clone(), r.clone());
                let mut key_vals = Vec::with_capacity(set.keys.len());
                for g in &set.keys {
                    key_vals.push(ev.eval(txn, g, &env)?);
                }
                let key = row_key(&key_vals);
                if let Some(&gi) = index.get(&key) {
                    groups[gi].1.push(r.clone());
                } else {
                    charge.add(2 * key.len() + super::mem::row_bytes(r) + 64)?;
                    index.insert(key, groups.len());
                    groups.push((r.clone(), vec![r.clone()]));
                }
            }
            if groups.is_empty() && set.keys.is_empty() {
                // An aggregate over no rows is one row.
                groups.push((vec![Value::Null; schema.cols.len()], Vec::new()));
            }
            for (first, members) in groups {
                ev.ctx.tick(txn)?;
                let mut env = outer.with(schema.clone(), first);
                env.group = Some(Rc::new(members));
                if let Some(h) = &having
                    && ev.truth(txn, h, &env)? != Some(true)
                {
                    continue;
                }
                envs.push((env, si));
            }
        }
    } else {
        envs = rows
            .into_iter()
            .map(|r| (outer.with(schema.clone(), r), 0))
            .collect();
    }
    // The select list, its columns typed in the scope of the rows.
    let mut scope = Scope::of_env(outer);
    scope.frames.push(schema.as_ref());
    scope.sequences = super::restype::sequences(
        txn,
        ev.ctx,
        sel.items.iter().filter_map(|i| if let SelectItem::Expr(e, _) = i { Some(e) } else { None }),
    );
    scope.variables = super::restype::variables(
        txn,
        ev.ctx,
        &scope,
        sel.items.iter().filter_map(|i| if let SelectItem::Expr(e, _) = i { Some(e) } else { None }),
    );
    let mut out_cols: Vec<ColInfo> = Vec::new();
    let mut plan: Vec<(Expr, usize, Option<SqlType>)> = Vec::new();
    let mut n = 0;
    for item in &sel.items {
        match item {
            SelectItem::Star => {
                for (i, c) in schema.cols.iter().enumerate() {
                    if !c.hidden {
                        n += 1;
                        out_cols.push(ColInfo {
                            table: None,
                            ..c.clone()
                        });
                        plan.push((column_at(&schema, i), i, None));
                    }
                }
            }
            SelectItem::TableStar(t) => {
                let mut any = false;
                for (i, c) in schema.cols.iter().enumerate() {
                    if !c.hidden && c.table.as_deref() == Some(t.value.as_str()) {
                        any = true;
                        n += 1;
                        out_cols.push(ColInfo {
                            table: None,
                            ..c.clone()
                        });
                        plan.push((column_at(&schema, i), i, None));
                    }
                }
                if !any {
                    return Err(SqlError::ColumnNotFound {
                        name: format!("{}.*", t.value),
                    });
                }
            }
            SelectItem::Expr(e, alias) => {
                n += 1;
                // A grouping expression another set leaves out is null.
                let absent = with_sets && sets.iter().any(|s| s.absent.contains(e));
                let found = match e {
                    Expr::Column(c) => schema.find(c.table.as_ref().map(|t| t.value.as_str()), &c.column.value)?,
                    _ => None,
                };
                let (field, nullable, ty) = match found {
                    // A column of the row keeps its field.
                    Some(i) if schema.cols[i].field.is_some() => {
                        (schema.cols[i].field.clone(), schema.cols[i].nullable || absent, None)
                    }
                    _ => match type_in(e, &scope)? {
                        Some(t) => {
                            let null = t.nullable || absent;
                            let t = t.nullable(null);
                            (Some(super::restype::field_of(&t)), t.nullable, Some(t))
                        }
                        None => (None, true, None),
                    },
                };
                out_cols.push(ColInfo {
                    name: item_name(e, alias.as_ref(), n),
                    table: None,
                    nullable,
                    field,
                    hidden: false,
                });
                plan.push((e.clone(), usize::MAX, ty));
            }
        }
    }
    // ORDER BY items with an OLAP specification are worked out with the
    // select list, as hidden columns.
    let mut order_items: Vec<OrderItem> = order.to_vec();
    for (k, item) in order_items.iter_mut().enumerate() {
        if super::olap::has_over(&item.expr) {
            let name = format!("*O{k}");
            out_cols.push(ColInfo { name: name.clone(), table: None, field: None, nullable: true, hidden: true });
            plan.push((item.expr.clone(), usize::MAX, None));
            item.expr = Expr::Column(ColumnRef {
                table: None,
                column: crate::ast::Ident { value: name, delimited: true },
            });
        }
    }
    let out_schema = Rc::new(Schema { cols: out_cols });
    // The select list of each grouping set.
    let set_plans: Vec<Vec<Expr>> = if with_sets {
        sets.iter().map(|s| plan.iter().map(|(e, _, _)| grouping::rewrite(e, s)).collect()).collect()
    } else {
        Vec::new()
    };
    // OLAP specifications over the rows, before the select list.
    let mut overs = Vec::new();
    for (e, _, _) in &plan {
        super::olap::collect(e, &mut overs);
    }
    let saved = std::mem::take(&mut ev.ctx.windows);
    let windows = if overs.is_empty() {
        Vec::new()
    } else {
        let only: Vec<Env> = envs.iter().map(|(e, _)| e.clone()).collect();
        match super::olap::compute(txn, &mut ev, &only, &overs) {
            Ok(w) => w,
            Err(e) => {
                ev.ctx.windows = saved;
                return Err(e);
            }
        }
    };
    let mut produced = Vec::with_capacity(envs.len());
    let r = (|| -> Result<()> {
        for (ri, (env, si)) in envs.into_iter().enumerate() {
            ev.ctx.tick(txn)?;
            if let Some(w) = windows.get(ri) {
                ev.ctx.windows.clone_from(w);
            }
            let mut row = Vec::with_capacity(plan.len());
            for (pi, (e, direct, ty)) in plan.iter().enumerate() {
                if *direct == usize::MAX {
                    let e = if with_sets { &set_plans[si][pi] } else { e };
                    let mode = ev.ctx.decfloat_rounding;
                    let v = ev.eval(txn, e, &env).and_then(|v| match ty {
                        Some(t) => super::restype::conform(v, t, mode),
                        None => Ok(v),
                    });
                    row.push(super::cast::mapped(ev.ctx, v)?);
                } else {
                    row.push(env.frames.last().expect("row").1[*direct].clone());
                }
            }
            charge.row(&row)?;
            produced.push(Produced {
                out: row,
                source: env,
            });
        }
        Ok(())
    })();
    ev.ctx.windows = saved;
    r?;
    if sel.distinct {
        distinct(&mut charge, &mut produced)?;
    }
    if !order_items.is_empty() && !ordered {
        let joined = from_items.len() > 1 || sel.from.iter().any(|f| matches!(f, TableRef::Join { .. } | TableRef::JoinUsing { .. }));
        let n = produced.len() as u64;
        ev.ctx.dbmon.note(crate::dbmon::plan::PlanEvent::Sort { rows: n, reason: if joined { "F2" } else { "F7" } });
        order_rows(txn, ev.ctx, params, &out_schema, &mut produced, &order_items)?;
    }
    Ok((out_schema, produced))
}

fn column_at(schema: &Schema, i: usize) -> Expr {
    let c = &schema.cols[i];
    Expr::Column(ColumnRef {
        table: c.table.as_ref().map(|t| crate::ast::Ident::new(t)),
        column: crate::ast::Ident {
            value: c.name.clone(),
            delimited: true,
        },
    })
}

/// Sort produced rows by the `ORDER BY` items: a number is a position in
/// the select list, a name is looked up in the output row first, then
/// the source row. Past the job's row budget the sort spills runs to
/// its temporary storage and merges them.
fn order_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    out_schema: &Rc<Schema>,
    produced: &mut Vec<Produced>,
    order: &[OrderItem],
) -> Result<()> {
    // A number past the select list is refused before any row is read.
    let visible = out_schema.cols.iter().filter(|c| !c.hidden).count();
    for item in order {
        if let Expr::Literal(Literal::Num(n)) = &item.expr
            && n.parse::<usize>().map_or(true, |p| p == 0 || p > visible)
        {
            return Err(SqlError::OrderByNumber { n: n.clone() });
        }
    }
    let mut ev = Evaluator { ctx, params };
    let mut keys: Vec<Vec<Value>> = Vec::with_capacity(produced.len());
    for p in produced.iter() {
        ev.ctx.tick(txn)?;
        let mut env = p.source.clone();
        env.frames.push((out_schema.clone(), p.out.clone()));
        let mut k = Vec::with_capacity(order.len());
        for item in order {
            let v = match &item.expr {
                Expr::Literal(Literal::Num(n)) => {
                    let pos: usize = n.parse().map_err(|_| SqlError::Syntax {
                        line: 0,
                        col: 0,
                        found: n.clone(),
                        expected: "a select-list position".into(),
                    })?;
                    let visible = out_schema.cols.iter().filter(|c| !c.hidden).count();
                    if pos == 0 || pos > visible {
                        return Err(SqlError::OrderByNumber { n: n.clone() });
                    }
                    p.out[pos - 1].clone()
                }
                e => ev.eval(txn, e, &env)?,
            };
            k.push(v);
        }
        keys.push(k);
    }
    // Nulls sort high unless told otherwise.
    let specs: Vec<KeySpec> = order
        .iter()
        .map(|item| KeySpec {
            desc: item.desc,
            nulls_first: item.nulls_first.unwrap_or(item.desc),
        })
        .collect();
    let mut sorter = Sorter::new(specs, ctx.sort_budget);
    for (k, p) in keys.into_iter().zip(produced.drain(..)) {
        sorter.push(&mut *ctx.spill, k, p)?;
    }
    let (in_order, _runs) = sorter.finish(&mut *ctx.spill)?;
    *produced = in_order;
    Ok(())
}

/// The rows of one table that satisfy `cond`, with their RRNs first:
/// what `UPDATE` and `DELETE` work through.
pub fn matching_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableInfo,
    alias: &str,
    cond: Option<&Expr>,
    outer: &Env,
) -> Result<(Rc<Schema>, Vec<Row>)> {
    let schema = Rc::new(table_schema(t, alias));
    let mut scans = vec![Scan {
        info: Some(t.clone()),
        rows: None,
        alias: alias.to_string(),
        schema: schema.clone(),
        preds: Vec::new(),
        pushable: true,
    }];
    let mut rest = Vec::new();
    if let Some(w) = cond {
        let mut parts = Vec::new();
        conjuncts(w, &mut parts);
        for p in parts {
            if aliases_of(&p, &scans).is_some_and(|s| s.len() <= 1) {
                scans[0].preds.push(p);
            } else {
                rest.push(p);
            }
        }
    }
    let (mut rows, _) = scan_table(txn, ctx, params, &scans[0], outer, &[], false)?;
    if !rest.is_empty() {
        let mut ev = Evaluator { ctx, params };
        let mut kept = Vec::new();
        for r in rows {
            ev.ctx.tick(txn)?;
            let env = outer.with(schema.clone(), r);
            let mut ok = true;
            for p in &rest {
                if ev.truth(txn, p, &env)? != Some(true) {
                    ok = false;
                    break;
                }
            }
            if ok {
                kept.push(env.frames.last().expect("row").1.clone());
            }
        }
        rows = kept;
    }
    Ok((schema, rows))
}

/// Column descriptions of a table, for `describe()`.
#[must_use]
pub fn describe_table(t: &TableInfo) -> Vec<ColumnDesc> {
    t.format
        .format
        .fields
        .iter()
        .map(|f: &FieldDef| ColumnDesc {
            name: f.name.to_string(),
            data_type: f.udt.clone().unwrap_or_else(|| sql_type(f)),
            nullable: f.allow_null,
        })
        .collect()
}

/// The visible result columns of a query and its rows, as `CREATE TABLE
/// AS` takes them.
pub(crate) fn columns_and_rows(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
) -> Result<(Vec<ColInfo>, Vec<Row>)> {
    let (schema, produced) = run_body(txn, ctx, params, q, &Env::default(), false)?;
    let visible: Vec<usize> = schema.cols.iter().enumerate().filter(|(_, c)| !c.hidden).map(|(i, _)| i).collect();
    let cols = visible.iter().map(|&i| schema.cols[i].clone()).collect();
    let rows = produced.into_iter().map(|p| visible.iter().map(|&i| p.out[i].clone()).collect()).collect();
    Ok((cols, rows))
}

/// The result columns of a query as planned, without reading its rows.
pub(crate) fn columns_and_rows_planned(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
) -> Result<(Vec<ColInfo>, Vec<Row>)> {
    let kept = std::mem::take(&mut ctx.explain);
    let r = run_body(txn, ctx, params, q, &Env::default(), true);
    ctx.explain = kept;
    let (schema, _) = r?;
    Ok((schema.cols.iter().filter(|c| !c.hidden).cloned().collect(), Vec::new()))
}

/// `FROM a, b, LATERAL (...) x, c`: the items before a lateral one joined
/// to it, so it runs once for each of their rows.
fn fold_laterals(from: &[TableRef]) -> Vec<TableRef> {
    let mut out: Vec<TableRef> = Vec::with_capacity(from.len());
    for t in from {
        if matches!(t, TableRef::Lateral(_)) && !out.is_empty() {
            let mut left = out.remove(0);
            for x in out.drain(..) {
                left = TableRef::Join { left: Box::new(left), right: Box::new(x), kind: JoinKind::Cross, on: None };
            }
            out.push(TableRef::Join { left: Box::new(left), right: Box::new(t.clone()), kind: JoinKind::Cross, on: None });
        } else {
            out.push(t.clone());
        }
    }
    out
}
