// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Conditions as expression trees, and truth tables over them.

/// A `:COND.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    /// Name.
    pub name: String,
    /// The parsed `EXPR`.
    pub expr: Expr,
    /// `MSG`: the message sent when the condition fails a check.
    pub msg: Option<String>,
    /// Source line.
    pub line: u32,
}

/// A condition expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// `&name.`: a dialog variable's value.
    Var(String),
    /// A quoted string.
    Str(String),
    /// A number, kept as written.
    Num(String),
    /// `%LIST(name)`: the number of entries in a list.
    List(String),
    /// `%NODE(name)`: whether a node of a list is set.
    Node(String),
    /// A comparison.
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    /// `AND`.
    And(Box<Expr>, Box<Expr>),
    /// `OR`.
    Or(Box<Expr>, Box<Expr>),
    /// `NOT`.
    Not(Box<Expr>),
}

impl Expr {
    /// Every variable the expression reads.
    #[must_use]
    pub fn variables(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_vars(&mut out);
        out
    }

    fn collect_vars<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Self::Var(v) => out.push(v),
            Self::Str(_) | Self::Num(_) | Self::List(_) | Self::Node(_) => {}
            Self::Cmp(_, a, b) | Self::And(a, b) | Self::Or(a, b) => {
                a.collect_vars(out);
                b.collect_vars(out);
            }
            Self::Not(a) => a.collect_vars(out),
        }
    }

    /// Every list `%LIST` or `%NODE` names.
    #[must_use]
    pub fn lists(&self) -> Vec<&str> {
        let mut out = Vec::new();
        self.collect_lists(&mut out);
        out
    }

    fn collect_lists<'a>(&'a self, out: &mut Vec<&'a str>) {
        match self {
            Self::List(l) | Self::Node(l) => out.push(l),
            Self::Var(_) | Self::Str(_) | Self::Num(_) => {}
            Self::Cmp(_, a, b) | Self::And(a, b) | Self::Or(a, b) => {
                a.collect_lists(out);
                b.collect_lists(out);
            }
            Self::Not(a) => a.collect_lists(out),
        }
    }
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    /// `EQ`.
    Eq,
    /// `NE`.
    Ne,
    /// `GT`.
    Gt,
    /// `LT`.
    Lt,
    /// `GE`.
    Ge,
    /// `LE`.
    Le,
}

impl CmpOp {
    /// The keyword.
    #[must_use]
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Eq => "EQ",
            Self::Ne => "NE",
            Self::Gt => "GT",
            Self::Lt => "LT",
            Self::Ge => "GE",
            Self::Le => "LE",
        }
    }
}

/// A `:TT.`: which combinations of conditions are allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TruthTable {
    /// Name.
    pub name: String,
    /// `CONDS`: the conditions the columns stand for.
    pub conds: Vec<String>,
    /// `:TTROW.` rows, one value per condition.
    pub rows: Vec<Vec<TtValue>>,
    /// Source line.
    pub line: u32,
}

/// One cell of a truth table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtValue {
    /// `1`: the condition is true.
    True,
    /// `0`: the condition is false.
    False,
    /// `X`: either.
    Any,
}
