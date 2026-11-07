// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL information of a program made with embedded SQL (kind
//! `PROGRAM`): an `OPTIONS` entry with the precompile options as
//! `PRTSQLINF` prints them, and a `STATEMENT` entry per statement.
//! See `docs/DB_FORMAT.md` §8.1 `SqlStatement`.

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// `SqlStatement` layout.
pub static STATEMENT_LAYOUT: Layout = Layout {
    name: "SqlStatement",
    size: None,
    fields: &[
        field("line", 0, 4),
        field("kind", 4, 1),
        field("reserved", 5, 3),
        Field {
            name: "text",
            offset: Some(8),
            size: None,
        },
    ],
};

/// Bytes before an `SqlStatement`'s text.
pub const STATEMENT_FIXED: usize = 8;

/// What a statement needs at run time, for `PRTSQLINF`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum StmtKind {
    /// An access plan would be built for it (queries and changes).
    #[default]
    Planned = 0,
    /// Nothing is planned (`OPEN`, `FETCH`, `CLOSE`, `PREPARE` ...).
    Plain = 1,
    /// A cursor's declaration.
    Declare = 2,
}

/// One precompiled statement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StmtDef {
    /// Its first source line.
    pub line: u32,
    /// What it needs.
    pub kind: StmtKind,
    /// The statement as `PRTSQLINF` prints it: tokens apart, host
    /// variables `:H`.
    pub text: String,
}

impl StmtDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; STATEMENT_FIXED];
        out[0..4].copy_from_slice(&self.line.to_le_bytes());
        out[4] = self.kind as u8;
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < STATEMENT_FIXED {
            return Err(DbError::Corrupt {
                what: "SQL information: statement".into(),
            });
        }
        let kind = match b[4] {
            1 => StmtKind::Plain,
            2 => StmtKind::Declare,
            _ => StmtKind::Planned,
        };
        Ok(Self {
            line: u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            kind,
            text: String::from_utf8_lossy(&b[STATEMENT_FIXED..]).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_round_trip() {
        let s = StmtDef {
            line: 12,
            kind: StmtKind::Declare,
            text: "DECLARE C1 CURSOR FOR SELECT A FROM T WHERE B = : H".into(),
        };
        assert_eq!(StmtDef::decode(&s.encode()).unwrap(), s);
    }
}
