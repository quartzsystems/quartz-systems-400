// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE PROCEDURE`, `CREATE FUNCTION`, `CALL`, `DROP` of routines,
//! `SIGNAL` and `SET RESULT SETS`. The SQL PL of a body is `pl.rs`.

use super::Parser;
use crate::ast::{
    Call, CreateRoutine, DropRoutine, Ident, Language, Parm, ParmMode, ParmStyle, Returns,
    RoutineBody, RoutineKind, RoutineOptions, Signal, SignalValue, SqlAccess, Statement,
};
use crate::error::{Result, SqlError};
use crate::lex::Tok;

impl Parser {
    /// `CREATE [OR REPLACE] PROCEDURE|FUNCTION`, after `OR REPLACE`.
    pub(crate) fn create_routine(&mut self, replace: bool, start: usize) -> Result<Statement> {
        let kind = if self.eat_word("PROCEDURE") {
            RoutineKind::Procedure
        } else {
            self.expect_word("FUNCTION")?;
            RoutineKind::Function
        };
        let name = self.object_name()?;
        self.routine_definition(kind, replace, name, start)
    }

    /// A routine's parameters, options and body, after its name.
    fn routine_definition(&mut self, kind: RoutineKind, replace: bool, name: crate::ast::ObjectName, start: usize) -> Result<Statement> {
        let parms = if self.is_sym("(") { self.routine_parms(kind)? } else { Vec::new() };
        let returns = if kind == RoutineKind::Function {
            self.expect_word("RETURNS")?;
            Some(self.returns()?)
        } else {
            None
        };
        let mut options = RoutineOptions { null_input: false, ..RoutineOptions::default() };
        let mut sourced = None;
        loop {
            if self.at_end() || self.is_sym(";") {
                break;
            }
            if kind == RoutineKind::Function && self.eat_word("SOURCE") {
                let src = self.object_name()?;
                let types = if self.eat_sym("(") {
                    let mut ts = Vec::new();
                    if !self.is_sym(")") {
                        ts.push(self.type_spec()?);
                        while self.eat_sym(",") {
                            ts.push(self.type_spec()?);
                        }
                    }
                    self.expect_sym(")")?;
                    Some(ts)
                } else {
                    None
                };
                sourced = Some((src, types));
                continue;
            }
            if !self.routine_option(&mut options)? {
                break;
            }
        }
        let body_mark = self.mark();
        let body = if let Some((src, types)) = sourced {
            RoutineBody::Sourced(src, types)
        } else if options.is_external {
            RoutineBody::External
        } else if self.at_end() || self.is_sym(";") {
            return Err(self.expected("a routine body or EXTERNAL"));
        } else {
            let lang = options.language.unwrap_or(Language::Sql);
            if lang != Language::Sql {
                return Err(self.expected("EXTERNAL for a routine that is not LANGUAGE SQL"));
            }
            RoutineBody::Sql(Box::new(self.pl_stmt()?))
        };
        let body_text = self.text_from(body_mark);
        if !self.at_end() && !self.is_sym(";") {
            return Err(self.expected("end of statement"));
        }
        let text = self.text_from(start);
        Ok(Statement::CreateRoutine(Box::new(CreateRoutine {
            kind,
            replace,
            name,
            parms,
            returns,
            options,
            body,
            body_text,
            text,
        })))
    }

    /// `( [IN|OUT|INOUT] [name] type, ... )`.
    fn routine_parms(&mut self, kind: RoutineKind) -> Result<Vec<Parm>> {
        self.expect_sym("(")?;
        let mut out = Vec::new();
        if self.eat_sym(")") {
            return Ok(out);
        }
        loop {
            let mode = if self.eat_word("INOUT") {
                ParmMode::InOut
            } else if self.eat_word("OUT") {
                ParmMode::Out
            } else {
                self.eat_word("IN");
                ParmMode::In
            };
            if kind == RoutineKind::Function && mode != ParmMode::In {
                return Err(self.expected("IN: a function's parameters are input only"));
            }
            // A name, unless the type follows at once.
            let name = if self.starts_type() { None } else { Some(self.ident()?) };
            let ty = self.type_spec()?;
            if self.eat_words(&["AS", "LOCATOR"]) {
                return Err(Self::unsupported("parameters AS LOCATOR"));
            }
            let default = if self.eat_word("DEFAULT") {
                if mode == ParmMode::Out {
                    return Err(SqlError::Coded { code: -469, state: "42886", what: "an OUT parameter has no default".into() });
                }
                let m = self.mark();
                self.expr()?;
                Some(self.text_from(m))
            } else {
                None
            };
            out.push(Parm { mode, name, ty, default });
            if self.eat_sym(")") {
                return Ok(out);
            }
            self.expect_sym(",")?;
        }
    }

