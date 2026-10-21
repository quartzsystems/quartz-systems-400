// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Queries and the data-change statements.

use super::expr::{ColumnRef, Expr};
use super::{Ident, Isolation, ObjectName};

/// A query: a body with its ordering, row limits and clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// The body.
    pub body: QueryBody,
    /// `ORDER BY`.
    pub order_by: Vec<OrderItem>,
    /// `OFFSET n ROWS`.
    pub offset: Option<Expr>,
    /// `FETCH FIRST n ROWS ONLY` or `LIMIT n`.
    pub fetch_first: Option<Expr>,
    /// `FOR UPDATE [OF cols]`: `Some(cols)`, empty for every column.
    pub for_update: Option<Vec<Ident>>,
    /// `FOR READ ONLY` / `FOR FETCH ONLY`.
    pub read_only: bool,
    /// `WITH xx` isolation.
    pub isolation: Option<Isolation>,
}

/// The body of a query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryBody {
    /// `SELECT ...`.
    Select(Box<Select>),
    /// `VALUES (...), (...)`.
    Values(Vec<Vec<Expr>>),
    /// `a UNION [ALL] b`.
    Union {
        /// Left side.
        left: Box<QueryBody>,
        /// Right side.
        right: Box<QueryBody>,
        /// `ALL`.
        all: bool,
    },
    /// A parenthesized body.
    Nested(Box<Query>),
}

/// `SELECT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Select {
    /// `DISTINCT`.
    pub distinct: bool,
    /// The select list.
    pub items: Vec<SelectItem>,
    /// `FROM`.
    pub from: Vec<TableRef>,
    /// `WHERE`.
    pub where_clause: Option<Expr>,
    /// `GROUP BY`.
    pub group_by: Vec<Expr>,
    /// `HAVING`.
    pub having: Option<Expr>,
}

/// One select-list item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectItem {
    /// `*`.
    Star,
    /// `T.*`.
    TableStar(Ident),
    /// An expression with an optional `AS` name.
    Expr(Expr, Option<Ident>),
}

/// Join kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinKind {
    /// `[INNER] JOIN`.
    Inner,
    /// `LEFT [OUTER] JOIN`.
    Left,
    /// `RIGHT [OUTER] JOIN`.
    Right,
    /// `FULL [OUTER] JOIN`.
    Full,
    /// `[LEFT] EXCEPTION JOIN`.
    LeftException,
    /// `RIGHT EXCEPTION JOIN`.
    RightException,
    /// `CROSS JOIN`.
    Cross,
}

/// A table reference in `FROM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableRef {
    /// A table, view or alias with an optional correlation name and
    /// column names.
    Table {
        /// The name.
        name: ObjectName,
        /// Correlation name.
        alias: Option<Ident>,
        /// Column names.
        columns: Vec<Ident>,
    },
    /// A derived table.
    Subquery {
        /// The query.
        query: Box<Query>,
        /// Correlation name.
        alias: Ident,
        /// Column names.
        columns: Vec<Ident>,
    },
    /// A join.
    Join {
        /// Left.
        left: Box<TableRef>,
        /// Right.
        right: Box<TableRef>,
        /// Kind.
        kind: JoinKind,
        /// `ON`.
        on: Option<Expr>,
    },
}

/// `ORDER BY` item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderItem {
    /// The expression, a column or a 1-based select-list position.
    pub expr: Expr,
    /// `DESC`.
    pub desc: bool,
    /// `NULLS FIRST`/`NULLS LAST`.
    pub nulls_first: Option<bool>,
}

/// `INSERT` source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertSource {
    /// `VALUES`.
    Values(Vec<Vec<Expr>>),
    /// A query.
    Query(Box<Query>),
    /// `DEFAULT VALUES`.
    DefaultValues,
}

/// `INSERT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Insert {
    /// The table.
    pub table: ObjectName,
    /// Column list.
    pub columns: Vec<Ident>,
    /// The rows.
    pub source: InsertSource,
    /// Isolation.
    pub isolation: Option<Isolation>,
}

/// One `SET` assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    /// The columns (one, or several for a row assignment).
    pub columns: Vec<Ident>,
    /// The values, one per column, or a single row-valued expression.
    pub values: Vec<Expr>,
}

