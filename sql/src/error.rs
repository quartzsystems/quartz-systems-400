// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One error type for SQL. Each variant knows its `SQLCODE` and
//! `SQLSTATE`; the text is Quartz's.

/// Result with an [`SqlError`].
pub type Result<T> = std::result::Result<T, SqlError>;

/// Why a statement failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SqlError {
    /// The text does not parse.
    #[error("syntax error at line {line} column {col}: {found}; expected {expected}")]
    Syntax {
        /// Line, from 1.
        line: u32,
        /// Column, from 1.
        col: u32,
        /// What was there.
        found: String,
        /// What would have fitted.
        expected: String,
    },

    /// Valid SQL this release does not do.
    #[error("not supported: {what}")]
    Unsupported {
        /// The construct.
        what: String,
    },

    /// A name of more than two parts.
    #[error("qualified name {name} has too many parts")]
    ThreePartName {
        /// The name as written.
        name: String,
    },

    /// A literal that cannot be a value.
    #[error("literal {text} is not valid: {what}")]
    Literal {
        /// The literal.
        text: String,
        /// What is wrong.
        what: String,
    },
}

impl SqlError {
    /// The `SQLCODE`.
    #[must_use]
    pub fn sqlcode(&self) -> i32 {
        match self {
            Self::Syntax { .. } | Self::ThreePartName { .. } => -104,
            Self::Unsupported { .. } => -84,
            Self::Literal { .. } => -302,
        }
    }

    /// The `SQLSTATE`.
    #[must_use]
    pub fn sqlstate(&self) -> &'static str {
        match self {
            Self::Syntax { .. } | Self::ThreePartName { .. } => "42601",
            Self::Unsupported { .. } => "42612",
            Self::Literal { .. } => "22001",
        }
    }

    /// The message ID: `SQL` plus the code's four digits.
    #[must_use]
    pub fn msgid(&self) -> String {
        format!("SQL{:04}", self.sqlcode().unsigned_abs())
    }
}