    /// Whether the current word starts a data type rather than a name:
    /// a type word followed by something a name could not be followed by.
    fn starts_type(&self) -> bool {
        const TYPES: &[&str] = &[
            "CHAR", "CHARACTER", "VARCHAR", "GRAPHIC", "VARGRAPHIC", "DECIMAL", "DEC", "NUMERIC", "NUM",
            "SMALLINT", "INTEGER", "INT", "BIGINT", "REAL", "DOUBLE", "FLOAT", "DATE", "TIME", "TIMESTAMP",
            "DECFLOAT", "BINARY", "VARBINARY", "BOOLEAN", "BLOB", "CLOB", "DBCLOB",
        ];
        let Tok::Word(w) = self.tok() else { return false };
        if !TYPES.contains(&w.as_str()) {
            return false;
        }
        // `name type`: the next token is a type word; `type`: it is not.
        !matches!(self.tok_at(1), Tok::Word(n) if TYPES.contains(&n.as_str()))
    }

    /// `RETURNS type [CAST FROM type]` or `RETURNS TABLE (...)`.
    fn returns(&mut self) -> Result<Returns> {
        if self.eat_word("TABLE") {
            self.expect_sym("(")?;
            let mut cols = Vec::new();
            loop {
                let n = self.ident()?;
                let t = self.type_spec()?;
                cols.push((n, t));
                if self.eat_sym(")") {
                    break;
                }
                self.expect_sym(",")?;
            }
            return Ok(Returns::Table(cols));
        }
        let t = self.type_spec()?;
        let cast = if self.eat_words(&["CAST", "FROM"]) { Some(self.type_spec()?) } else { None };
        if self.eat_words(&["AS", "LOCATOR"]) {
            return Err(Self::unsupported("RETURNS ... AS LOCATOR"));
        }
        Ok(Returns::Scalar(t, cast))
    }

