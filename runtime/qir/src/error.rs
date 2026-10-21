// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong with an IR module: text that does not parse, a
//! type that has no layout, a module the verifier rejects.

/// Result with a [`QirError`].
pub type Result<T> = std::result::Result<T, QirError>;

/// An IR error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QirError {
    /// The text form does not parse.
    #[error("{line}:{col}: {msg}")]
    Syntax {
        /// Line, from 1.
        line: u32,
        /// Column, from 1.
        col: u32,
        /// What was wrong.
        msg: String,
    },

    /// A type that cannot be laid out.
    #[error("type {ty}: {msg}")]
    Type {
        /// The type as written.
        ty: String,
        /// What was wrong.
        msg: String,
    },

    /// The verifier rejected the module.
    #[error("{at}: {msg}")]
    Verify {
        /// Where: `module`, `proc NAME`, `proc NAME block L inst n`.
        at: String,
        /// What was wrong.
        msg: String,
    },

    /// A binary module that cannot be read.
    #[error("binary module: {msg}")]
    Binary {
        /// What was wrong.
        msg: String,
    },
}

impl QirError {
    /// A syntax error at a position.
    #[must_use]
    pub fn syntax(line: u32, col: u32, msg: impl Into<String>) -> Self {
        Self::Syntax {
            line,
            col,
            msg: msg.into(),
        }
    }

    /// A verifier error at a location.
    #[must_use]
    pub fn verify(at: impl Into<String>, msg: impl Into<String>) -> Self {
        Self::Verify {
            at: at.into(),
            msg: msg.into(),
        }
    }
}
