// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Security errors, each with the message identifier a command reports.

use thiserror::Error;

/// A security failure.
#[derive(Debug, Error)]
pub enum SecError {
    /// The store failed, or refused for authority.
    #[error("{0}")]
    Store(#[from] store::StoreError),
    /// A payload that does not decode.
    #[error("damaged object: {0}")]
    Corrupt(String),
    /// A refusal or a bad value, with its message.
    #[error("{msgid} {text}")]
    Msg {
        /// Message identifier.
        msgid: &'static str,
        /// Quartz-written text.
        text: String,
    },
}

impl SecError {
    /// A refusal or a bad value.
    pub fn msg(msgid: &'static str, text: impl Into<String>) -> Self {
        Self::Msg {
            msgid,
            text: text.into(),
        }
    }

    /// The message identifier, when there is one.
    #[must_use]
    pub fn msgid(&self) -> Option<&'static str> {
        match self {
            Self::Store(e) => e.msgid(),
            Self::Corrupt(_) => Some("CPF2204"),
            Self::Msg { msgid, .. } => Some(msgid),
        }
    }
}

/// A security result.
pub type Result<T, E = SecError> = std::result::Result<T, E>;