/// `WHERE ...` or `WHERE CURRENT OF cursor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Where {
    /// Every row.
    All,
    /// A search condition.
    Cond(Expr),
    /// The cursor's current row.
    CurrentOf(Ident),
}

/// `UPDATE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// The table.
    pub table: ObjectName,
    /// Correlation name.
    pub alias: Option<Ident>,
    /// `SET`.
    pub set: Vec<Assignment>,
    /// `WHERE`.
    pub where_clause: Where,
    /// Isolation.
    pub isolation: Option<Isolation>,
}

/// `DELETE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delete {
    /// The table.
    pub table: ObjectName,
    /// Correlation name.
    pub alias: Option<Ident>,
    /// `WHERE`.
    pub where_clause: Where,
    /// Isolation.
    pub isolation: Option<Isolation>,
}

/// One `WHEN` of a `MERGE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeAction {
    /// `WHEN MATCHED [AND c] THEN UPDATE SET`.
    Update {
        /// Extra condition.
        cond: Option<Expr>,
        /// Assignments.
        set: Vec<Assignment>,
    },
    /// `WHEN MATCHED [AND c] THEN DELETE`.
    Delete {
        /// Extra condition.
        cond: Option<Expr>,
    },
    /// `WHEN NOT MATCHED [AND c] THEN INSERT`.
    Insert {
        /// Extra condition.
        cond: Option<Expr>,
        /// Columns.
        columns: Vec<Ident>,
        /// Values.
        values: Vec<Expr>,
    },
}

/// `MERGE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    /// The target.
    pub target: ObjectName,
    /// Target correlation name.
    pub alias: Option<Ident>,
    /// `USING`.
    pub source: TableRef,
    /// `ON`.
    pub on: Expr,
    /// The actions, in order.
    pub actions: Vec<MergeAction>,
}

impl Query {
    /// A query of just a body.
    #[must_use]
    pub fn of(body: QueryBody) -> Self {
        Self {
            body,
            order_by: Vec::new(),
            offset: None,
            fetch_first: None,
            for_update: None,
            read_only: false,
            isolation: None,
        }
    }
}

fn walk(e: &Expr, out: &mut Vec<ColumnRef>) {
    match e {
        Expr::Column(c) => out.push(c.clone()),
        Expr::Neg(a)
        | Expr::Not(a)
        | Expr::IsNull(a, _)
        | Expr::Cast(a, _)
        | Expr::Duration(a, _)
        | Expr::InSubquery(a, _, _) => walk(a, out),
        Expr::Binary(a, _, b) | Expr::And(a, b) | Expr::Or(a, b) | Expr::Compare(a, _, b, _) => {
            walk(a, out);
            walk(b, out);
        }
        Expr::Between(a, b, c, _) => {
            walk(a, out);
            walk(b, out);
            walk(c, out);
        }
        Expr::InList(a, list, _) => {
            walk(a, out);
            for x in list {
                walk(x, out);
            }
        }
        Expr::Like(a, p, esc, _) => {
            walk(a, out);
            walk(p, out);
            if let Some(x) = esc {
                walk(x, out);
            }
        }
        Expr::Case {
            operand,
            whens,
            otherwise,
        } => {
            if let Some(o) = operand {
                walk(o, out);
            }
            for (w, t) in whens {
                walk(w, out);
                walk(t, out);
            }
            if let Some(o) = otherwise {
                walk(o, out);
            }
        }
        Expr::Function(f) => {
            for x in &f.args {
                walk(x, out);
            }
        }
        Expr::Row(items) => {
            for x in items {
                walk(x, out);
            }
        }
        Expr::Literal(_)
        | Expr::Param(_)
        | Expr::Host { .. }
        | Expr::Register(_)
        | Expr::Exists(..)
        | Expr::Subquery(_)
        | Expr::Default => {}
    }
}

/// Column references a search condition names, for tests and the planner.
#[must_use]
pub fn columns_of(e: &Expr) -> Vec<ColumnRef> {
    let mut out = Vec::new();
    walk(e, &mut out);
    out
}
