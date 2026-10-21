// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management errors, each with the message identifier the
//! commands report.

use thiserror::Error;

/// A work management failure.
#[derive(Debug, Error)]
pub enum Error {
    /// The store failed.
    #[error("{0}")]
    Store(#[from] store::StoreError),
    /// A payload that does not decode.
    #[error("damaged object: {0}")]
    Corrupt(String),
    /// A parameter value that is not allowed.
    #[error("{what}")]
    Value {
        /// Message identifier.
        msgid: &'static str,
        /// What is wrong.
        what: String,
    },
    /// No such object.
    #[error("{kind} {lib}/{name} not found")]
    NotFound {
        /// Object type.
        kind: &'static str,
        /// Library.
        lib: String,
        /// Name.
        name: String,
    },
}

impl Error {
    /// A bad parameter value.
    #[must_use]
    pub fn value(msgid: &'static str, what: impl Into<String>) -> Self {
        Self::Value {
            msgid,
            what: what.into(),
        }
    }

    /// The message identifier, when there is one.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        match self {
            Self::Store(e) => e.msgid(),
            Self::Corrupt(_) => Some("CPF8100"),
            Self::Value { msgid, .. } => Some(msgid),
            Self::NotFound { .. } => Some("CPF9801"),
        }
    }
}

/// A work management result.
pub type Result<T> = std::result::Result<T, Error>;
