// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A refused registration request: IBM's message identifier and its
//! substitution data, as the APIs' error code and the commands' escape
//! report it.

use ipc::wire_struct;

use crate::text;

/// A failure with its message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{msgid}: {detail}")]
pub struct RegFail {
    /// `CPF3Cxx`.
    pub msgid: String,
    /// Substitution data, CP037 and big-endian binary.
    pub data: Vec<u8>,
    /// What went wrong, for logs and the service's text.
    pub detail: String,
}

wire_struct!(RegFail {
    msgid,
    data,
    detail
});

impl RegFail {
    /// A failure with no data.
    #[must_use]
    pub fn id(msgid: &str, detail: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            data: Vec::new(),
            detail: detail.into(),
        }
    }

    /// Append a character field.
    #[must_use]
    pub fn text(mut self, s: &str, n: usize) -> Self {
        self.data.extend(text::fixed(s, n));
        self
    }

    /// Append a binary field.
    #[must_use]
    pub fn bin4(mut self, v: i32) -> Self {
        self.data.extend(v.to_be_bytes());
        self
    }

    /// `CPF3CDB`: no such exit point and format.
    #[must_use]
    pub fn no_point(name: &str, format: &str) -> Self {
        Self::id(
            "CPF3CDB",
            format!("exit point {name} format {format} does not exist"),
        )
        .text(name, 20)
        .text(format, 8)
    }

    /// `CPF3CDA`: the repository cannot be used.
    #[must_use]
    pub fn unavailable(why: &str) -> Self {
        Self::id("CPF3CDA", why)
    }
}

/// What a registration request answers.
pub type RegResult<T> = Result<T, RegFail>;
