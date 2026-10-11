// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Operator enumerations with their text names.

/// A binary integer or float operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    /// Addition.
    Add,
    /// Subtraction.
    Sub,
    /// Multiplication.
    Mul,
    /// Division toward zero; `MCH1211` when the divisor is zero.
    Div,
    /// Remainder with the sign of the dividend; `MCH1211` on zero.
    Rem,
    /// Bitwise and; integers only.
    And,
    /// Bitwise or; integers only.
    Or,
    /// Bitwise exclusive or; integers only.
    Xor,
    /// Shift left; integers only.
    Shl,
    /// Shift right, arithmetic for signed types; integers only.
    Shr,
}

impl BinOp {
    /// The opcode as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Shl => "shl",
            Self::Shr => "shr",
        }
    }

    /// From the opcode.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "add" => Self::Add,
            "sub" => Self::Sub,
            "mul" => Self::Mul,
            "div" => Self::Div,
            "rem" => Self::Rem,
            "and" => Self::And,
            "or" => Self::Or,
            "xor" => Self::Xor,
            "shl" => Self::Shl,
            "shr" => Self::Shr,
            _ => return None,
        })
    }

    /// Whether the operator applies to floats as well as integers.
    #[must_use]
    pub fn allows_float(self) -> bool {
        matches!(
            self,
            Self::Add | Self::Sub | Self::Mul | Self::Div | Self::Rem
        )
    }

    /// Whether the `.chk` suffix (overflow check) makes sense.
    #[must_use]
    pub fn checkable(self) -> bool {
        matches!(self, Self::Add | Self::Sub | Self::Mul)
    }
}

/// A unary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// Negation.
    Neg,
    /// Bitwise not; integers only.
    Not,
    /// Absolute value.
    Abs,
}

impl UnOp {
    /// The opcode as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Neg => "neg",
            Self::Not => "not",
            Self::Abs => "abs",
        }
    }

    /// From the opcode.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "neg" => Self::Neg,
            "not" => Self::Not,
            "abs" => Self::Abs,
            _ => return None,
        })
    }
}

/// A comparison condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cond {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Le,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Ge,
}

impl Cond {
    /// The suffix as written.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        }
    }

    /// From the suffix.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "lt" => Self::Lt,
            "le" => Self::Le,
            "gt" => Self::Gt,
            "ge" => Self::Ge,
            _ => return None,
        })
    }
}
