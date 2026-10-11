// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `INSERT`, `UPDATE` and `DELETE` through the open data path, so the
//! journal sees what it would from RPG.

use std::rc::Rc;

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

/// A column's default value, and whether it is the null value.
fn default_of(f: &FieldDef) -> Result<Option<Value>> {
    Ok(match &f.default {
        Some(DefaultValue::Null) => Some(Value::Null),
        Some(DefaultValue::Quoted(q)) => Some(Value::Str(q.clone())),
        Some(DefaultValue::Hex(h)) => Some(Value::Str(ccsid::from_ebcdic(h))),
        Some(DefaultValue::Number(n)) => {
            Some(Value::Num(qdecimal::Decimal::parse(n).map_err(|e| {
                SqlError::Conversion {
                    what: e.to_string(),
                }
            })?))
        }
        None => None,
    })
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
                Some(d) => d,
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
    let result = (|| {
        for r in rows {
            let mut per_field: Vec<Option<Value>> = vec![None; t.format.format.fields.len()];
            for (k, &fi) in idx.iter().enumerate() {
                per_field[fi] = r.get(k).cloned().flatten();
            }
            let (rec, nulls) = build_record(&t, &per_field, None)?;
            o.write(txn, &rec, &nulls)?;
            n += 1;
        }
        Ok(n)
    })();
    o.close(txn);
    result
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
                None => Some(match default_of(f)? {
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
    let cond = match &u.where_clause {
        Where::All => None,
        Where::Cond(c) => Some(c),
        Where::CurrentOf(_) => {
            return Err(SqlError::Unsupported {
                what: "UPDATE WHERE CURRENT OF outside a cursor".into(),
            });
        }
    };
    let (schema, rows) = matching_rows(txn, ctx, params, &t, &alias, cond)?;
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
    let cond = match &d.where_clause {
        Where::All => None,
        Where::Cond(c) => Some(c),
        Where::CurrentOf(_) => {
            return Err(SqlError::Unsupported {
                what: "DELETE WHERE CURRENT OF outside a cursor".into(),
            });
        }
    };
    let (schema, rows) = matching_rows(txn, ctx, params, &t, &alias, cond)?;
    let mut o = ctx.open(txn, &t, Usage::UPDATE)?;
    let result = change_rows(txn, ctx, params, &t, &schema, rows, &mut o, None);
    o.close(txn);
    result
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
