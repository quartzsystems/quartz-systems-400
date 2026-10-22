// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSERT`, `UPDATE` and `DELETE` through the open data path, so the
//! journal sees what it would from RPG.

use std::rc::Rc;

use db::odp::Odp;
use db::odp::Usage;
use db::record;
use db::{KeyArg, Value as DbValue};
use dds::{DefaultValue, FieldDef};
use store::txn::Txn;

use super::eval::{Env, Evaluator, Row, Schema};
use super::query::{matching_rows, row_of, run_query};
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
/// identity column's next value and expression defaults.
fn fill_generated(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    t: &TableInfo,
    o: &mut Odp,
    per_field: &mut [Option<Value>],
) -> Result<()> {
    for (fi, f) in t.format.format.fields.iter().enumerate() {
        if let Some(id) = &f.identity {
            if per_field[fi].is_some() {
                if id.always {
                    return Err(SqlError::IdentityValue {
                        column: f.name.to_string(),
                    });
                }
                continue;
            }
            let v = o.next_identity(txn)?.unwrap_or(id.start);
            per_field[fi] = Some(Value::int(v));
        } else if per_field[fi].is_none() && matches!(f.default, Some(DefaultValue::Expr(_))) {
            let mut ev = Evaluator { ctx, params };
            per_field[fi] = default_value(txn, &mut ev, f)?;
        }
    }
    Ok(())
}

