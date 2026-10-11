// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE`, `ALTER`, `DROP`, `LABEL ON`, `COMMENT ON`, `RENAME`.

use super::Parser;
use crate::ast::{
    AlterAction, AlterTable, ColumnDef, ColumnDefault, Constraint, CreateIndex, CreateTable,
    CreateView, Ident, Identity, Label, LabelTarget, Rename, Statement,
};
use crate::error::Result;
use crate::lex::Tok;

impl Parser {
    pub(crate) fn create(&mut self) -> Result<Statement> {
        self.expect_word("CREATE")?;
        let replace = self.eat_words(&["OR", "REPLACE"]);
        if self.eat_word("TABLE") {
            return self.create_table(replace);
        }
        let unique = self.eat_word("UNIQUE");
        let where_not_null = unique && self.eat_words(&["WHERE", "NOT", "NULL"]);
        if self.eat_words(&["ENCODED", "VECTOR"]) {
            return Err(Self::unsupported("encoded vector indexes"));
        }
        if self.eat_word("INDEX") {
            return self.create_index(unique, where_not_null);
        }
        if unique {
            return Err(self.expected("INDEX"));
        }
        if self.eat_word("VIEW") {
            return self.create_view(replace);
        }
        if self.eat_word("ALIAS") {
            let name = self.object_name()?;
            self.expect_word("FOR")?;
            let table = self.object_name()?;
            let member = if self.eat_sym("(") {
                let m = self.ident()?;
                self.expect_sym(")")?;
                Some(m)
            } else {
                None
            };
            return Ok(Statement::CreateAlias {
                name,
                table,
                member,
                replace,
            });
        }
        if self.eat_word("SCHEMA") || self.eat_word("COLLECTION") {
            return Err(Self::unsupported("CREATE SCHEMA; use CRTLIB"));
        }
        Err(Self::unsupported("that CREATE statement"))
    }