    /// One clause after the parameters; false at the body.
    #[allow(clippy::too_many_lines)] // one arm per clause
    fn routine_option(&mut self, o: &mut RoutineOptions) -> Result<bool> {
        if self.eat_word("LANGUAGE") {
            let Tok::Word(w) = self.advance() else { return Err(self.expected("a language")) };
            o.language = Some(match w.as_str() {
                "SQL" => Language::Sql,
                "RPGLE" => Language::Rpgle,
                "RPG" => Language::Rpg,
                "CL" => Language::Cl,
                "C" | "CPP" => Language::C,
                "COBOL" => Language::Cobol,
                "COBOLLE" => Language::Cobolle,
                "JAVA" => Language::Java,
                "PLI" => Language::Pli,
                _ => return Err(self.expected("SQL, RPGLE, RPG, CL, C, COBOL, COBOLLE, JAVA or PLI")),
            });
        } else if self.eat_word("SPECIFIC") {
            o.specific = Some(self.object_name()?);
        } else if self.eat_word("DETERMINISTIC") {
            o.deterministic = Some(true);
        } else if self.eat_words(&["NOT", "DETERMINISTIC"]) {
            o.deterministic = Some(false);
        } else if self.eat_words(&["NO", "SQL"]) {
            o.access = Some(SqlAccess::NoSql);
        } else if self.eat_words(&["CONTAINS", "SQL"]) {
            o.access = Some(SqlAccess::Contains);
        } else if self.eat_words(&["READS", "SQL", "DATA"]) {
            o.access = Some(SqlAccess::Reads);
        } else if self.eat_words(&["MODIFIES", "SQL", "DATA"]) {
            o.access = Some(SqlAccess::Modifies);
        } else if self.eat_words(&["DYNAMIC", "RESULT", "SETS"]) || self.eat_words(&["RESULT", "SETS"]) {
            o.result_sets = u16::try_from(self.integer()?).map_err(|_| self.expected("a count of result sets"))?;
        } else if self.eat_words(&["COMMIT", "ON", "RETURN"]) {
            if self.eat_word("YES") {
                o.commit_on_return = true;
            } else {
                self.expect_word("NO")?;
            }
        } else if self.eat_words(&["PARAMETER", "STYLE"]) {
            o.style = Some(if self.eat_words(&["GENERAL", "WITH", "NULLS"]) {
                ParmStyle::GeneralNulls
            } else if self.eat_word("GENERAL") {
                ParmStyle::General
            } else if self.eat_word("SQL") {
                ParmStyle::Sql
            } else if self.eat_word("DB2SQL") {
                ParmStyle::Db2Sql
            } else if self.eat_word("DB2GENERAL") {
                ParmStyle::Db2General
            } else if self.eat_word("JAVA") {
                ParmStyle::Java
            } else {
                return Err(self.expected("SQL, GENERAL, GENERAL WITH NULLS, DB2SQL, DB2GENERAL or JAVA"));
            });
        } else if self.eat_words(&["PARAMETER", "CCSID"]) {
            let w = self.advance();
            o.other.push(format!("PARAMETER CCSID {}", w.describe()));
        } else if self.eat_word("EXTERNAL") {
            if self.eat_word("ACTION") {
                o.no_external_action = false;
                return Ok(true);
            }
            o.is_external = true;
            if self.eat_word("NAME") {
                o.external = Some(self.external_name()?);
            }
        } else if self.eat_words(&["NO", "EXTERNAL", "ACTION"]) {
            o.no_external_action = true;
        } else if self.eat_words(&["PROGRAM", "TYPE"]) {
            if self.eat_word("SUB") {
                o.program_sub = true;
            } else {
                self.expect_word("MAIN")?;
            }
        } else if self.eat_words(&["CALLED", "ON", "NULL", "INPUT"]) {
            o.null_input = false;
        } else if self.eat_words(&["RETURNS", "NULL", "ON", "NULL", "INPUT"]) {
            o.null_input = true;
        } else if self.eat_words(&["NOT", "FENCED"]) {
            o.not_fenced = true;
        } else if self.eat_word("FENCED") {
            o.not_fenced = false;
        } else if self.eat_words(&["ALLOW", "PARALLEL"]) {
            o.parallel = true;
        } else if self.eat_words(&["DISALLOW", "PARALLEL"]) {
            o.parallel = false;
        } else if self.eat_words(&["FINAL", "CALL"]) {
            o.final_call = true;
        } else if self.eat_words(&["NO", "FINAL", "CALL"]) {
            o.final_call = false;
        } else if self.eat_word("SCRATCHPAD") {
            o.scratchpad = Some(match self.tok() {
                Tok::Num(_) => u32::try_from(self.integer()?).map_err(|_| self.expected("a scratchpad length"))?,
                _ => 100,
            });
        } else if self.eat_words(&["NO", "SCRATCHPAD"]) {
            o.scratchpad = None;
        } else if self.eat_word("DBINFO") {
            o.dbinfo = true;
        } else if self.eat_words(&["NO", "DBINFO"]) {
            o.dbinfo = false;
        } else if self.eat_words(&["SET", "OPTION"]) {
            loop {
                let Tok::Word(k) = self.advance() else { return Err(self.expected("an option")) };
                self.expect_sym("=")?;
                let m = self.mark();
                // A value: a word, a special value (`*CHG`), a number or a string.
                self.eat_sym("*");
                self.advance();
                o.set_options.push((k, self.text_from(m)));
                if !self.eat_sym(",") {
                    break;
                }
            }
        } else if self.eat_words(&["OLD", "SAVEPOINT", "LEVEL"]) {
            o.new_savepoint_level = false;
        } else if self.eat_words(&["NEW", "SAVEPOINT", "LEVEL"]) {
            o.new_savepoint_level = true;
        } else if self.eat_words(&["INHERIT", "SPECIAL", "REGISTERS"]) {
            o.default_registers = false;
        } else if self.eat_words(&["DEFAULT", "SPECIAL", "REGISTERS"]) {
            o.default_registers = true;
        } else if self.eat_words(&["STATEMENT", "DETERMINISTIC"]) {
            o.other.push("STATEMENT DETERMINISTIC".into());
        } else if self.eat_word("CARDINALITY") {
            let n = self.integer()?;
            o.other.push(format!("CARDINALITY {n}"));
        } else if self.eat_words(&["NOT", "SECURED"]) || self.eat_word("SECURED") {
            o.other.push("SECURED".into());
        } else if self.eat_words(&["ALLOW", "DEBUG", "MODE"])
            || self.eat_words(&["DISALLOW", "DEBUG", "MODE"])
            || self.eat_words(&["DISABLE", "DEBUG", "MODE"])
        {
        } else if self.is_word("AUTONOMOUS") || self.is_word("WRAPPED") || self.is_words(&["CONCURRENT", "ACCESS"]) {
            let Tok::Word(w) = self.tok().clone() else { unreachable!() };
            return Err(Self::unsupported(&format!("routines {w}")));
        } else {
            return Ok(false);
        }
        Ok(true)
    }

