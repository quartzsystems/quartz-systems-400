// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The table references beyond tables, derived tables and joins:
//! `JOIN ... USING`, `LATERAL` and `TABLE (...)` that see the rows
//! before them, table functions, and the hidden column behind
//! `ORDER OF`.

use std::rc::Rc;

use store::txn::Txn;

use super::eval::{ColInfo, Env, Evaluator, Row, Schema};
use super::query::{Scan, from_rows};
use super::restype::SqlType;
use super::{SqlContext, join};
use crate::ast::{
    ChangeRows, CmpOp, ColumnRef, Expr, Ident, JoinKind, ObjectName, OrderItem, Statement, TableRef,
};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The hidden column of a derived table: each row's place in its order.
pub const ORDER_COLUMN: &str = "*ORD";

/// [`ORDER_COLUMN`] of correlation `alias`.
pub(crate) fn order_column(alias: &str) -> ColInfo {
    ColInfo {
        name: ORDER_COLUMN.into(),
        table: Some(alias.to_string()),
        field: Some(SqlType::bigint(false).to_field()),
        nullable: false,
        hidden: true,
    }
}

/// A column reference written as a delimited name.
fn col(table: Option<&str>, name: &str) -> Expr {
    Expr::Column(ColumnRef {
        table: table.map(Ident::new),
        column: Ident {
            value: name.into(),
            delimited: true,
        },
    })
}

/// The schema of a join: right columns null-capable for a left or full
/// join, left ones for a right or full join.
fn joined(ls: &Schema, rs: &Schema, kind: JoinKind) -> Vec<ColInfo> {
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
    cols
}

/// `left JOIN right USING (columns)`: joined on the columns' equality;
/// the columns once, first, each the left value or for a missing left
/// row the right one; then the other columns. The joined sides' own
/// columns stay behind them, hidden, for qualified references.
#[allow(clippy::too_many_arguments)]
pub(super) fn join_using(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    left: &TableRef,
    right: &TableRef,
    kind: JoinKind,
    columns: &[Ident],
    scans: &[Scan],
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>, bool)> {
    let (ls, left_rows, _) = from_rows(txn, ctx, params, left, scans, outer, order, explain_only)?;
    let (rs, right_rows, _) = from_rows(txn, ctx, params, right, scans, outer, &[], explain_only)?;
    let mut li = Vec::new();
    let mut ri = Vec::new();
    let missing = |c: &Ident| SqlError::ColumnNotFound {
        name: c.value.clone(),
    };
    for c in columns {
        li.push(ls.find(None, &c.value)?.ok_or_else(|| missing(c))?);
        ri.push(rs.find(None, &c.value)?.ok_or_else(|| missing(c))?);
    }
    // The right side's columns renamed apart for the condition.
    let mut rs2 = (*rs).clone();
    let mut on: Option<Expr> = None;
    for (k, &i) in ri.iter().enumerate() {
        let tmp = format!("*U{k}");
        rs2.cols[i].name.clone_from(&tmp);
        rs2.cols[i].table = None;
        // Found by the new name only, not its field's system name.
        rs2.cols[i].field = None;
        let eq = Expr::Compare(
            Box::new(col(None, &ls.cols[li[k]].name)),
            CmpOp::Eq,
            Box::new(col(None, &tmp)),
            None,
        );
        on = Some(match on {
            None => eq,
            Some(prev) => Expr::And(Box::new(prev), Box::new(eq)),
        });
    }
    let schema = Rc::new(Schema {
        cols: joined(&ls, &rs2, kind),
    });
    let rows = if explain_only {
        ctx.explain.push(format!("JOIN {kind:?} USING nested loop"));
        Vec::new()
    } else {
        let mut ev = Evaluator { ctx, params };
        join::join(
            txn,
            &mut ev,
            &schema,
            &ls,
            &rs2,
            &left_rows,
            &right_rows,
            kind,
            on.as_ref(),
            outer,
        )?
    };
    // The coalesced columns, then both sides with the named ones hidden.
    let mut cols: Vec<ColInfo> = Vec::new();
    for (k, c) in columns.iter().enumerate() {
        let l = &ls.cols[li[k]];
        let r = &rs.cols[ri[k]];
        let nullable = match kind {
            JoinKind::Right | JoinKind::RightException => r.nullable,
            JoinKind::Full => l.nullable || r.nullable,
            _ => l.nullable,
        };
        cols.push(ColInfo {
            name: c.value.clone(),
            table: None,
            field: l.field.clone(),
            nullable,
            hidden: false,
        });
    }
    let mut sides = joined(&ls, &rs, kind);
    for &i in &li {
        sides[i].hidden = true;
    }
    for &i in &ri {
        sides[ls.cols.len() + i].hidden = true;
    }
    cols.extend(sides);
    let width = ls.cols.len();
    let out = rows
        .into_iter()
        .map(|r| {
            let mut v: Row = li
                .iter()
                .zip(&ri)
                .map(|(&a, &b)| {
                    if r[a].is_null() && !r[width + b].is_null() {
                        r[width + b].clone()
                    } else {
                        r[a].clone()
                    }
                })
                .collect();
            v.extend(r);
            v
        })
        .collect();
    Ok((Rc::new(Schema { cols }), out, false))
}

