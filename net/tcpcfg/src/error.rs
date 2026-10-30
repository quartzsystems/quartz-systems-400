// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A refusal: the message identifier a command sends and its text.

/// A refused request: the escape message's identifier and our text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{msgid} {text}")]
pub struct Error {
    /// `TCP....` or `CPF....`.
    pub msgid: String,
    /// The message text, Quartz-written.
    pub text: String,
}

impl Error {
    /// An error with an identifier and text.
    pub fn new(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
        }
    }
}

/// A result with [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
