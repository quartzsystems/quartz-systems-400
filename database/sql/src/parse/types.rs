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

impl Parser {
    /// `NCHAR(n)`, `NVARCHAR(n)`, `NCHAR VARYING(n)`, `NCLOB(n)` and the
    /// `NATIONAL CHARACTER` spellings: `GRAPHIC`, `VARGRAPHIC` and
    /// `DBCLOB` in CCSID 1200, as IBM defines them. `w` is consumed.
    pub(crate) fn national_type(&mut self, w: &str) -> Result<crate::ast::TypeSpec> {
        use crate::ast::DataType;
        let w = if w == "NATIONAL" {
            if !(self.eat_word("CHARACTER") || self.eat_word("CHAR")) {
                return Err(self.expected("CHARACTER after NATIONAL"));
            }
            "NCHAR"
        } else {
            w
        };
        let data_type = match w {
            "NCLOB" => DataType::Dbclob(self.lob_length()?),
            "NCHAR" if self.eat_words(&["LARGE", "OBJECT"]) => DataType::Dbclob(self.lob_length()?),
            "NVARCHAR" => DataType::Vargraphic(self.length_args()?.ok_or_else(|| self.expected("a length"))?.0),
            _ if self.eat_word("VARYING") => {
                DataType::Vargraphic(self.length_args()?.ok_or_else(|| self.expected("a length"))?.0)
            }
            _ => DataType::Graphic(self.length_args()?.map_or(1, |(n, _)| n)),
        };
        Ok(crate::ast::TypeSpec { data_type, ccsid: Some(1200), bit_data: false })
    }
}
