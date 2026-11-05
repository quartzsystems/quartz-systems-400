// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements of the data types: `SET CURRENT DECFLOAT ROUNDING MODE`.

use qdecimal::DfRounding;

use super::Parser;
use crate::ast::Statement;
use crate::error::{Result, SqlError};
use crate::lex::Tok;

impl Parser {
    /// After `SET CURRENT DECFLOAT ROUNDING MODE`: `[=] ROUND_x` or a
    /// string naming one. Another word is an unknown name (`SQL0206`, as
    /// the primary reports `ROUND_SIDEWAYS`).
    pub(crate) fn set_decfloat_rounding(&mut self) -> Result<Statement> {
        self.eat_sym("=");
        let name = match self.tok().clone() {
            Tok::Word(w) | Tok::Str(w) => w,
            Tok::Host { name, .. } => {
                return Err(Self::unsupported(&format!(
                    "the rounding mode from host variable :{name} (embedded SQL)"
                )));
            }
            _ => return Err(self.expected("a DECFLOAT rounding mode")),
        };
        self.advance();
        DfRounding::parse(&name)
            .map(Statement::SetDecfloatRounding)
            .ok_or(SqlError::ColumnNotFound { name })
    }
}