    /// `EXTERNAL NAME`: `LIB/PGM`, `LIB.PGM`, `PGM`, `LIB/SRVPGM(ENTRY)`
    /// or any of them as a string.
    fn external_name(&mut self) -> Result<String> {
        if let Tok::Str(s) = self.tok().clone() {
            self.advance();
            return Ok(s);
        }
        let mut out = self.ident()?.value;
        if self.naming == crate::ast::Naming::Sql && self.is_sym("/") {
            return Err(crate::error::SqlError::Coded { code: -5016, state: "42833", what: format!("{out}/...: a qualified name in SQL naming takes a period") });
        }
        if self.eat_sym("/") || self.eat_sym(".") {
            out.push('/');
            out.push_str(&self.ident()?.value);
        }
        if self.eat_sym("(") {
            out.push('(');
            out.push_str(&self.ident()?.value);
            self.expect_sym(")")?;
            out.push(')');
        }
        Ok(out)
    }

    /// `CALL name [(args)]`.
    pub(crate) fn call_stmt(&mut self) -> Result<Statement> {
        self.expect_word("CALL")?;
        let name = self.object_name()?;
        let mut args = Vec::new();
        if self.eat_sym("(") && !self.eat_sym(")") {
            loop {
                // `name => value` as a table function's argument.
                args.push(self.table_arg()?);
                if self.eat_sym(")") {
                    break;
                }
                self.expect_sym(",")?;
            }
        }
        Ok(Statement::Call(Call { name, args }))
    }

    /// `DROP PROCEDURE|FUNCTION|SPECIFIC PROCEDURE|SPECIFIC FUNCTION`,
    /// after `DROP`.
    pub(crate) fn drop_routine(&mut self) -> Result<Statement> {
        let s = self.drop_routine_target()?;
        if self.is_word("RESTRICT") || self.is_word("CASCADE") {
            self.advance();
        }
        Ok(s)
    }

    /// `[SPECIFIC] PROCEDURE|FUNCTION name [(types)]`.
    fn drop_routine_target(&mut self) -> Result<Statement> {
        let specific = self.eat_word("SPECIFIC");
        let kind = if self.eat_word("PROCEDURE") {
            RoutineKind::Procedure
        } else if self.eat_word("FUNCTION") || self.eat_word("ROUTINE") {
            RoutineKind::Function
        } else {
            return Err(self.expected("PROCEDURE or FUNCTION"));
        };
        let name = self.object_name()?;
        let signature = if !specific && self.eat_sym("(") {
            let mut ts = Vec::new();
            if !self.is_sym(")") {
                ts.push(self.type_spec()?);
                while self.eat_sym(",") {
                    ts.push(self.type_spec()?);
                }
            }
            self.expect_sym(")")?;
            Some(ts)
        } else {
            None
        };
        Ok(Statement::DropRoutine(DropRoutine { kind, specific, name, signature }))
    }

