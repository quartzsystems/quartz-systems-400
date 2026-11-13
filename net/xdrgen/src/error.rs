// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiler's one error: a position and what is wrong there.

/// A position in a description, 1-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    /// The line.
    pub line: u32,
    /// The column.
    pub col: u32,
}

impl std::fmt::Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// What is wrong with a description, and where.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{pos}: {msg}")]
pub struct Error {
    /// The description's file name.
    pub file: String,
    /// Where.
    pub pos: Pos,
    /// What.
    pub msg: String,
}

/// An error without its file name yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct At {
    /// Where.
    pub pos: Pos,
    /// What.
    pub msg: String,
}

impl At {
    /// An error at `pos`.
    pub fn new(pos: Pos, msg: impl Into<String>) -> Self {
        Self {
            pos,
            msg: msg.into(),
        }
    }

    /// The error in `file`.
    pub fn in_file(self, file: &str) -> Error {
        Error {
            file: file.to_string(),
            pos: self.pos,
            msg: self.msg,
        }
    }
}

/// A result of the compiler's stages.
pub type Result<T> = std::result::Result<T, At>;
