// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cursors: declared with a query, opened into a result set, fetched
//! in either direction, closed at `CLOSE` or `COMMIT` unless `WITH HOLD`.

use store::txn::Txn;

use super::query::run_cursor_query;
use super::{ColumnDesc, Outcome, ResultSet, SqlContext, TableInfo};
use crate::ast::{DeclareCursor, Fetch, FetchOrientation, Query};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// A declared cursor.
#[derive(Clone, Debug)]
pub struct Cursor {
    /// The query.
    pub query: Query,
    /// `WITH HOLD`: survives `COMMIT`.
    pub hold: bool,
    /// `SCROLL`: every orientation; else `NEXT` only.
    pub scroll: bool,
    /// Open.
    pub open: bool,
    /// Columns of the result.
    pub columns: Vec<ColumnDesc>,
    /// The rows.
    pub rows: Vec<Vec<Value>>,
    /// The table read, when the query reads one plainly.
    pub table: Option<TableInfo>,
    /// Each row's relative record number in that table.
    pub rrns: Vec<Option<u64>>,
    /// Position: 0 before the first row, `rows.len() + 1` after the last.
    pub pos: usize,
}

impl Cursor {
    /// The current row's table and RRN, for `WHERE CURRENT OF`.
    pub fn current(&self, name: &str) -> Result<(TableInfo, u64)> {
        if !self.open {
            return Err(SqlError::CursorState {
                cursor: name.into(),
                what: "is not open".into(),
            });
        }
        if self.pos == 0 || self.pos > self.rows.len() {
            return Err(SqlError::CursorState {
                cursor: name.into(),
                what: "is not on a row".into(),
            });
        }
        let table = self.table.clone().ok_or_else(|| SqlError::CursorState {
            cursor: name.into(),
            what: "is not updatable: it does not read one table".into(),
        })?;
        let rrn = self.rrns[self.pos - 1].ok_or_else(|| SqlError::CursorState {
            cursor: name.into(),
            what: "has no record for the row".into(),
        })?;
        Ok((table, rrn))
    }
}

/// `DECLARE CURSOR`: a second declaration replaces the first.
pub fn declare(ctx: &mut SqlContext, d: &DeclareCursor) -> Outcome {
    ctx.cursors.insert(
        d.name.value.clone(),
        Cursor {
            query: d.query.clone(),
            hold: d.hold,
            scroll: d.scroll,
            open: false,
            columns: Vec::new(),
            rows: Vec::new(),
            table: None,
            rrns: Vec::new(),
            pos: 0,
        },
    );
    Outcome::Done
}

fn get<'a>(ctx: &'a mut SqlContext, name: &str) -> Result<&'a mut Cursor> {
    ctx.cursors
        .get_mut(name)
        .ok_or_else(|| SqlError::CursorState {
            cursor: name.into(),
            what: "is not declared".into(),
        })
}

/// `OPEN`: run the query; `USING` values stand for the `?` markers.
pub fn open(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    name: &str,
    params: &[Value],
) -> Result<Outcome> {
    let c = get(ctx, name)?;
    if c.open {
        return Err(SqlError::CursorState {
            cursor: name.into(),
            what: "is already open".into(),
        });
    }
    let query = c.query.clone();
    let (rs, table, rrns) = run_cursor_query(txn, ctx, params, &query)?;
    let c = get(ctx, name)?;
    c.columns = rs.columns;
    c.rows = rs.rows;
    c.table = table;
    c.rrns = rrns;
    c.pos = 0;
    c.open = true;
    Ok(Outcome::Done)
}

/// `FETCH`: the row fetched as a one-row result set, or an empty one
/// past either end (the SQLCA then says `+100`).
#[allow(clippy::many_single_char_names)]
pub fn fetch(ctx: &mut SqlContext, f: &Fetch, params: &[Value]) -> Result<Outcome> {
    let name = f.cursor.value.as_str();
    let offset = |e: &crate::ast::Expr| -> Result<i64> {
        match e {
            crate::ast::Expr::Literal(crate::ast::Literal::Num(n)) => {
                n.parse().map_err(|_| SqlError::Conversion {
                    what: format!("{n} is not a row count"),
                })
            }
            crate::ast::Expr::Neg(inner) => offset_inner(inner).map(|n| -n),
            crate::ast::Expr::Param(i) => params
                .get(*i)
                .ok_or_else(|| SqlError::ValueCount {
                    what: "FETCH position parameter has no value".into(),
                })?
                .as_i64(),
            _ => Err(SqlError::Unsupported {
                what: "a FETCH position that is not a number".into(),
            }),
        }
    };
    let c = get(ctx, name)?;
    if !c.open {
        return Err(SqlError::CursorState {
            cursor: name.into(),
            what: "is not open".into(),
        });
    }
    let n = c.rows.len();
    let after = n + 1;
    if !c.scroll && !matches!(f.orientation, FetchOrientation::Next) {
        return Err(SqlError::CursorState {
            cursor: name.into(),
            what: "is not a SCROLL cursor".into(),
        });
    }
    let target: i64 = match &f.orientation {
        FetchOrientation::Next => i64::try_from(c.pos).expect("fits") + 1,
        FetchOrientation::Prior => i64::try_from(c.pos).expect("fits") - 1,
        FetchOrientation::First => 1,
        FetchOrientation::Last => i64::try_from(n).expect("fits"),
        FetchOrientation::Before => 0,
        FetchOrientation::After => i64::try_from(after).expect("fits"),
        FetchOrientation::Current => i64::try_from(c.pos).expect("fits"),
        FetchOrientation::Relative(e) => i64::try_from(c.pos).expect("fits") + offset(e)?,
        FetchOrientation::Absolute(e) => {
            let k = offset(e)?;
            if k < 0 {
                i64::try_from(n).expect("fits") + k + 1
            } else {
                k
            }
        }
    };
    let pos = usize::try_from(target.clamp(0, i64::try_from(after).expect("fits"))).expect("fits");
    c.pos = pos;
    if pos == 0 || pos > n {
        return Ok(Outcome::Rows(ResultSet {
            columns: c.columns.clone(),
            rows: Vec::new(),
        }));
    }
    Ok(Outcome::Rows(ResultSet {
        columns: c.columns.clone(),
        rows: vec![c.rows[pos - 1].clone()],
    }))
}

fn offset_inner(e: &crate::ast::Expr) -> Result<i64> {
    match e {
        crate::ast::Expr::Literal(crate::ast::Literal::Num(n)) => {
            n.parse().map_err(|_| SqlError::Conversion {
                what: format!("{n} is not a row count"),
            })
        }
        _ => Err(SqlError::Unsupported {
            what: "a FETCH position that is not a number".into(),
        }),
    }
}

/// `CLOSE`.
pub fn close(ctx: &mut SqlContext, name: &str) -> Result<Outcome> {
    let c = get(ctx, name)?;
    if !c.open {
        return Err(SqlError::CursorState {
            cursor: name.into(),
            what: "is not open".into(),
        });
    }
    c.open = false;
    c.rows.clear();
    c.rrns.clear();
    Ok(Outcome::Done)
}

/// At `COMMIT` or `ROLLBACK`: cursors without `WITH HOLD` close.
pub fn end_of_unit(ctx: &mut SqlContext) {
    for c in ctx.cursors.values_mut() {
        if !c.hold {
            c.open = false;
            c.rows.clear();
            c.rrns.clear();
        }
    }
}
