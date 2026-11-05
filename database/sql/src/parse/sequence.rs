// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE SEQUENCE`, `ALTER SEQUENCE`, `NEXT VALUE FOR` and
//! `PREVIOUS VALUE FOR`.

use super::Parser;
use crate::ast::{CreateSequence, Expr, Identity, Statement};
use crate::error::Result;

impl Parser {
    /// After `CREATE [OR REPLACE] SEQUENCE`.
    pub(crate) fn create_sequence(&mut self, replace: bool) -> Result<Statement> {
        let name = self.object_name()?;
        let mut s = CreateSequence { name, system_name: None, data_type: None, options: Identity::default(), replace };
        if self.eat_words(&["FOR", "SYSTEM", "NAME"]) {
            s.system_name = Some(self.ident()?);
        }
        loop {
            let before = self.at;
            if self.eat_word("AS") {
                s.data_type = Some(self.type_spec()?);
            }
            self.identity_options(&mut s.options, false)?;
            if self.at == before {
                break;
            }
        }
        Ok(Statement::CreateSequence(s))
    }

    /// After `ALTER SEQUENCE`.
    pub(crate) fn alter_sequence(&mut self) -> Result<Statement> {
        let name = self.object_name()?;
        let mut options = Identity::default();
        if self.is_word("AS") {
            return Err(Self::unsupported("ALTER SEQUENCE ... AS data-type"));
        }
        self.identity_options(&mut options, true)?;
        if options == Identity::default() {
            return Err(self.expected("RESTART, INCREMENT BY, MINVALUE, MAXVALUE, CYCLE, CACHE or ORDER"));
        }
        Ok(Statement::AlterSequence { name, options })
    }

    /// `NEXT VALUE FOR s` or `PREVIOUS VALUE FOR s` (also `NEXTVAL FOR`,
    /// `PREVVAL FOR`), when one starts here.
    pub(crate) fn sequence_reference(&mut self) -> Result<Option<Expr>> {
        let next = if self.eat_words(&["NEXT", "VALUE", "FOR"]) || self.eat_words(&["NEXTVAL", "FOR"]) {
            true
        } else if self.eat_words(&["PREVIOUS", "VALUE", "FOR"]) || self.eat_words(&["PREVVAL", "FOR"]) {
            false
        } else {
            return Ok(None);
        };
        let name = self.object_name()?;
        Ok(Some(if next { Expr::NextValue(name) } else { Expr::PrevValue(name) }))
    }
}