/// Write `values` (one per field, `None` for the default) into a record.
pub fn build_record(
    t: &TableInfo,
    values: &[Option<Value>],
    base: Option<(&[u8], &[u8])>,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let fmt = &t.format;
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
        let dbv = v.to_field(f)?;
        record::write_field(f, &mut rec, &dbv).map_err(|e| SqlError::Conversion {
            what: e.to_string(),
        })?;
        if let Some(ni) = fmt.null_index(i) {
            record::set_null(&mut nulls, ni, matches!(dbv, DbValue::Null));
        }
    }
    Ok((rec, nulls))
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
                for e in r {
                    row.push(match e {
                        Expr::Default => None,
                        e => Some(ev.eval(txn, e, &env)?),
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
    let mut o = ctx.open(txn, &t, Usage::OUTPUT)?;
    let mut n = 0u64;
    let mut result = Ok(());
    for r in rows {
        let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
        for (k, &fi) in idx.iter().enumerate() {
            per_field[fi] = r.get(k).cloned().flatten();
        }
        let written = fill_generated(txn, ctx, params, &t, &mut o, &mut per_field)
            .and_then(|()| build_record(&t, &per_field, None))
            .and_then(|(rec, nulls)| o.write(txn, &rec, &nulls).map_err(SqlError::from));
        if let Err(e) = written {
            result = Err(e);
            break;
        }
        n += 1;
    }
    o.close(txn);
    result.map(|()| n)
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
            for e in &a.values {
                v.push(match e {
                    Expr::Default => None,
                    e => Some(ev.eval(txn, e, env)?),
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
    let t = ctx.resolve_table(txn, &u.table)?;
    let alias = u
        .alias
        .as_ref()
        .map_or_else(|| u.table.name.value.clone(), |a| a.value.clone());
    let (schema, rows) = match &u.where_clause {
        Where::All => matching_rows(txn, ctx, params, &t, &alias, None, &Env::default())?,
        Where::Cond(c) => matching_rows(txn, ctx, params, &t, &alias, Some(c), &Env::default())?,
        Where::CurrentOf(c) => current_row(txn, ctx, &t, &alias, &c.value)?,
    };
    let mut o = ctx.open(txn, &t, Usage::UPDATE)?;
    let result = change_rows(txn, ctx, params, &t, &schema, rows, &mut o, Some(&u.set));
    o.close(txn);
    result
}

/// `DELETE`: rows deleted.
pub fn delete(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    d: &Delete,
) -> Result<u64> {
    let t = ctx.resolve_table(txn, &d.table)?;
    let alias = d
        .alias
        .as_ref()
        .map_or_else(|| d.table.name.value.clone(), |a| a.value.clone());
    let (schema, rows) = match &d.where_clause {
        Where::All => matching_rows(txn, ctx, params, &t, &alias, None, &Env::default())?,
        Where::Cond(c) => matching_rows(txn, ctx, params, &t, &alias, Some(c), &Env::default())?,
        Where::CurrentOf(c) => current_row(txn, ctx, &t, &alias, &c.value)?,
    };
    let mut o = ctx.open(txn, &t, Usage::UPDATE)?;
    let result = change_rows(txn, ctx, params, &t, &schema, rows, &mut o, None);
    o.close(txn);
    result
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
        Some(s) if !s.deleted => vec![row_of(&t.format, &s)?],
        _ => Vec::new(),
    };
    Ok((schema, rows))
}

/// Update (with `set`) or delete each row by RRN through the path.
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
) -> Result<u64> {
    let mut ev = Evaluator { ctx, params };
    let mut n = 0u64;
    for r in rows {
        let rrn = u64::try_from(r[0].as_i64()?).expect("positive");
        let Some(current) = o.chain(txn, &KeyArg::Rrn(rrn), true)? else {
            continue;
        };
        match set {
            None => {
                o.delete(txn)?;
            }
            Some(set) => {
                // Evaluate against the record as it is now.
                let slot = db::Slot {
                    rrn,
                    record: current.data.clone(),
                    null_map: current.null_map.clone(),
                    deleted: false,
                };
                let row = row_of(&t.format, &slot)?;
                let env = Env::default().with(schema.clone(), row);
                let assigned = assignments(txn, &mut ev, t, set, &env)?;
                let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
                for (i, v) in assigned {
                    per_field[i] = v;
                }
                let (rec, nulls) =
                    build_record(t, &per_field, Some((&current.data, &current.null_map)))?;
                o.update(txn, &rec, &nulls)?;
            }
        }
        n += 1;
    }
    Ok(n)
}

/// `MERGE`: for each source row, the matching target rows take the
/// first `WHEN MATCHED` whose condition holds; a row without a match
/// takes the first `WHEN NOT MATCHED`. Returns rows changed.
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn merge(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    m: &crate::ast::Merge,
) -> Result<u64> {
    use crate::ast::{MergeAction, Query, QueryBody, Select, SelectItem};
    let t = ctx.resolve_table(txn, &m.target)?;
    let alias = m
        .alias
        .as_ref()
        .map_or_else(|| m.target.name.value.clone(), |a| a.value.clone());
    // The source as a query of its own.
    let src_alias = match &m.source {
        crate::ast::TableRef::Table { name, alias, .. } => alias
            .as_ref()
            .map_or_else(|| name.name.value.clone(), |a| a.value.clone()),
        crate::ast::TableRef::Subquery { alias, .. } => alias.value.clone(),
        crate::ast::TableRef::Join { .. } => {
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
    let mut o = ctx.open(txn, &t, Usage::ALL)?;
    let mut n = 0u64;
    let result = (|| {
        for srow in src.rows {
            let outer = Env::default().with(src_schema.clone(), srow);
            let (tschema, matches) =
                matching_rows(txn, ctx, params, &t, &alias, Some(&m.on), &outer)?;
            if matches.is_empty() {
                for a in &m.actions {
                    let MergeAction::Insert {
                        cond,
                        columns,
                        values,
                    } = a
                    else {
                        continue;
                    };
                    let mut ev = Evaluator { ctx, params };
                    if let Some(c) = cond
                        && ev.truth(txn, c, &outer)? != Some(true)
                    {
                        continue;
                    }
                    let idx = column_indexes(&t, columns)?;
                    if values.len() != idx.len() {
                        return Err(SqlError::ValueCount {
                            what: format!("{} values for {} columns", values.len(), idx.len()),
                        });
                    }
                    let mut per_field: Vec<Option<Value>> =
                        vec![None; t.format.format.fields.len()];
                    for (k, &fi) in idx.iter().enumerate() {
                        per_field[fi] = match &values[k] {
                            Expr::Default => None,
                            e => Some(ev.eval(txn, e, &outer)?),
                        };
                    }
                    let (rec, nulls) = build_record(&t, &per_field, None)?;
                    o.write(txn, &rec, &nulls)?;
                    n += 1;
                    break;
                }
                continue;
            }
            for trow in matches {
                let rrn = u64::try_from(trow[0].as_i64()?).expect("positive");
                let env = outer.with(tschema.clone(), trow);
                for a in &m.actions {
                    let mut ev = Evaluator { ctx, params };
                    match a {
                        MergeAction::Update { cond, set } => {
                            if let Some(c) = cond
                                && ev.truth(txn, c, &env)? != Some(true)
                            {
                                continue;
                            }
                            let Some(current) = o.chain(txn, &KeyArg::Rrn(rrn), true)? else {
                                break;
                            };
                            let assigned = assignments(txn, &mut ev, &t, set, &env)?;
                            let mut per_field: Vec<Option<Value>> =
                                vec![None; t.format.format.fields.len()];
                            for (i, v) in assigned {
                                per_field[i] = v;
                            }
                            let (rec, nulls) = build_record(
                                &t,
                                &per_field,
                                Some((&current.data, &current.null_map)),
                            )?;
                            o.update(txn, &rec, &nulls)?;
                            n += 1;
                            break;
                        }
                        MergeAction::Delete { cond } => {
                            if let Some(c) = cond
                                && ev.truth(txn, c, &env)? != Some(true)
                            {
                                continue;
                            }
                            if o.chain(txn, &KeyArg::Rrn(rrn), true)?.is_some() {
                                o.delete(txn)?;
                                n += 1;
                            }
                            break;
                        }
                        MergeAction::Insert { .. } => {}
                    }
                }
            }
        }
        Ok(n)
    })();
    o.close(txn);
    result
}
