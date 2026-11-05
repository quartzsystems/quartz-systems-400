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
    /// `TIMESTAMP(p)`, `p` 0 to 12 (6 when not given).
    Timestamp(u8),
    /// `DECFLOAT(16)` or `DECFLOAT(34)`.
    DecFloat(u8),
    /// `BINARY(n)`.
    Binary(u32),
    /// `VARBINARY(n)`, `BINARY VARYING(n)`.
    Varbinary(u32),
    /// `BOOLEAN`.
    Boolean,
    /// `BLOB(n)`.
    Blob(u32),
    /// `CLOB(n)`.
    Clob(u32),
    /// `DBCLOB(n)`.
    Dbclob(u32),
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
    /// A distinct type: its name as written until the statement resolves
    /// it (`exec::udt`), then `SCHEMA.NAME` with `data_type` its source.
    pub udt: Option<String>,
}

/// A literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Literal {
    /// A character string.
    Str(String),
    /// A hexadecimal string.
    Hex(Vec<u8>),
    /// A binary string, `BX'..'`.
    Bin(Vec<u8>),
    /// `TRUE` or `FALSE`.
    Bool(bool),
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
    /// `CURRENT TIMESTAMP`, `CURRENT TIMESTAMP(p)`: its precision, 6
    /// when none is given.
    Timestamp(u8),
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
    /// `CURRENT DECFLOAT ROUNDING MODE`.
    DecfloatRounding,
    /// Another special register or built-in global variable, by its
    /// canonical name (`CURRENT DEGREE`, `QSYS2.JOB_NAME`, ...).
    Named(&'static str),
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
    /// `NEXT VALUE FOR sequence`.
    NextValue(super::ObjectName),
    /// `PREVIOUS VALUE FOR sequence`.
    PrevValue(super::ObjectName),
    /// An OLAP specification: a function `OVER (...)`.
    Over(Box<Over>),
    /// An aggregate with `WITHIN GROUP (ORDER BY ...)` or `ON OVERFLOW`.
    WithinGroup(Box<WithinGroup>),
}

/// A function over a window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Over {
    /// The function: a [`Function`] or a [`WithinGroup`].
    pub func: Expr,
    /// `IGNORE NULLS` (else `RESPECT NULLS`).
    pub ignore_nulls: bool,
    /// The window.
    pub window: Window,
    /// The statement's number for this specification: where its value
    /// is found while the select list is evaluated.
    pub slot: usize,
}

/// `OVER (PARTITION BY ... ORDER BY ... frame)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// `PARTITION BY`.
    pub partition: Vec<Expr>,
    /// `ORDER BY`.
    pub order: Vec<super::query::OrderItem>,
    /// `ROWS` or `RANGE`.
    pub frame: Option<Frame>,
}

/// A window frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// `ROWS` (else `RANGE`).
    pub rows: bool,
    /// Start.
    pub start: FrameBound,
    /// End.
    pub end: FrameBound,
}

/// One end of a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameBound {
    /// `UNBOUNDED PRECEDING`.
    UnboundedPreceding,
    /// `n PRECEDING`.
    Preceding(Expr),
    /// `CURRENT ROW`.
    CurrentRow,
    /// `n FOLLOWING`.
    Following(Expr),
    /// `UNBOUNDED FOLLOWING`.
    UnboundedFollowing,
}

/// `LISTAGG`, `PERCENTILE_CONT` and the like with their ordering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WithinGroup {
    /// The aggregate.
    pub func: Function,
    /// `WITHIN GROUP (ORDER BY ...)`.
    pub order: Vec<super::query::OrderItem>,
    /// `ON OVERFLOW ...` of `LISTAGG`.
    pub overflow: Option<Overflow>,
}

/// `LISTAGG ... ON OVERFLOW`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// `ERROR`.
    Error,
    /// `TRUNCATE [filler] WITH|WITHOUT COUNT`.
    Truncate {
        /// The filler, `...` when none.
        filler: Option<String>,
        /// `WITH COUNT`.
        count: bool,
    },
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

impl DataType {
    /// Whether `BLOB`, `CLOB` or `DBCLOB`.
    #[must_use]
    pub fn is_lob_type(&self) -> bool {
        matches!(self, Self::Blob(_) | Self::Clob(_) | Self::Dbclob(_))
    }
}