    fn create_table(&mut self, replace: bool) -> Result<Statement> {
        let name = self.object_name()?;
        let mut t = CreateTable {
            name,
            system_name: None,
            columns: Vec::new(),
            constraints: Vec::new(),
            rcdfmt: None,
            as_query: None,
            replace,
        };
        if self.eat_words(&["FOR", "SYSTEM", "NAME"]) {
            t.system_name = Some(self.ident()?);
        }
        if self.eat_word("AS") || self.eat_word("LIKE") {
            if self.is_word("LIKE") {
                return Err(Self::unsupported("CREATE TABLE LIKE"));
            }
            self.expect_sym("(")?;
            let q = self.query()?;
            self.expect_sym(")")?;
            let with_data = if self.eat_words(&["WITH", "DATA"]) {
                true
            } else {
                self.expect_words(&["WITH", "NO", "DATA"])?;
                false
            };
            t.as_query = Some((Box::new(q), with_data));
        } else {
            self.expect_sym("(")?;
            loop {
                if self.starts_constraint() {
                    let c = self.constraint()?;
                    t.constraints.push(c);
                } else {
                    let c = self.column_def()?;
                    t.columns.push(c);
                }
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
        }
        loop {
            if self.eat_word("RCDFMT") {
                t.rcdfmt = Some(self.ident()?);
            } else if self.eat_words(&["ON", "COMMIT"]) {
                return Err(Self::unsupported("declared temporary tables (ON COMMIT)"));
            } else if self.eat_words(&["IN", "ASP"]) || self.eat_word("IN") {
                self.ident()?;
            } else if self.eat_words(&["NOT", "VOLATILE"]) || self.eat_word("VOLATILE") {
            } else {
                break;
            }
        }
        Ok(Statement::CreateTable(t))
    }

    fn starts_constraint(&self) -> bool {
        self.is_word("CONSTRAINT")
            || self.is_words(&["PRIMARY", "KEY"])
            || self.is_word("UNIQUE") && matches!(self.tok_at(1), Tok::Sym("("))
            || self.is_words(&["FOREIGN", "KEY"])
            || self.is_word("CHECK") && matches!(self.tok_at(1), Tok::Sym("("))
    }

    fn constraint(&mut self) -> Result<Constraint> {
        let name = if self.eat_word("CONSTRAINT") {
            Some(self.ident()?)
        } else {
            None
        };
        if self.eat_words(&["PRIMARY", "KEY"]) {
            let columns = self.ident_list()?;
            return Ok(Constraint::PrimaryKey { name, columns });
        }
        if self.eat_word("UNIQUE") {
            let columns = self.ident_list()?;
            return Ok(Constraint::Unique { name, columns });
        }
        if self.eat_words(&["FOREIGN", "KEY"]) {
            let columns = self.ident_list()?;
            self.expect_word("REFERENCES")?;
            let parent = self.object_name()?;
            let parent_columns = if self.is_sym("(") {
                self.ident_list()?
            } else {
                Vec::new()
            };
            // ON DELETE / ON UPDATE actions are read and ignored.
            while self.eat_word("ON") {
                self.advance();
                if self.eat_word("NO") {
                    self.expect_word("ACTION")?;
                } else {
                    self.eat_word("SET");
                    self.advance();
                }
            }
            return Ok(Constraint::ForeignKey {
                name,
                columns,
                parent,
                parent_columns,
            });
        }
        if self.eat_word("CHECK") {
            self.expect_sym("(")?;
            let cond = self.expr()?;
            self.expect_sym(")")?;
            return Ok(Constraint::Check { name, cond });
        }
        Err(self.expected("PRIMARY KEY, UNIQUE, FOREIGN KEY or CHECK"))
    }

    fn column_def(&mut self) -> Result<ColumnDef> {
        let name = self.ident()?;
        let mut system_name = None;
        if self.eat_word("FOR") {
            self.eat_word("COLUMN");
            system_name = Some(self.ident()?);
        }
        let type_spec = self.type_spec()?;
        let mut c = ColumnDef {
            name,
            system_name,
            type_spec,
            not_null: false,
            default: None,
            identity: None,
            primary_key: false,
            unique: false,
        };
        loop {
            if self.eat_word("FOR") {
                self.eat_word("COLUMN");
                c.system_name = Some(self.ident()?);
            } else if self.eat_words(&["NOT", "NULL"]) {
                c.not_null = true;
            } else if self.eat_words(&["WITH", "DEFAULT"]) || self.eat_word("DEFAULT") {
                c.default = Some(self.default_value()?);
            } else if self.eat_word("GENERATED") {
                let always = if self.eat_word("ALWAYS") {
                    true
                } else {
                    self.expect_words(&["BY", "DEFAULT"])?;
                    false
                };
                self.expect_words(&["AS", "IDENTITY"])?;
                c.identity = Some(self.identity_options(always)?);
            } else if self.eat_words(&["PRIMARY", "KEY"]) {
                c.primary_key = true;
            } else if self.eat_word("UNIQUE") {
                c.unique = true;
            } else if self.eat_word("CONSTRAINT") {
                self.ident()?;
            } else if self.eat_word("CHECK") {
                self.expect_sym("(")?;
                self.expr()?;
                self.expect_sym(")")?;
            } else if self.eat_word("REFERENCES") {
                self.object_name()?;
                if self.is_sym("(") {
                    self.ident_list()?;
                }
            } else if self.eat_words(&["IMPLICITLY", "HIDDEN"]) || self.eat_word("HIDDEN") {
                return Err(Self::unsupported("hidden columns"));
            } else {
                return Ok(c);
            }
        }
    }

    /// What follows `DEFAULT`: a value, or nothing for the type's default.
    fn default_value(&mut self) -> Result<ColumnDefault> {
        match self.tok() {
            Tok::Sym("," | ")") => Ok(ColumnDefault::System),
            Tok::Word(w)
                if matches!(
                    w.as_str(),
                    "NOT"
                        | "WITH"
                        | "GENERATED"
                        | "PRIMARY"
                        | "UNIQUE"
                        | "CONSTRAINT"
                        | "CHECK"
                        | "REFERENCES"
                        | "FOR"
                        | "CCSID"
                ) =>
            {
                Ok(ColumnDefault::System)
            }
            _ => Ok(ColumnDefault::Value(self.expr()?)),
        }
    }

    fn identity_options(&mut self, always: bool) -> Result<Identity> {
        let mut id = Identity {
            always,
            start: 1,
            increment: 1,
            cycle: false,
        };
        if !self.eat_sym("(") {
            return Ok(id);
        }
        loop {
            if self.eat_words(&["START", "WITH"]) {
                id.start = self.signed_integer()?;
            } else if self.eat_words(&["INCREMENT", "BY"]) {
                id.increment = self.signed_integer()?;
            } else if self.eat_word("CYCLE") {
                id.cycle = true;
            } else if self.eat_words(&["NO", "CYCLE"]) {
                id.cycle = false;
            } else if self.eat_word("MINVALUE") || self.eat_word("MAXVALUE") {
                self.signed_integer()?;
            } else if self.eat_words(&["NO", "MINVALUE"])
                || self.eat_words(&["NO", "MAXVALUE"])
                || self.eat_words(&["NO", "CACHE"])
                || self.eat_words(&["NO", "ORDER"])
                || self.eat_word("ORDER")
            {
            } else if self.eat_word("CACHE") {
                self.integer()?;
            } else {
                break;
            }
            self.eat_sym(",");
        }
        self.expect_sym(")")?;
        Ok(id)
    }

    fn create_index(&mut self, unique: bool, where_not_null: bool) -> Result<Statement> {
        let name = self.object_name()?;
        self.expect_word("ON")?;
        let table = self.object_name()?;
        self.expect_sym("(")?;
        let mut columns = Vec::new();
        loop {
            let c = self.ident()?;
            let desc = if self.eat_word("DESC") {
                true
            } else {
                self.eat_word("ASC");
                false
            };
            columns.push((c, desc));
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        let mut rcdfmt = None;
        loop {
            if self.eat_word("RCDFMT") {
                rcdfmt = Some(self.ident()?);
            } else if self.eat_word("UNIT") || self.eat_word("PAGESIZE") {
                self.advance();
            } else if self.eat_word("WHERE") {
                return Err(Self::unsupported(
                    "CREATE INDEX ... WHERE (derived indexes)",
                ));
            } else {
                break;
            }
        }
        Ok(Statement::CreateIndex(CreateIndex {
            name,
            unique,
            where_not_null,
            table,
            columns,
            rcdfmt,
        }))
    }

    fn create_view(&mut self, replace: bool) -> Result<Statement> {
        let name = self.object_name()?;
        let columns = if self.is_sym("(") {
            self.ident_list()?
        } else {
            Vec::new()
        };
        self.expect_word("AS")?;
        let query = Box::new(self.query()?);
        let mut check_option = false;
        if self.eat_word("WITH") {
            self.eat_word("CASCADED");
            self.eat_word("LOCAL");
            self.expect_words(&["CHECK", "OPTION"])?;
            check_option = true;
        }
        if self.eat_word("RCDFMT") {
            self.ident()?;
        }
        Ok(Statement::CreateView(CreateView {
            name,
            columns,
            query,
            check_option,
            replace,
        }))
    }

    pub(crate) fn alter(&mut self) -> Result<Statement> {
        self.expect_word("ALTER")?;
        self.expect_word("TABLE")?;
        let name = self.object_name()?;
        let mut actions = Vec::new();
        loop {
            if self.eat_word("ADD") {
                if self.starts_constraint() {
                    actions.push(AlterAction::AddConstraint(self.constraint()?));
                } else {
                    self.eat_word("COLUMN");
                    actions.push(AlterAction::AddColumn(self.column_def()?));
                }
            } else if self.eat_word("DROP") {
                if self.eat_words(&["PRIMARY", "KEY"]) {
                    actions.push(AlterAction::DropPrimaryKey);
                } else if self.eat_word("CONSTRAINT")
                    || self.eat_word("UNIQUE")
                    || self.eat_words(&["FOREIGN", "KEY"])
                    || self.eat_word("CHECK")
                {
                    actions.push(AlterAction::DropConstraint(self.ident()?));
                } else {
                    self.eat_word("COLUMN");
                    let col = self.ident()?;
                    let cascade = if self.eat_word("CASCADE") {
                        true
                    } else {
                        self.eat_word("RESTRICT");
                        false
                    };
                    actions.push(AlterAction::DropColumn { name: col, cascade });
                }
            } else if self.eat_word("ALTER") {
                self.eat_word("COLUMN");
                let col = self.ident()?;
                if self.eat_words(&["SET", "DATA", "TYPE"]) {
                    actions.push(AlterAction::SetDataType(col, self.type_spec()?));
                } else if self.eat_words(&["SET", "NOT", "NULL"]) {
                    actions.push(AlterAction::SetNotNull(col, true));
                } else if self.eat_words(&["DROP", "NOT", "NULL"]) {
                    actions.push(AlterAction::SetNotNull(col, false));
                } else if self.eat_words(&["SET", "DEFAULT"])
                    || self.eat_words(&["SET", "WITH", "DEFAULT"])
                {
                    actions.push(AlterAction::SetDefault(col, Some(self.default_value()?)));
                } else if self.eat_words(&["DROP", "DEFAULT"]) {
                    actions.push(AlterAction::SetDefault(col, None));
                } else {
                    return Err(
                        self.expected("SET DATA TYPE, SET/DROP NOT NULL or SET/DROP DEFAULT")
                    );
                }
            } else {
                break;
            }
            self.eat_sym(",");
        }
        if actions.is_empty() {
            return Err(self.expected("ADD, DROP or ALTER"));
        }
        Ok(Statement::AlterTable(AlterTable { name, actions }))
    }

    pub(crate) fn drop(&mut self) -> Result<Statement> {
        self.expect_word("DROP")?;
        let Tok::Word(kind) = self.tok().clone() else {
            return Err(self.expected("TABLE, INDEX, VIEW or ALIAS"));
        };
        self.advance();
        let name = self.object_name()?;
        let cascade = if self.eat_word("CASCADE") {
            true
        } else {
            self.eat_word("RESTRICT");
            false
        };
        match kind.as_str() {
            "TABLE" => Ok(Statement::DropTable { name, cascade }),
            "INDEX" => Ok(Statement::DropIndex(name)),
            "VIEW" => Ok(Statement::DropView(name)),
            "ALIAS" => Ok(Statement::DropAlias(name)),
            "SCHEMA" | "COLLECTION" => Err(Self::unsupported("DROP SCHEMA; use DLTLIB")),
            _ => Err(self.expected("TABLE, INDEX, VIEW or ALIAS")),
        }
    }

    /// `LABEL ON` (`comment` false) or `COMMENT ON` (true).
    pub(crate) fn label(&mut self, comment: bool) -> Result<Statement> {
        self.advance();
        self.expect_word("ON")?;
        let mut l = Label {
            target: LabelTarget::Table(crate::ast::ObjectName::new("")),
            columns: Vec::new(),
            text: None,
            is_text: comment,
        };
        if self.eat_word("COLUMN") {
            let name = self.object_name_and_column()?;
            l.target = LabelTarget::Column(name.0, name.1);
            if self.eat_word("TEXT") {
                l.is_text = true;
            }
            self.expect_word("IS")?;
            l.text = Some(self.string()?);
            return Ok(if comment {
                Statement::CommentOn(l)
            } else {
                Statement::LabelOn(l)
            });
        }
        let is_table = self.eat_word("TABLE") || self.eat_word("VIEW");
        let is_index = !is_table && self.eat_word("INDEX");
        let is_alias = !is_table && !is_index && self.eat_word("ALIAS");
        if !(is_table || is_index || is_alias) {
            if self.eat_word("PACKAGE") || self.eat_word("PROCEDURE") || self.eat_word("FUNCTION") {
                return Err(Self::unsupported("labels on packages and routines"));
            }
            return Err(self.expected("TABLE, COLUMN, INDEX or ALIAS"));
        }
        let name = self.object_name()?;
        l.target = if is_index {
            LabelTarget::Index(name)
        } else if is_alias {
            LabelTarget::Alias(name)
        } else {
            LabelTarget::Table(name)
        };
        if self.eat_sym("(") {
            loop {
                let col = self.ident()?;
                if self.eat_word("TEXT") {
                    l.is_text = true;
                }
                self.expect_word("IS")?;
                let text = self.string()?;
                l.columns.push((col, text));
                if !self.eat_sym(",") {
                    break;
                }
            }
            self.expect_sym(")")?;
        } else {
            self.expect_word("IS")?;
            l.text = Some(self.string()?);
        }
        Ok(if comment {
            Statement::CommentOn(l)
        } else {
            Statement::LabelOn(l)
        })
    }

    /// `[schema.]table.column` in the naming mode's spelling.
    fn object_name_and_column(&mut self) -> Result<(crate::ast::ObjectName, Ident)> {
        let first = self.ident()?;
        let sep = self.separator();
        if self.eat_sym(sep) {
            let second = self.ident()?;
            if self.eat_sym(".") {
                let third = self.ident()?;
                return Ok((
                    crate::ast::ObjectName {
                        schema: Some(first),
                        name: second,
                    },
                    third,
                ));
            }
            if sep == "." {
                return Ok((
                    crate::ast::ObjectName {
                        schema: None,
                        name: first,
                    },
                    second,
                ));
            }
            return Err(self.expected("a column name"));
        }
        self.expect_sym(".")?;
        let column = self.ident()?;
        Ok((
            crate::ast::ObjectName {
                schema: None,
                name: first,
            },
            column,
        ))
    }

    pub(crate) fn rename(&mut self) -> Result<Statement> {
        self.expect_word("RENAME")?;
        let index = if self.eat_word("INDEX") {
            true
        } else {
            self.eat_word("TABLE");
            false
        };
        let name = self.object_name()?;
        let mut r = Rename {
            index,
            name,
            to: None,
            system_name: None,
        };
        if self.eat_word("TO") {
            r.to = Some(self.ident()?);
        }
        if self.eat_words(&["FOR", "SYSTEM", "NAME"]) {
            r.system_name = Some(self.ident()?);
        }
        if r.to.is_none() && r.system_name.is_none() {
            return Err(self.expected("TO or FOR SYSTEM NAME"));
        }
        Ok(Statement::Rename(r))
    }
}
