// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One error type for the database layer. Variants with an IBM message
//! ID report it through [`DbError::msgid`]; the text is Quartz's.

use store::StoreError;

/// Result with a [`DbError`].
pub type Result<T> = std::result::Result<T, DbError>;

/// Why a database operation failed.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),

    /// The object is not a database file.
    #[error("{name} in library {library} is not a database file")]
    NotAFile {
        /// Object name.
        name: String,
        /// Library name.
        library: String,
    },

    /// No member of that name.
    #[error("member {member} not found in file {file}")]
    MemberNotFound {
        /// File name.
        file: String,
        /// Member name.
        member: String,
    },

    /// A member of that name exists.
    #[error("member {member} already exists in file {file}")]
    MemberExists {
        /// File name.
        file: String,
        /// Member name.
        member: String,
    },

    /// `MAXMBRS` reached.
    #[error("file {file} already has its maximum of {max} members")]
    TooManyMembers {
        /// File name.
        file: String,
        /// The maximum.
        max: u32,
    },

    /// `SIZE` reached.
    #[error("member {member} is full at {records} records")]
    MemberFull {
        /// Member name.
        member: String,
        /// Records it holds.
        records: u64,
    },

    /// No record at that relative record number.
    #[error("record {rrn} not found")]
    RecordNotFound {
        /// The RRN asked for.
        rrn: u64,
    },

    /// The record was deleted.
    #[error("record {rrn} is deleted")]
    RecordDeleted {
        /// The RRN asked for.
        rrn: u64,
    },

    /// A record of the wrong size for its format.
    #[error("record of {got} bytes for a format of {want}")]
    RecordLength {
        /// Bytes the format takes.
        want: usize,
        /// Bytes given.
        got: usize,
    },

    /// A definition the file cannot hold.
    #[error("file definition: {what}")]
    Definition {
        /// What is wrong.
        what: String,
    },

    /// On-disk structures that make no sense.
    #[error("database object {what}")]
    Corrupt {
        /// What was wrong.
        what: String,
    },
}

impl DbError {
    /// The message ID a later phase sends for this error, if any.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        Some(match self {
            Self::Store(e) => return e.msgid(),
            Self::NotAFile { .. } => "CPF9812",
            Self::MemberNotFound { .. } => "CPF9815",
            Self::MemberExists { .. } => "CPF5812",
            Self::TooManyMembers { .. } => "CPF7306",
            Self::RecordNotFound { .. } | Self::RecordDeleted { .. } => "CPF5006",
            Self::Corrupt { .. } => "CPF3113",
            Self::MemberFull { .. } | Self::RecordLength { .. } | Self::Definition { .. } => {
                return None;
            }
        })
    }
}
