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

    /// A name longer than its kind allows.
    #[error("the name {name} is longer than {max} characters")]
    NameTooLong {
        /// The name.
        name: String,
        /// The most characters allowed.
        max: usize,
    },

    /// A literal that cannot be a value.
    #[error("literal {text} is not valid: {what}")]
    Literal {
        /// The literal.
        text: String,
        /// What is wrong.
        what: String,
    },

    /// A table, index, schema or other object that does not exist.
    #[error("{kind} {name} not found")]
    ObjectNotFound {
        /// The name as written.
        name: String,
        /// What kind of object.
        kind: &'static str,
    },

    /// A column that no table in scope has.
    #[error("column {name} not found")]
    ColumnNotFound {
        /// The name.
        name: String,
    },

    /// A column more than one table in scope has.
    #[error("column {name} is ambiguous")]
    AmbiguousColumn {
        /// The name.
        name: String,
    },

    /// A value that cannot become the type needed.
    #[error("conversion: {what}")]
    Conversion {
        /// What failed.
        what: String,
    },

    /// Operands of types that do not go together.
    #[error("incompatible types: {what}")]
    TypeMismatch {
        /// Which.
        what: String,
    },

    /// A value too large for its target.
    #[error("overflow: {what}")]
    Overflow {
        /// What failed.
        what: String,
    },

    /// Null into a column that does not allow it.
    #[error("column {column} does not allow null")]
    NullNotAllowed {
        /// The column.
        column: String,
    },

    /// A duplicate key.
    #[error("duplicate key: {what}")]
    DuplicateKey {
        /// Which.
        what: String,
    },

    /// A row or object another job holds.
    #[error("locked: {what}")]
    Locked {
        /// Which.
        what: String,
    },

    /// A change under commitment control to an unjournaled table.
    #[error("{what}")]
    NotJournaled {
        /// Which.
        what: String,
    },

    /// A cursor used in the wrong state.
    #[error("cursor {cursor} {what}")]
    CursorState {
        /// The cursor.
        cursor: String,
        /// What is wrong.
        what: String,
    },

    /// A value was given for a `GENERATED ALWAYS` identity column.
    #[error("column {column} is an identity column the system generates")]
    IdentityValue {
        /// The column.
        column: String,
    },

    /// A scalar subquery with more than one row.
    #[error("the subquery returned more than one row")]
    TooManyRows,

    /// A division by zero.
    #[error("division by zero")]
    DivideByZero,

    /// Wrong number of values for the columns.
    #[error("{what}")]
    ValueCount {
        /// Which.
        what: String,
    },

    /// The user lacks the authority the statement needs.
    #[error("{what}")]
    NotAuthorized {
        /// What was refused.
        what: String,
    },

    /// The statement needed more working storage than one may hold.
    #[error("{what}")]
    ResourceLimit {
        /// What ran out.
        what: String,
    },

    /// The statement was ended by a cancel from outside it.
    #[error("the statement was cancelled")]
    Cancelled,

    /// Allowed only at the start of a unit of work: changes are not
    /// committed or rolled back.
    #[error("{what} is allowed only before the unit of work changes anything")]
    UnitOfWork {
        /// The statement.
        what: String,
    },

    /// The database failed in a way SQL has no code for.
    #[error("database error: {what}")]
    Database {
        /// Which.
        what: String,
        /// The CPF message, when there is one.
        msgid: Option<String>,
    },
}

impl SqlError {
    /// The `SQLCODE`.
    #[must_use]
    pub fn sqlcode(&self) -> i32 {
        match self {
            Self::Syntax { .. } | Self::ThreePartName { .. } => -104,
            Self::Unsupported { .. } => -84,
            Self::NameTooLong { .. } => -107,
            Self::Literal { .. } | Self::Conversion { .. } => -302,
            Self::ObjectNotFound { .. } => -204,
            Self::ColumnNotFound { .. } => -206,
            Self::AmbiguousColumn { .. } => -203,
            Self::TypeMismatch { .. } => -401,
            Self::NullNotAllowed { .. } => -407,
            Self::IdentityValue { .. } => -798,
            Self::DuplicateKey { .. } => -803,
            Self::Locked { .. } => -913,
            Self::NotJournaled { .. } => -7008,
            Self::TooManyRows => -811,
            Self::CursorState { .. } => -501,
            Self::Overflow { .. } | Self::DivideByZero => -802,
            Self::ValueCount { .. } => -117,
            Self::NotAuthorized { .. } => -551,
            Self::Cancelled => -952,
            Self::ResourceLimit { .. } => -904,
            Self::UnitOfWork { .. } => -428,
            Self::Database { .. } => -901,
        }
    }

    /// The `SQLSTATE`.
    #[must_use]
    pub fn sqlstate(&self) -> &'static str {
        match self {
            Self::Syntax { .. } | Self::ThreePartName { .. } => "42601",
            Self::Unsupported { .. } => "42612",
            Self::NameTooLong { .. } => "42622",
            Self::Literal { .. } | Self::Conversion { .. } => "22018",
            Self::ObjectNotFound { .. } => "42704",
            Self::ColumnNotFound { .. } => "42703",
            Self::AmbiguousColumn { .. } => "42702",
            Self::TypeMismatch { .. } => "42818",
            Self::Overflow { .. } | Self::DivideByZero => "22003",
            Self::NullNotAllowed { .. } => "23502",
            Self::IdentityValue { .. } => "428C9",
            Self::DuplicateKey { .. } => "23505",
            Self::Locked { .. } => "57033",
            Self::NotJournaled { .. } => "55019",
            Self::TooManyRows => "21000",
            Self::CursorState { .. } => "24501",
            Self::ValueCount { .. } => "42802",
            Self::NotAuthorized { .. } => "42501",
            Self::Cancelled => "57014",
            Self::ResourceLimit { .. } => "57011",
            Self::UnitOfWork { .. } => "25001",
            Self::Database { .. } => "58004",
        }
    }

    /// The message ID: `SQL` plus the code's four digits.
    #[must_use]
    pub fn msgid(&self) -> String {
        format!("SQL{:04}", self.sqlcode().unsigned_abs())
    }
}
