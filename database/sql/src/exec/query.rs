// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running a query: scans with access path selection, joins, grouping,
//! projection, `DISTINCT`, ordering and row limits. Scans read their
//! paths in batches; `ORDER BY` is an external merge sort that spills
//! past the job's row budget (`sort`); the other steps hold their rows.

use std::collections::{HashMap, HashSet};
use std::ops::Bound;
use std::rc::Rc;

use db::file::{self, FileKind, MemberEntry};
use db::format::FormatDef;
use db::member::{Core, Member, Slot};
use db::util::{Order, each_record};
use dds::FieldDef;
use store::obj::ObjectId;
use store::txn::Txn;

use super::eval::{ColInfo, Env, Evaluator, Row, Schema, has_aggregate, sql_type};
use super::sort::{KeySpec, Sorter};
use super::{ColumnDesc, ResultSet, SqlContext, TableInfo};
use crate::ast::{
    CmpOp, ColumnRef, Expr, JoinKind, Literal, OrderItem, Query, QueryBody, Select, SelectItem,
    TableRef, columns_of,
};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// Path entries a keyed scan reads per batch.
const KEY_BATCH: usize = 256;

/// A table in `FROM`, with its schema and the predicates pushed to it.
struct Scan {
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
            name: f.name.to_string(),
            table: Some(alias.to_string()),
            field: Some(f.clone()),
            nullable: f.allow_null,
            hidden: false,
        });
    }
    Schema { cols }
}

