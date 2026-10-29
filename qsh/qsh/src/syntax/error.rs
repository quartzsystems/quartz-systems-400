// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Parse errors.

use thiserror::Error;

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ParseErrorKind {
    /// A token where the grammar does not allow it.
    #[error("token \"{0}\" was not expected")]
    Unexpected(String),
    /// The input ended inside a construct.
    #[error("end of input inside {0}")]
    Eof(&'static str),
    /// A malformed `${...}`.
    #[error("parameter substitution is not valid")]
    BadSubst,
    /// Nesting beyond the parser's limit.
    #[error("commands are nested too deeply")]
    TooDeep,
    /// A function name that is not a name.
    #[error("function name {0} is not valid")]
    BadFunctionName(String),
}

/// A parse error with its line.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[error("line {line}: {kind}")]
pub struct ParseError {
    /// The line, from 1.
    pub line: u32,
    /// What went wrong.
    pub kind: ParseErrorKind,
}

impl ParseError {
    /// More input could complete the command (interactive `PS2`).
    pub fn is_incomplete(&self) -> bool {
        matches!(self.kind, ParseErrorKind::Eof(_))
    }
}