/// `left JOIN LATERAL (...)`: the right side run once for each left row,
/// which it sees.
#[allow(clippy::too_many_arguments)]
pub(super) fn lateral_join(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    left: &TableRef,
    right: &TableRef,
    kind: JoinKind,
    on: Option<&Expr>,
    scans: &[Scan],
    outer: &Env,
    order: &[OrderItem],
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>, bool)> {
    if !matches!(kind, JoinKind::Inner | JoinKind::Cross | JoinKind::Left) {
        return Err(SqlError::Unsupported {
            what: format!("a {kind:?} join to a LATERAL table"),
        });
    }
    let (ls, left_rows, lord) =
        from_rows(txn, ctx, params, left, scans, outer, order, explain_only)?;
    let mut rschema: Option<Rc<Schema>> = None;
    let mut out = Vec::new();
    let mut charge = ctx.charge();
    if !explain_only {
        for l in &left_rows {
            ctx.tick(txn)?;
            let env = outer.with(ls.clone(), l.clone());
            let (rs, right_rows, _) = from_rows(txn, ctx, params, right, scans, &env, &[], false)?;
            let schema = Rc::new(Schema {
                cols: joined(&ls, &rs, kind),
            });
            let rs_len = rs.cols.len();
            rschema.get_or_insert(rs);
            let mut matched = false;
            for r in right_rows {
                let mut row = l.clone();
                row.extend(r);
                let hit = match on {
                    None => true,
                    Some(e) => {
                        let env = outer.with(schema.clone(), row.clone());
                        let mut ev = Evaluator { ctx, params };
                        ev.truth(txn, e, &env)? == Some(true)
                    }
                };
                if hit {
                    matched = true;
                    charge.row(&row)?;
                    out.push(row);
                }
            }
            if !matched && kind == JoinKind::Left {
                let mut row = l.clone();
                row.extend(std::iter::repeat_n(Value::Null, rs_len));
                out.push(row);
            }
        }
    }
    // With no left row, the right side's columns from a run on nulls.
    let rs = if let Some(s) = rschema {
        s
    } else {
        let env = outer.with(ls.clone(), vec![Value::Null; ls.cols.len()]);
        let saved = std::mem::take(&mut ctx.explain);
        let r = from_rows(txn, ctx, params, right, scans, &env, &[], true);
        let mut lines = std::mem::replace(&mut ctx.explain, saved);
        if explain_only {
            ctx.explain.push("LATERAL nested loop".into());
            ctx.explain.append(&mut lines);
        }
        r?.0
    };
    Ok((
        Rc::new(Schema {
            cols: joined(&ls, &rs, kind),
        }),
        out,
        lord && kind != JoinKind::Cross,
    ))
}

/// `TABLE (f(args)) AS x (cols)`: a table function a provider answers.
#[allow(clippy::too_many_arguments)]
pub(super) fn table_function(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    name: &ObjectName,
    args: &[Expr],
    alias: &Ident,
    columns: &[Ident],
    outer: &Env,
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>, bool)> {
    let mut values = Vec::with_capacity(args.len());
    {
        let mut ev = Evaluator { ctx, params };
        for a in args {
            values.push(ev.eval(txn, a, outer)?);
        }
    }
    let unnest =
        name.schema.is_none() && matches!(name.name.value.as_str(), "*UNNEST" | "*UNNESTO");
    let found = if unnest {
        Some(super::array::unnest(
            ctx,
            &values,
            name.name.value == "*UNNESTO",
        )?)
    } else if let Some(t) =
        super::catalog::table_function(txn, ctx, name, &values, args, &alias.value)?
    {
        Some(t)
    } else {
        // A user table function (`routine::udf`).
        let forms: Vec<_> = args.iter().map(super::routine::udf::arg_form).collect();
        super::routine::udf::table(txn, ctx, name, &values, &forms, &alias.value).transpose()?
    };
    let Some((mut schema, rows)) = found else {
        return Err(SqlError::ObjectNotFound {
            name: name.to_string(),
            kind: "function",
        });
    };
    if !columns.is_empty() {
        let visible: Vec<usize> = schema
            .cols
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.hidden)
            .map(|(i, _)| i)
            .collect();
        if visible.len() != columns.len() {
            return Err(SqlError::ValueCount {
                what: format!(
                    "{} names {} columns for {}",
                    alias.value,
                    columns.len(),
                    visible.len()
                ),
            });
        }
        for (i, c) in visible.into_iter().zip(columns) {
            schema.cols[i].name.clone_from(&c.value);
        }
    }
    for c in &mut schema.cols {
        c.table = Some(alias.value.clone());
    }
    if explain_only {
        ctx.explain
            .push(format!("TABLE FUNCTION {name} ({})", alias.value));
    }
    Ok((Rc::new(schema), rows, false))
}

/// `FINAL TABLE (...)`, `NEW TABLE (...)`, `OLD TABLE (...)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn data_change(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    params: &[Value],
    kind: ChangeRows,
    statement: &Statement,
    alias: &Ident,
    columns: &[Ident],
    explain_only: bool,
) -> Result<(Rc<Schema>, Vec<Row>, bool)> {
    let (mut schema, rows) = super::change::data_change_rows(
        txn,
        ctx,
        params,
        kind,
        statement,
        &alias.value,
        explain_only,
    )?;
    if !columns.is_empty() {
        for (c, n) in schema.cols.iter_mut().filter(|c| !c.hidden).zip(columns) {
            c.name.clone_from(&n.value);
        }
    }
    Ok((Rc::new(schema), rows, false))
}
