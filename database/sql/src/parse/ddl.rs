// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE`, `ALTER`, `DROP`, `LABEL ON`, `COMMENT ON`, `RENAME`.

use super::Parser;
use crate::ast::{
    ColumnDef, ColumnDefault, CreateIndex, CreateTable, CreateView, Ident, Label, LabelTarget, Rename,
    Statement,
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
        let encoded_vector = !unique && self.eat_words(&["ENCODED", "VECTOR"]);
        if self.eat_word("INDEX") {
            if replace {
                return Err(self.expected("VIEW, TABLE, ALIAS or another object OR REPLACE allows"));
            }
            return self.create_index(unique, where_not_null, encoded_vector);
        }
        if encoded_vector {
            return Err(self.expected("INDEX"));
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
        if self.eat_word("SEQUENCE") {
            return self.create_sequence(replace);
        }
        if self.eat_word("SCHEMA") || self.eat_word("COLLECTION") {
            let name = self.ident()?;
            if self.is_word("AUTHORIZATION") {
                return Err(Self::unsupported("CREATE SCHEMA AUTHORIZATION"));
            }
            return Ok(Statement::CreateSchema(name));
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
            like: None,
            preserve_rows: true,
            column_names: Vec::new(),
        };
        if self.eat_words(&["FOR", "SYSTEM", "NAME"]) {
            t.system_name = Some(self.ident()?);
        }
        // `(a, b) AS (query)`: names only.
        if self.is_sym("(")
            && matches!(self.tok_at(1), Tok::Word(_) | Tok::Delimited(_))
            && matches!(self.tok_at(2), Tok::Sym("," | ")"))
            && !self.is_word_at(1, "LIKE")
        {
            t.column_names = self.ident_list()?;
            if !self.is_word("AS") {
                return Err(self.expected("AS"));
            }
        }
        if self.eat_word("LIKE") {
            t.like = Some((self.object_name()?, false));
            self.copy_options()?;
        } else if self.eat_word("AS") {
            self.expect_sym("(")?;
            let q = self.query()?;
            self.expect_sym(")")?;
            let with_data = if self.eat_words(&["WITH", "DATA"]) {
                true
            } else if self.eat_words(&["DEFINITION", "ONLY"]) {
                false
            } else {
                self.expect_words(&["WITH", "NO", "DATA"])?;
                false
            };
            self.copy_options()?;
            t.as_query = Some((Box::new(q), with_data));
        } else if self.is_sym("(") && self.is_word_at(1, "LIKE") {
            self.expect_sym("(")?;
            self.expect_word("LIKE")?;
            t.like = Some((self.object_name()?, true));
            self.copy_options()?;
            self.expect_sym(")")?;
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
            } else if self.eat_words(&["ON", "REPLACE"]) {
                if !t.replace {
                    return Err(self.expected("OR REPLACE before ON REPLACE"));
                }
                t.preserve_rows = if self.eat_word("PRESERVE") {
                    true
                } else {
                    self.expect_word("DELETE")?;
                    false
                };
                self.expect_word("ROWS")?;
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

    pub(crate) fn column_def(&mut self) -> Result<ColumnDef> {
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
            generated: None,
            hidden: false,
            constraints: Vec::new(),
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
                self.generated(&mut c)?;
            } else if self.eat_words(&["IMPLICITLY", "HIDDEN"]) {
                c.hidden = true;
            } else if let Some(k) = self.column_constraint(&c)? {
                c.constraints.push(k);
            } else {
                return Ok(c);
            }
        }
    }
    /// What follows `DEFAULT`: a value, or nothing for the type's default.
    pub(crate) fn default_value(&mut self) -> Result<ColumnDefault> {
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

    fn create_index(&mut self, unique: bool, where_not_null: bool, encoded_vector: bool) -> Result<Statement> {
        let name = self.object_name()?;
        self.expect_word("ON")?;
        let table = self.object_name()?;
        self.expect_sym("(")?;
        let mut columns = Vec::new();
        loop {
            // A key is a column; an expression key is a derived-key index.
            if !matches!(self.tok_at(1), Tok::Sym("," | ")") | Tok::Word(_)) || self.is_word_at(1, "AS") {
                // BLOCKED(sql): derived keys need key expressions in the
                // database's access paths.
                return Err(Self::unsupported("an expression as an index key (derived-key indexes)"));
            }
            let c = self.ident()?;
            let desc = if self.eat_word("DESC") {
                true
            } else {
                self.eat_word("ASC");
                false
            };
            if !self.is_sym(",") && !self.is_sym(")") {
                return Err(Self::unsupported("an expression as an index key (derived-key indexes)"));
            }
            columns.push((c, desc));
            if !self.eat_sym(",") {
                break;
            }
        }
        self.expect_sym(")")?;
        let mut rcdfmt = None;
        let mut sparse = None;
        let mut includes = Vec::new();
        loop {
            if self.eat_word("RCDFMT") {
                rcdfmt = Some(self.ident()?);
            } else if self.eat_word("UNIT") {
                // Media preference: accepted, no effect.
                self.advance();
            } else if self.eat_word("PAGESIZE") {
                // A page size hint: accepted, no effect.
                self.integer()?;
            } else if encoded_vector && self.eat_word("INCLUDE") {
                self.expect_sym("(")?;
                loop {
                    let mark = self.mark();
                    self.expr()?;
                    includes.push(self.text_from(mark));
                    if !self.eat_sym(",") {
                        break;
                    }
                }
                self.expect_sym(")")?;
            } else if self.is_word("INCLUDE") {
                return Err(self.expected("WHERE, RCDFMT or the end of the statement"));
            } else if self.eat_word("WHERE") {
                let mark = self.mark();
                let cond = self.expr()?;
                sparse = Some((cond, self.text_from(mark)));
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
            sparse,
            encoded_vector,
            includes,
        }))
    }

    fn create_view(&mut self, replace: bool) -> Result<Statement> {
        let name = self.object_name()?;
        let system_name = if self.eat_words(&["FOR", "SYSTEM", "NAME"]) { Some(self.ident()?) } else { None };
        let columns = if self.is_sym("(") {
            self.ident_list()?
        } else {
            Vec::new()
        };
        self.expect_word("AS")?;
        let mark = self.mark();
        let query = Box::new(self.query()?);
        let query_text = self.text_from(mark);
        let mut check_option = false;
        let mut local = false;
        if self.eat_word("WITH") {
            if !self.eat_word("CASCADED") {
                local = self.eat_word("LOCAL");
            }
            self.expect_words(&["CHECK", "OPTION"])?;
            check_option = true;
        }
        let rcdfmt = if self.eat_word("RCDFMT") { Some(self.ident()?) } else { None };
        Ok(Statement::CreateView(CreateView {
            name,
            columns,
            query,
            check_option,
            local,
            replace,
            system_name,
            query_text,
            rcdfmt,
        }))
    }

    pub(crate) fn drop(&mut self) -> Result<Statement> {
        self.expect_word("DROP")?;
        let Tok::Word(kind) = self.tok().clone() else {
            return Err(self.expected("TABLE, INDEX, VIEW, ALIAS or SCHEMA"));
        };
        self.advance();
        if kind == "SCHEMA" || kind == "COLLECTION" {
            let name = self.ident()?;
            let cascade = self.eat_word("CASCADE");
            let restrict = !cascade && self.eat_word("RESTRICT");
            return Ok(Statement::DropSchema { name, cascade, restrict });
        }
        let name = self.object_name()?;
        let behaviour = |p: &mut Self| {
            let cascade = p.eat_word("CASCADE");
            let restrict = !cascade && p.eat_word("RESTRICT");
            (cascade, restrict)
        };
        match kind.as_str() {
            "TABLE" => {
                let (cascade, restrict) = behaviour(self);
                Ok(Statement::DropTable { name, cascade, restrict })
            }
            "VIEW" => {
                let (cascade, restrict) = behaviour(self);
                Ok(Statement::DropView { name, cascade, restrict })
            }
            "INDEX" => Ok(Statement::DropIndex(name)),
            "ALIAS" => Ok(Statement::DropAlias(name)),
            "SEQUENCE" => Ok(Statement::DropSequence { name }),
            _ => Err(self.expected("TABLE, INDEX, VIEW, ALIAS, SEQUENCE or SCHEMA")),
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
                let is_text = comment || self.eat_word("TEXT");
                self.expect_word("IS")?;
                let text = self.string()?;
                l.columns.push((col, text, is_text));
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
        self.expect_word("TO")?;
        if self.eat_words(&["SYSTEM", "NAME"]) {
            r.system_name = Some(self.ident()?);
        } else {
            r.to = Some(self.ident()?);
            if self.eat_words(&["FOR", "SYSTEM", "NAME"]) {
                r.system_name = Some(self.ident()?);
            }
        }
        Ok(Statement::Rename(r))
    }
}
