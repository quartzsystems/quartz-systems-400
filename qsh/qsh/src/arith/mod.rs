// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arithmetic expansion: the C-like expression language of `$((...))`
//! and `let`, over signed 64-bit integers.

mod eval;
mod lex;
mod parse;

use thiserror::Error;

pub use eval::eval;

/// Variables as arithmetic sees them.
pub trait ArithEnv {
    /// The value of a variable, `None` when unset.
    fn get(&mut self, name: &str) -> Option<String>;
    /// Assign a variable; the error is a message naming why not.
    fn set(&mut self, name: &str, value: i64) -> Result<(), String>;
}

/// An arithmetic failure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ArithError {
    /// The expression does not parse.
    #[error("arithmetic expression {0} is not valid")]
    Syntax(String),
    /// A number with digits outside its base.
    #[error("number {0} is not valid")]
    BadNumber(String),
    /// `/` or `%` by zero.
    #[error("division by zero")]
    DivZero,
    /// An assignment the variable refused.
    #[error("{0}")]
    Assign(String),
    /// Variables referring to each other too deeply.
    #[error("expression recursion is too deep")]
    TooDeep,
}
