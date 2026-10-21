// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions, predicates and data types.

use super::{Ident, query::Query};

/// A data type as written in a column definition or `CAST`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataType {
    /// `CHAR(n)`.
    Char(u32),
    /// `VARCHAR(n)`.
    Varchar(u32),
    /// `GRAPHIC(n)`.
    Graphic(u32),
    /// `VARGRAPHIC(n)`.
    Vargraphic(u32),
    /// `DECIMAL(p, s)` (packed).
    Decimal(u8, u8),
    /// `NUMERIC(p, s)` (zoned).
    Numeric(u8, u8),
    /// `SMALLINT`.
    SmallInt,
    /// `INTEGER`.
    Integer,
    /// `BIGINT`.
    BigInt,
    /// `REAL`.
    Real,
    /// `DOUBLE`, `FLOAT`.
    Double,
    /// `DATE`.
    Date,
    /// `TIME`.
    Time,
    /// `TIMESTAMP`.
    Timestamp,
}

/// A column data type with its options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSpec {
    /// The type.
    pub data_type: DataType,
    /// `CCSID n`.
    pub ccsid: Option<u16>,
    /// `FOR BIT DATA`.
    pub bit_data: bool,
}

/// A literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// A character string.
    Str(String),
    /// A hexadecimal string.
    Hex(Vec<u8>),
    /// A number, as written.
    Num(String),
    /// `NULL`.
    Null,
    /// `DATE 'x'`.
    Date(String),
    /// `TIME 'x'`.
    Time(String),
    /// `TIMESTAMP 'x'`.
    Timestamp(String),
}

/// A special register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Register {
    /// `CURRENT DATE`.
    Date,
    /// `CURRENT TIME`.
    Time,
    /// `CURRENT TIMESTAMP`.
    Timestamp,
    /// `CURRENT USER`, `USER`, `SESSION_USER`.
    User,
    /// `CURRENT SERVER`.
    Server,
    /// `CURRENT SCHEMA`.
    Schema,
    /// `CURRENT PATH`.
    Path,
    /// `CURRENT TIMEZONE`.
    Timezone,
}

/// Comparison operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    /// `=`.
    Eq,
    /// `<>`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

/// Arithmetic and string operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `||`, `CONCAT`.
    Concat,
}

/// Labeled duration units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DurationUnit {
    /// `YEARS`.
    Years,
    /// `MONTHS`.
    Months,
    /// `DAYS`.
    Days,
    /// `HOURS`.
    Hours,
    /// `MINUTES`.
    Minutes,
    /// `SECONDS`.
    Seconds,
    /// `MICROSECONDS`.
    Microseconds,
}

/// `ALL`, `ANY`, `SOME` after a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantifier {
    /// `ALL`.
    All,
    /// `ANY` or `SOME`.
    Any,
}

/// A column reference: `C`, `T.C`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ColumnRef {
    /// Table or correlation name.
    pub table: Option<Ident>,
    /// Column.
    pub column: Ident,
}

/// An expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Literal(Literal),
    /// A column.
    Column(ColumnRef),
    /// `?`, numbered from 0 in statement order.
    Param(usize),
    /// `:NAME[:IND]`.
    Host {
        /// The variable.
        name: String,
        /// The indicator.
        ind: Option<String>,
    },
    /// A special register.
    Register(Register),
    /// `-e`.
    Neg(Box<Expr>),
    /// `a op b`.
    Binary(Box<Expr>, BinOp, Box<Expr>),
    /// `e unit`: a labeled duration.
    Duration(Box<Expr>, DurationUnit),
    /// `a cmp b`, optionally quantified over a subquery.
    Compare(Box<Expr>, CmpOp, Box<Expr>, Option<Quantifier>),
    /// `a AND b`.
    And(Box<Expr>, Box<Expr>),
    /// `a OR b`.
    Or(Box<Expr>, Box<Expr>),
    /// `NOT p`.
    Not(Box<Expr>),
    /// `e IS [NOT] NULL`.
    IsNull(Box<Expr>, bool),
    /// `e [NOT] BETWEEN a AND b`.
    Between(Box<Expr>, Box<Expr>, Box<Expr>, bool),
    /// `e [NOT] IN (list)`.
    InList(Box<Expr>, Vec<Expr>, bool),
    /// `e [NOT] IN (subquery)`.
    InSubquery(Box<Expr>, Box<Query>, bool),
    /// `e [NOT] LIKE pattern [ESCAPE c]`.
    Like(Box<Expr>, Box<Expr>, Option<Box<Expr>>, bool),
    /// `[NOT] EXISTS (subquery)`.
    Exists(Box<Query>, bool),
    /// A scalar subquery.
    Subquery(Box<Query>),
    /// `CASE`.
    Case {
        /// The operand of a simple `CASE`.
        operand: Option<Box<Expr>>,
        /// `WHEN ... THEN ...`.
        whens: Vec<(Expr, Expr)>,
        /// `ELSE`.
        otherwise: Option<Box<Expr>>,
    },
    /// `CAST(e AS type)`.
    Cast(Box<Expr>, TypeSpec),
    /// A function call.
    Function(Function),
    /// A row of values, in `IN`, `VALUES` and row comparisons.
    Row(Vec<Expr>),
    /// `DEFAULT` in an `INSERT` or `UPDATE` value.
    Default,
}

/// A function call, scalar or aggregate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    /// The name, upper case.
    pub name: String,
    /// `DISTINCT` (aggregates).
    pub distinct: bool,
    /// `*` as the only argument (`COUNT(*)`).
    pub star: bool,
    /// The arguments.
    pub args: Vec<Expr>,
}

impl Expr {
    /// A column by name.
    #[must_use]
    pub fn column(name: &str) -> Self {
        Self::Column(ColumnRef {
            table: None,
            column: Ident::new(name),
        })
    }

    /// A number literal.
    #[must_use]
    pub fn num(n: &str) -> Self {
        Self::Literal(Literal::Num(n.into()))
    }

    /// A string literal.
    #[must_use]
    pub fn str(s: &str) -> Self {
        Self::Literal(Literal::Str(s.into()))
    }
}