    /// After `ALTER`: `[SPECIFIC] PROCEDURE|FUNCTION name [(types)]`, then
    /// `ALTER options` or `REPLACE definition`, kept as text for the
    /// routine's stored definition (`exec::routine::alter`).
    pub(crate) fn alter_routine(&mut self) -> Result<Statement> {
        let Statement::DropRoutine(target) = self.drop_routine_target()? else { unreachable!("a routine") };
        let replace = if self.eat_word("REPLACE") {
            true
        } else {
            self.expect_word("ALTER")?;
            false
        };
        let mark = self.mark();
        if replace {
            // Parsed here for its end; made from the text with the name.
            self.routine_definition(target.kind, false, target.name.clone(), mark)?;
        }
        while !self.at_end() && !self.is_sym(";") {
            // The specific name stays; a routine's options do not name one.
            if !replace && self.is_word("SPECIFIC") {
                return Err(self.expected("an option of the routine"));
            }
            self.advance();
        }
        let tail = self.text_from(mark);
        if tail.is_empty() {
            return Err(self.expected("the routine's options"));
        }
        Ok(Statement::AlterRoutine(Box::new(crate::ast::AlterRoutine { target, replace, tail })))
    }

    /// `SIGNAL` or `RESIGNAL`: the condition and its message.
    pub(crate) fn signal(&mut self) -> Result<Signal> {
        let resignal = if self.eat_word("RESIGNAL") {
            true
        } else {
            self.expect_word("SIGNAL")?;
            false
        };
        let state = if self.eat_word("SQLSTATE") {
            self.eat_word("VALUE");
            match self.tok().clone() {
                Tok::Str(s) => {
                    self.advance();
                    if s.len() != 5 || !s.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()) || s.starts_with("00") {
                        return Err(crate::error::SqlError::Coded { code: -435, state: "428B3", what: format!("SQLSTATE {s} is not one a SIGNAL may raise") });
                    }
                    Some(SignalValue::State(s))
                }
                _ => Some(SignalValue::Variable(self.pl_expr()?)),
            }
        } else if matches!(self.tok(), Tok::Word(w) if w != "SET") || matches!(self.tok(), Tok::Delimited(_)) {
            Some(SignalValue::Condition(self.ident()?))
        } else {
            None
        };
        if state.is_none() && !resignal {
            return Err(self.expected("SQLSTATE or a condition name"));
        }
        let message = if self.eat_word("SET") {
            self.expect_word("MESSAGE_TEXT")?;
            self.expect_sym("=")?;
            Some(self.pl_expr()?)
        } else if self.is_sym("(") {
            // The older form: SIGNAL SQLSTATE 'xxxxx' ('text').
            self.advance();
            let e = self.pl_expr()?;
            self.expect_sym(")")?;
            Some(e)
        } else {
            None
        };
        Ok(Signal { state, message, resignal })
    }

    /// `SET RESULT SETS`, after `SET`: `NONE`, or `[WITH RETURN [TO
    /// CALLER|CLIENT] [FOR]] CURSOR c, ...`.
    pub(crate) fn set_result_sets(&mut self) -> Result<Statement> {
        self.expect_words(&["RESULT", "SETS"])?;
        if self.eat_word("NONE") {
            return Ok(Statement::SetResultSets(Vec::new()));
        }
        if self.eat_words(&["WITH", "RETURN"]) {
            if self.eat_word("TO") {
                self.advance();
            }
            self.eat_word("FOR");
        }
        let mut out: Vec<Ident> = Vec::new();
        loop {
            if self.eat_word("ARRAY") {
                return Err(Self::unsupported("SET RESULT SETS ARRAY: result sets from host arrays"));
            }
            self.expect_word("CURSOR")?;
            out.push(self.ident()?);
            if !self.eat_sym(",") {
                break;
            }
        }
        Ok(Statement::SetResultSets(out))
    }
}