/// A record as a row: RRN first.
pub fn row_of(fmt: &FormatDef, slot: &Slot) -> Result<Row> {
    let mut row = Vec::with_capacity(fmt.format.fields.len() + 1);
    row.push(Value::int(i64::try_from(slot.rrn).expect("fits")));
    for (i, f) in fmt.format.fields.iter().enumerate() {
        let v = db::record::read_field(f, &slot.record, &slot.null_map, fmt.null_index(i))
            .map_err(|e| SqlError::Conversion {
                what: e.to_string(),
            })?;
        row.push(Value::from_field(f, &v)?);
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

fn run_body(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    q: &Query,
    outer: &Env,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Produced>)> {
    let (schema, mut produced) = body(txn, ctx, params, &q.body, outer, q, explain_only)?;
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
            for r in rows {
                if r.len() != width {
                    return Err(SqlError::ValueCount {
                        what: "VALUES rows differ in length".into(),
                    });
                }
                let mut row = Vec::with_capacity(r.len());
                for e in r {
                    row.push(ev.eval(txn, e, outer)?);
                }
                out.push(row);
            }
            let schema = Rc::new(Schema {
                cols: (0..width)
                    .map(|i| ColInfo {
                        name: format!("{:05}", i + 1),
                        table: None,
                        field: None,
                        nullable: true,
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
            let schema = Rc::new(Schema {
                cols: ls.cols.iter().filter(|c| !c.hidden).cloned().collect(),
            });
            if !*all {
                let mut seen = HashSet::new();
                all_rows.retain(|p| seen.insert(row_key(&p.out)));
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

fn has_subquery(e: &Expr) -> bool {
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
        TableRef::Subquery { .. } => {}
        TableRef::Join {
            left, right, kind, ..
        } => {
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
}

/// The paths over a table: its own and those of dependent logical files
/// that select nothing and rename nothing.
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
        });
    }
    if t.desc.attr.kind == FileKind::Logical {
        return out;
    }
    for d in &t.desc.dependents {
        if d.base_member != t.member.object {
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
        });
    }
    out
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

fn choose(cands: &[Candidate], preds: &[Expr], order: &[OrderItem], alias: &str) -> Option<Choice> {
    let mut best: Option<Choice> = None;
    for (ci, c) in cands.iter().enumerate() {
        let mut eq = Vec::new();
        let (mut lo, mut hi) = (None, None);
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
        for (name, descend) in &c.keys {
            if let Some((_, _, v)) = simple
                .iter()
                .find(|(n, op, _)| n == name && *op == CmpOp::Eq)
            {
                eq.push(v.clone());
                continue;
            }
            if !descend {
                for (n, op, v) in &simple {
                    if n != name {
                        continue;
                    }
                    match op {
                        CmpOp::Gt | CmpOp::Ge if lo.is_none() => lo = Some(v.clone()),
                        CmpOp::Lt | CmpOp::Le if hi.is_none() => hi = Some(v.clone()),
                        _ => {}
                    }
                }
            }
            break;
        }
        let ordered = order_matches(c, order, alias);
        let score = u32::try_from(eq.len()).expect("few") * 10
            + u32::from(lo.is_some() || hi.is_some()) * 5
            + u32::from(ordered.is_some()) * 3;
        if score > 0 && best.as_ref().is_none_or(|b| score > b.score) {
            best = Some(Choice {
                cand: ci,
                eq,
                lo,
                hi,
                ordered,
                score,
            });
        }
    }
    best
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

/// Scan one table: by its chosen path within bounds, else every record.
#[allow(clippy::too_many_lines)]
fn scan_table(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    scan: &Scan,
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Vec<Row>, bool)> {
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
        let kept = filter_rows(txn, &mut ev, scan, outer, all)?;
        return Ok((kept, false));
    };
    let cands = candidates(txn, t);
    let choice = choose(&cands, &scan.preds, order, &scan.alias);
    let label = format!("{}/{} ({})", t.desc.library_name, t.desc.name(), scan.alias);
    let base_pf: Option<Member> = if t.desc.attr.kind == FileKind::Logical {
        None
    } else {
        Some(Member::open(txn, &t.desc, &t.member)?)
    };
    if let Some(ch) = choice {
        let c = &cands[ch.cand];
        ordered = ch.ordered.is_some();
        if explain_only {
            ev.ctx.explain.push(format!(
                "SCAN {label} keyed via {} eq={} range={} {}",
                c.label,
                ch.eq.len(),
                if ch.lo.is_some() || ch.hi.is_some() {
                    "yes"
                } else {
                    "no"
                },
                if ordered { "ordered" } else { "" }
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
        let lo = encode(&lo_vals)?;
        let mut hi = encode(&hi_vals)?;
        hi.resize(usize::from(c.fmt.key_len), 0xFF);
        if explain_only {
            return Ok((Vec::new(), ordered));
        }
        let desc = file::describe(txn, c.file)?;
        let mut pm = Member::open(txn, &desc, &c.member)?;
        pm.ensure_path(txn, &mut |_, _| {})?;
        let backward = ch.ordered == Some(true);
        // The path in batches, each read as far as the bounds allow.
        let path = pm.path().ok_or_else(|| SqlError::Database {
            what: format!("{} has no access path", c.label),
            msgid: None,
        })?;
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
                let slots = match &base_pf {
                    Some(pf) => pf
                        .read(txn, k.rrn)?
                        .filter(|s| !s.deleted)
                        .into_iter()
                        .collect(),
                    None => pm.read_entries(txn, k)?,
                };
                for s in slots {
                    rows.push(row_of(&t.format, &s)?);
                }
            }
            batch = if backward {
                path.scan_back_from(txn, Bound::Excluded(&last), KEY_BATCH)?
            } else {
                path.scan_from(txn, Bound::Excluded(&last), KEY_BATCH)?
            };
        }
    } else {
        if explain_only {
            ev.ctx.explain.push(format!("SCAN {label} full"));
            return Ok((Vec::new(), false));
        }
        if let Some(pf) = base_pf {
            for s in pf.scan(txn, 1, usize::MAX)? {
                rows.push(row_of(&t.format, &s)?);
            }
        } else {
            let mut m = Member::open(txn, &t.desc, &t.member)?;
            let mut slots = Vec::new();
            each_record(txn, &mut m, &Order::default(), &mut |_, s| {
                slots.push(s);
                Ok(true)
            })?;
            for s in &slots {
                rows.push(row_of(&t.format, s)?);
            }
            let _ = Core::load;
        }
    }
    // The pushed predicates, all of them: bounds may be loose.
    let rows = filter_rows(txn, &mut ev, scan, outer, rows)?;
    Ok((rows, ordered))
}

/// Keep the rows that pass every pushed predicate.
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
fn from_rows(
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
            let cols = visible
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
            let rows = rows
                .into_iter()
                .map(|r| visible.iter().map(|&i| r[i].clone()).collect())
                .collect();
            if explain_only {
                ctx.explain.push(format!("DERIVED TABLE ({})", alias.value));
            }
            Ok((Rc::new(Schema { cols }), rows, false))
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
                ctx.explain.push(format!("JOIN {kind:?} nested loop"));
                return Ok((schema, Vec::new(), lord));
            }
            let rows = join(
                txn,
                ctx,
                params,
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

#[allow(clippy::too_many_arguments, clippy::many_single_char_names)]
fn join(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    schema: &Rc<Schema>,
    ls: &Schema,
    rs: &Schema,
    left_rows: &[Row],
    right_rows: &[Row],
    kind: JoinKind,
    on: Option<&Expr>,
    outer: &Env,
) -> Result<Vec<Row>> {
    let mut ev = Evaluator { ctx, params };
    let mut out = Vec::new();
    let lnull = vec![Value::Null; ls.cols.len()];
    let rnull = vec![Value::Null; rs.cols.len()];
    let mut right_matched = vec![false; right_rows.len()];
    for l in left_rows {
        let mut matched = false;
        for (ri, r) in right_rows.iter().enumerate() {
            let mut combined = l.clone();
            combined.extend(r.iter().cloned());
            let hit = match on {
                None => true,
                Some(e) => {
                    let env = outer.with(schema.clone(), combined.clone());
                    ev.truth(txn, e, &env)? == Some(true)
                }
            };
            if hit {
                matched = true;
                right_matched[ri] = true;
                if matches!(
                    kind,
                    JoinKind::Inner
                        | JoinKind::Cross
                        | JoinKind::Left
                        | JoinKind::Right
                        | JoinKind::Full
                ) {
                    out.push(combined);
                }
            }
        }
        if !matched
            && matches!(
                kind,
                JoinKind::Left | JoinKind::LeftException | JoinKind::Full
            )
        {
            let mut row = l.clone();
            row.extend(rnull.iter().cloned());
            out.push(row);
        }
    }
    if matches!(
        kind,
        JoinKind::Right | JoinKind::RightException | JoinKind::Full
    ) {
        for (ri, r) in right_rows.iter().enumerate() {
            if !right_matched[ri] {
                let mut row = lnull.clone();
                row.extend(r.iter().cloned());
                out.push(row);
            }
        }
    }
    Ok(out)
}

/// The name of a select-list item.
fn item_name(e: &Expr, alias: Option<&crate::ast::Ident>, n: usize) -> String {
    if let Some(a) = alias {
        return a.value.clone();
    }
    match e {
        Expr::Column(c) => c.column.value.clone(),
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
    // The tables and what each may filter.
    let mut scans = Vec::new();
    for t in &sel.from {
        collect_scans(txn, ctx, t, true, &mut scans)?;
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
        if sel.from.len() == 1 && sel.group_by.is_empty() && !sel.distinct {
            order
        } else {
            &[]
        };
    // FROM: the cross product of the items, each with its joins.
    let mut schema = Rc::new(Schema::default());
    let mut rows: Vec<Row> = vec![Vec::new()];
    let mut ordered = false;
    for (i, t) in sel.from.iter().enumerate() {
        let (s, r, o) = from_rows(txn, ctx, params, t, &scans, outer, order_hint, explain_only)?;
        if i == 0 {
            schema = s;
            rows = r;
            ordered = o;
        } else {
            let mut cols = schema.cols.clone();
            cols.extend(s.cols.iter().cloned());
            schema = Rc::new(Schema { cols });
            let mut product = Vec::with_capacity(rows.len() * r.len());
            for l in &rows {
                for x in &r {
                    let mut row = l.clone();
                    row.extend(x.iter().cloned());
                    product.push(row);
                }
            }
            rows = product;
            ordered = false;
        }
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
    // The remaining WHERE.
    if !rest.is_empty() {
        let mut kept = Vec::with_capacity(rows.len());
        for r in rows {
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
    // Grouping.
    let grouped = !sel.group_by.is_empty()
        || sel
            .items
            .iter()
            .any(|i| matches!(i, SelectItem::Expr(e, _) if has_aggregate(e)))
        || sel.having.as_ref().is_some_and(has_aggregate);
    let mut envs: Vec<Env> = Vec::new();
    if grouped {
        let mut groups: Vec<(Row, Vec<Row>)> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for r in rows {
            let env = outer.with(schema.clone(), r.clone());
            let mut key_vals = Vec::with_capacity(sel.group_by.len());
            for g in &sel.group_by {
                key_vals.push(ev.eval(txn, g, &env)?);
            }
            let key = row_key(&key_vals);
            if let Some(&gi) = index.get(&key) {
                groups[gi].1.push(r);
            } else {
                index.insert(key, groups.len());
                groups.push((r.clone(), vec![r]));
            }
        }
        if groups.is_empty() && sel.group_by.is_empty() {
            // An aggregate over no rows is one row.
            groups.push((vec![Value::Null; schema.cols.len()], Vec::new()));
        }
        for (first, members) in groups {
            let mut env = outer.with(schema.clone(), first);
            env.group = Some(Rc::new(members));
            if let Some(h) = &sel.having
                && ev.truth(txn, h, &env)? != Some(true)
            {
                continue;
            }
            envs.push(env);
        }
    } else {
        envs = rows
            .into_iter()
            .map(|r| outer.with(schema.clone(), r))
            .collect();
    }
    // The select list.
    let mut out_cols: Vec<ColInfo> = Vec::new();
    let mut plan: Vec<(Expr, usize)> = Vec::new();
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
                        plan.push((column_at(&schema, i), i));
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
                        plan.push((column_at(&schema, i), i));
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
                let field = match e {
                    Expr::Column(c) => {
                        let table = c.table.as_ref().map(|t| t.value.as_str());
                        schema
                            .find(table, &c.column.value)?
                            .and_then(|i| schema.cols[i].field.clone())
                    }
                    _ => None,
                };
                out_cols.push(ColInfo {
                    name: item_name(e, alias.as_ref(), n),
                    table: None,
                    nullable: field.as_ref().is_none_or(|f| f.allow_null),
                    field,
                    hidden: false,
                });
                plan.push((e.clone(), usize::MAX));
            }
        }
    }
    let out_schema = Rc::new(Schema { cols: out_cols });
    let mut produced = Vec::with_capacity(envs.len());
    for env in envs {
        let mut row = Vec::with_capacity(plan.len());
        for (e, direct) in &plan {
            if *direct == usize::MAX {
                row.push(ev.eval(txn, e, &env)?);
            } else {
                row.push(env.frames.last().expect("row").1[*direct].clone());
            }
        }
        produced.push(Produced {
            out: row,
            source: env,
        });
    }
    if sel.distinct {
        let mut seen = HashSet::new();
        produced.retain(|p| seen.insert(row_key(&p.out)));
    }
    if !order.is_empty() && !ordered {
        order_rows(txn, ev.ctx, params, &out_schema, &mut produced, order)?;
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
    let mut ev = Evaluator { ctx, params };
    let mut keys: Vec<Vec<Value>> = Vec::with_capacity(produced.len());
    for p in produced.iter() {
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
                    p.out.get(pos.wrapping_sub(1)).cloned().ok_or_else(|| {
                        SqlError::ColumnNotFound {
                            name: format!("ORDER BY {pos}"),
                        }
                    })?
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
            data_type: sql_type(f),
            nullable: f.allow_null,
        })
        .collect()
}
