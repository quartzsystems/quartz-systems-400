// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL PL: the statements of a routine body. An SQL statement inside a
//! body is kept as written (its `INTO` clause taken apart) and parsed
//! on its own, so the compiler can turn its variables into markers.

use super::Parser;
use crate::ast::{
    Assign, AssignSource, Compound, CursorSource, GetDiag, Handler, HandlerCond, HandlerKind, Ident,
    PlCursor, PlExpr, PlKind, PlSql, PlStmt, PlVar, ReturnTo,
};
use crate::error::{Result, SqlError};
use crate::lex::Tok;

/// Words that end a list of statements.
const LIST_END: &[&str] = &["END", "ELSE", "ELSEIF", "WHEN", "UNTIL"];

impl Parser {
    /// An expression with its text.
    pub(crate) fn pl_expr(&mut self) -> Result<PlExpr> {
        let m = self.mark();
        let expr = self.expr()?;
        Ok(PlExpr { text: self.text_from(m), expr })
    }

    /// One SQL PL statement, with its label.
    pub(crate) fn pl_stmt(&mut self) -> Result<PlStmt> {
        self.nested(|p| {
            let line = p.toks[p.at.min(p.toks.len() - 1)].line;
            let label = p.stmt_label()?;
            let kind = p.pl_kind()?;
            Ok(PlStmt { label, line, kind })
        })
    }

    /// `name:` before a statement.
    fn stmt_label(&mut self) -> Result<Option<Ident>> {
        let is_label = matches!(self.tok(), Tok::Word(_) | Tok::Delimited(_)) && matches!(self.tok_at(1), Tok::Sym(":"));
        if is_label {
            let l = self.ident()?;
            self.advance();
            return Ok(Some(l));
        }
        // `L1:BEGIN` lexes as a word and a host variable.
        if let (Tok::Word(_), Tok::Host { name, ind: None }) = (self.tok().clone(), self.tok_at(1).clone()) {
            let l = self.ident()?;
            let i = self.at;
            self.toks[i].tok = Tok::Word(name);
            return Ok(Some(l));
        }
        Ok(None)
    }

    #[allow(clippy::too_many_lines)] // one arm per statement
    fn pl_kind(&mut self) -> Result<PlKind> {
        let w = match self.tok() {
            Tok::Word(w) => w.clone(),
            Tok::Sym("(") => String::from("("),
            _ => return Err(self.expected("an SQL PL statement")),
        };
        Ok(match w.as_str() {
            "BEGIN" => PlKind::Compound(self.compound()?),
            "SET" if self.is_words(&["SET", "RESULT", "SETS"]) => {
                self.advance();
                match self.set_result_sets()? {
                    crate::ast::Statement::SetResultSets(c) => PlKind::SetResultSets(c),
                    _ => unreachable!(),
                }
            }
            "SET" if self.is_assignment() => {
                self.advance();
                PlKind::Set(self.assignments()?)
            }
            "IF" => self.if_stmt()?,
            "CASE" => self.case_stmt()?,
            "LOOP" => {
                self.advance();
                let body = self.pl_list()?;
                self.expect_words(&["END", "LOOP"])?;
                self.end_label();
                PlKind::Loop(body)
            }
            "WHILE" => {
                self.advance();
                let c = self.pl_expr()?;
                self.expect_word("DO")?;
                let body = self.pl_list()?;
                self.expect_words(&["END", "WHILE"])?;
                self.end_label();
                PlKind::While(c, body)
            }
            "REPEAT" => {
                self.advance();
                let body = self.pl_list()?;
                self.expect_word("UNTIL")?;
                let c = self.pl_expr()?;
                self.expect_words(&["END", "REPEAT"])?;
                self.end_label();
                PlKind::Repeat(body, c)
            }
            "FOR" => self.for_stmt()?,
            "LEAVE" => {
                self.advance();
                PlKind::Leave(self.ident()?)
            }
            "ITERATE" => {
                self.advance();
                PlKind::Iterate(self.ident()?)
            }
            "GOTO" => {
                self.advance();
                PlKind::Goto(self.ident()?)
            }
            "RETURN" => {
                self.advance();
                if self.at_end() || self.is_sym(";") || self.is_word("END") {
                    PlKind::Return(None)
                } else if self.is_word("SELECT") || self.is_word("WITH") || self.is_word("VALUES") || self.starts_query_paren() {
                    PlKind::ReturnQuery(self.pl_sql()?)
                } else {
                    PlKind::Return(Some(self.pl_expr()?))
                }
            }
            "CALL" => {
                self.advance();
                let name = self.object_name()?;
                let mut args = Vec::new();
                if self.eat_sym("(") && !self.eat_sym(")") {
                    loop {
                        args.push(self.pl_expr()?);
                        if self.eat_sym(")") {
                            break;
                        }
                        self.expect_sym(",")?;
                    }
                }
                PlKind::Call { name, args }
            }
            "SIGNAL" | "RESIGNAL" => PlKind::Signal(self.signal()?),
            "GET" => PlKind::GetDiag(self.get_diagnostics()?),
            "OPEN" => {
                self.advance();
                let cursor = self.ident()?;
                let mut using = Vec::new();
                if self.eat_word("USING") {
                    using = self.pl_exprs()?;
                }
                PlKind::Open { cursor, using }
            }
            "FETCH" => {
                self.advance();
                let m = self.mark();
                for o in ["NEXT", "PRIOR", "FIRST", "LAST", "BEFORE", "AFTER", "CURRENT"] {
                    if self.eat_word(o) {
                        break;
                    }
                }
                if self.eat_word("RELATIVE") || self.eat_word("ABSOLUTE") {
                    self.expr()?;
                }
                let orient = self.text_from(m);
                self.eat_word("FROM");
                let cursor = self.ident()?;
                self.expect_word("INTO")?;
                PlKind::Fetch { cursor, orient, into: self.targets()? }
            }
            "CLOSE" => {
                self.advance();
                PlKind::Close(self.ident()?)
            }
            "PREPARE" => {
                self.advance();
                let stmt = self.ident()?;
                if self.is_word("INTO") || self.is_word("USING") {
                    return Err(Self::unsupported("PREPARE INTO a descriptor in a routine"));
                }
                self.expect_word("FROM")?;
                PlKind::Prepare { stmt, from: self.pl_expr()? }
            }
            "EXECUTE" if self.is_word_at(1, "IMMEDIATE") => {
                self.advance();
                self.advance();
                PlKind::ExecImm(self.pl_expr()?)
            }
            "EXECUTE" => {
                self.advance();
                let stmt = self.ident()?;
                let into = if self.eat_word("INTO") { self.targets()? } else { Vec::new() };
                let using = if self.eat_word("USING") {
                    if self.is_word("DESCRIPTOR") {
                        return Err(Self::unsupported("EXECUTE USING DESCRIPTOR in a routine"));
                    }
                    self.pl_exprs()?
                } else {
                    Vec::new()
                };
                PlKind::Execute { stmt, into, using }
            }
            "DECLARE" if !self.is_words(&["DECLARE", "GLOBAL"]) => {
                return Err(self.expected("a statement: declarations come first in a compound statement"));
            }
            "ALLOCATE" | "ASSOCIATE" | "DESCRIBE" => {
                return Err(Self::unsupported(&format!("{w} in a routine: result set locators and descriptors")));
            }
            "PIPE" => return Err(Self::unsupported("PIPE: pipelined table functions")),
            _ => PlKind::Sql(self.pl_sql()?),
        })
    }

    /// Whether `SET` starts an assignment rather than `SET SCHEMA` and
    /// the other registers.
    fn is_assignment(&self) -> bool {
        const REGS: &[&str] = &[
            "SCHEMA", "CURRENT", "CURRENT_SCHEMA", "CURRENT_PATH", "PATH", "TRANSACTION", "OPTION",
            "SESSION", "ENCRYPTION", "CONNECTION",
        ];
        match self.tok_at(1) {
            Tok::Sym("(") | Tok::Delimited(_) => true,
            Tok::Word(w) => !REGS.contains(&w.as_str()) || matches!(self.tok_at(2), Tok::Sym("=")),
            _ => false,
        }
    }

    /// A `(` that opens a fullselect.
    fn starts_query_paren(&self) -> bool {
        self.is_sym("(") && matches!(self.tok_at(1), Tok::Word(w) if w == "SELECT" || w == "VALUES" || w == "WITH")
    }

    /// Expressions separated by commas.
    fn pl_exprs(&mut self) -> Result<Vec<PlExpr>> {
        let mut out = vec![self.pl_expr()?];
        while self.eat_sym(",") {
            out.push(self.pl_expr()?);
        }
        Ok(out)
    }

    /// Variables assigned: `v`, `label.v`, separated by commas.
    fn targets(&mut self) -> Result<Vec<Ident>> {
        let mut out = vec![self.target()?];
        while self.eat_sym(",") {
            out.push(self.target()?);
        }
        Ok(out)
    }

    fn target(&mut self) -> Result<Ident> {
        let mut t = self.ident()?;
        if self.eat_sym(".") {
            // `label.var`: kept as one name; a variable has no period.
            let v = self.ident()?;
            t = Ident { value: format!("{}.{}", t.value, v.value), delimited: false };
        }
        if self.is_sym("[") {
            return Err(Self::unsupported("assignment to an array element"));
        }
        Ok(t)
    }

    /// The assignments of `SET`.
    fn assignments(&mut self) -> Result<Vec<Assign>> {
        let mut out = Vec::new();
        loop {
            if self.eat_sym("(") {
                let targets = self.targets()?;
                self.expect_sym(")")?;
                self.expect_sym("=")?;
                if self.starts_query_paren() {
                    self.advance();
                    let q = self.pl_sql_until(&[")"])?;
                    self.expect_sym(")")?;
                    out.push(Assign { targets, source: AssignSource::Row(q) });
                } else if self.is_word("VALUES") || self.is_word("SELECT") {
                    let q = self.pl_sql()?;
                    out.push(Assign { targets, source: AssignSource::Row(q) });
                } else {
                    self.expect_sym("(")?;
                    let es = self.pl_exprs()?;
                    self.expect_sym(")")?;
                    if es.len() != targets.len() {
                        return Err(SqlError::Coded {
                            code: -117,
                            state: "42802".into(),
                            what: "the number of values differs from the number of variables".into(),
                        });
                    }
                    out.push(Assign { targets, source: AssignSource::Exprs(es) });
                }
            } else {
                let t = self.target()?;
                self.expect_sym("=")?;
                out.push(Assign { targets: vec![t], source: AssignSource::Exprs(vec![self.pl_expr()?]) });
            }
            if !self.eat_sym(",") {
                return Ok(out);
            }
        }
    }

    /// Statements each ended by `;`, up to a word in [`LIST_END`].
    fn pl_list(&mut self) -> Result<Vec<PlStmt>> {
        let mut out = Vec::new();
        while !matches!(self.tok(), Tok::Word(w) if LIST_END.contains(&w.as_str())) {
            if self.at_end() {
                return Err(self.expected("END"));
            }
            out.push(self.pl_stmt()?);
            self.expect_sym(";")?;
        }
        Ok(out)
    }

    /// The optional label after `END LOOP` and the like.
    fn end_label(&mut self) {
        if matches!(self.tok(), Tok::Word(w) if !LIST_END.contains(&w.as_str())) || matches!(self.tok(), Tok::Delimited(_)) {
            self.advance();
        }
    }

    /// `BEGIN [[NOT] ATOMIC] declarations statements END [label]`.
    fn compound(&mut self) -> Result<Compound> {
        self.expect_word("BEGIN")?;
        let atomic = if self.eat_words(&["NOT", "ATOMIC"]) { false } else { self.eat_word("ATOMIC") };
        let mut c = Compound {
            atomic,
            vars: Vec::new(),
            conditions: Vec::new(),
            statements: Vec::new(),
            cursors: Vec::new(),
            handlers: Vec::new(),
            body: Vec::new(),
        };
        // Variables and conditions first, then cursors, then handlers.
        let mut phase = 0u8;
        while self.is_word("DECLARE") && !self.is_word_at(1, "GLOBAL") {
            self.advance();
            let before = (c.cursors.len(), c.handlers.len());
            let t = &self.toks[self.at.min(self.toks.len() - 1)];
            let (line, col) = (t.line, t.col);
            self.declaration(&mut c)?;
            let kind = if c.handlers.len() > before.1 { 2 } else { u8::from(c.cursors.len() > before.0) };
            if kind < phase {
                return Err(SqlError::Coded {
                    code: -199,
                    state: "42601".into(),
                    what: format!("DECLARE at line {line} column {col}: variables and conditions come before cursors, cursors before handlers"),
                });
            }
            phase = phase.max(kind);
            self.expect_sym(";")?;
        }
        while !self.is_word("END") {
            if self.at_end() {
                return Err(self.expected("END"));
            }
            c.body.push(self.pl_stmt()?);
            self.expect_sym(";")?;
        }
        self.expect_word("END")?;
        if !self.at_end() && !self.is_sym(";") {
            self.end_label();
        }
        Ok(c)
    }

    /// One declaration, after `DECLARE`.
    fn declaration(&mut self, c: &mut Compound) -> Result<()> {
        if self.is_word("CONTINUE") || self.is_word("EXIT") || self.is_word("UNDO") {
            let kind = match self.advance() {
                Tok::Word(w) if w == "CONTINUE" => HandlerKind::Continue,
                Tok::Word(w) if w == "EXIT" => HandlerKind::Exit,
                _ => HandlerKind::Undo,
            };
            if kind == HandlerKind::Undo && !c.atomic {
                return Err(SqlError::Coded {
                    code: -780,
                    state: "428D6".into(),
                    what: "an UNDO handler needs an ATOMIC compound statement".into(),
                });
            }
            self.expect_words(&["HANDLER", "FOR"])?;
            let mut conds = vec![self.handler_cond()?];
            while self.eat_sym(",") {
                conds.push(self.handler_cond()?);
            }
            let body = Box::new(self.pl_stmt()?);
            c.handlers.push(Handler { kind, conds, body });
            return Ok(());
        }
        let first = self.ident()?;
        if self.eat_word("CONDITION") {
            self.expect_words(&["FOR"])?;
            self.eat_word("SQLSTATE");
            self.eat_word("VALUE");
            let s = self.string()?;
            c.conditions.push((first, s));
            return Ok(());
        }
        let mut names = vec![first];
        while self.eat_sym(",") {
            names.push(self.ident()?);
        }
        if self.is_word("STATEMENT") {
            // Not in SQL PL: a variable of a type of that name (IBM's reading).
            let _ = &c.statements;
            return Err(SqlError::Coded { code: -204, state: "42704".into(), what: "type STATEMENT not found".into() });
        }
        let cursor_words = ["ASENSITIVE", "INSENSITIVE", "SENSITIVE", "NO", "SCROLL", "DYNAMIC", "CURSOR"];
        if names.len() == 1 && matches!(self.tok(), Tok::Word(w) if cursor_words.contains(&w.as_str())) {
            let cur = self.cursor_decl(names.remove(0))?;
            c.cursors.push(cur);
            return Ok(());
        }
        let ty = self.type_spec()?;
        let mut v = PlVar { names, ty, default: None, constant: false };
        if self.eat_word("DEFAULT") {
            v.default = Some(self.pl_expr()?);
        } else if self.eat_word("CONSTANT") {
            v.constant = true;
            v.default = Some(self.pl_expr()?);
        }
        c.vars.push(v);
        Ok(())
    }

    /// A cursor's declaration after its name.
    fn cursor_decl(&mut self, name: Ident) -> Result<PlCursor> {
        let mut scroll = false;
        loop {
            if self.eat_word("SCROLL") {
                scroll = true;
            } else if self.eat_words(&["NO", "SCROLL"]) || self.eat_word("ASENSITIVE") || self.eat_word("DYNAMIC") {
            } else if self.is_word("INSENSITIVE") || self.is_word("SENSITIVE") {
                // A cursor over its own copy of the rows either way.
                self.advance();
            } else {
                break;
            }
        }
        self.expect_word("CURSOR")?;
        let (mut hold, mut returns) = (false, None);
        loop {
            if self.eat_words(&["WITH", "HOLD"]) {
                hold = true;
            } else if self.eat_words(&["WITHOUT", "HOLD"]) || self.eat_words(&["WITHOUT", "RETURN"]) {
            } else if self.eat_words(&["WITH", "RETURN"]) {
                returns = Some(if self.eat_words(&["TO", "CLIENT"]) {
                    ReturnTo::Client
                } else {
                    self.eat_words(&["TO", "CALLER"]);
                    ReturnTo::Caller
                });
            } else {
                break;
            }
        }
        self.expect_word("FOR")?;
        let query = if matches!(self.tok(), Tok::Word(w) if w != "SELECT" && w != "VALUES" && w != "WITH") {
            CursorSource::Statement(self.ident()?)
        } else {
            CursorSource::Query(self.pl_sql()?)
        };
        Ok(PlCursor { name, hold, scroll, returns, query })
    }

    /// A handler's condition.
    fn handler_cond(&mut self) -> Result<HandlerCond> {
        if self.eat_word("SQLEXCEPTION") {
            return Ok(HandlerCond::Exception);
        }
        if self.eat_word("SQLWARNING") {
            return Ok(HandlerCond::Warning);
        }
        if self.eat_words(&["NOT", "FOUND"]) {
            return Ok(HandlerCond::NotFound);
        }
        if self.eat_word("SQLSTATE") {
            self.eat_word("VALUE");
            return Ok(HandlerCond::State(self.string()?));
        }
        Ok(HandlerCond::Name(self.ident()?))
    }

    /// `IF c THEN ... [ELSEIF c THEN ...] [ELSE ...] END IF`.
    fn if_stmt(&mut self) -> Result<PlKind> {
        self.expect_word("IF")?;
        let mut branches = Vec::new();
        loop {
            let c = self.pl_expr()?;
            self.expect_word("THEN")?;
            branches.push((c, self.pl_list()?));
            if !self.eat_word("ELSEIF") {
                break;
            }
        }
        let otherwise = if self.eat_word("ELSE") { Some(self.pl_list()?) } else { None };
        self.expect_words(&["END", "IF"])?;
        Ok(PlKind::If { branches, otherwise })
    }

    /// `CASE [operand] WHEN ... THEN ... [ELSE ...] END CASE`.
    fn case_stmt(&mut self) -> Result<PlKind> {
        self.expect_word("CASE")?;
        let operand = if self.is_word("WHEN") { None } else { Some(self.pl_expr()?) };
        let mut whens = Vec::new();
        while self.eat_word("WHEN") {
            let v = self.pl_expr()?;
            self.expect_word("THEN")?;
            whens.push((v, self.pl_list()?));
        }
        if whens.is_empty() {
            return Err(self.expected("WHEN"));
        }
        let otherwise = if self.eat_word("ELSE") { Some(self.pl_list()?) } else { None };
        self.expect_words(&["END", "CASE"])?;
        Ok(PlKind::Case { operand, whens, otherwise })
    }

    /// `FOR v AS [c [cursor options] CURSOR FOR] query DO ... END FOR`.
    fn for_stmt(&mut self) -> Result<PlKind> {
        self.expect_word("FOR")?;
        let var = self.ident()?;
        self.expect_word("AS")?;
        let cursor = if !self.is_word("SELECT") && !self.is_word("VALUES") && !self.is_word("WITH") && !self.is_sym("(") {
            let c = self.ident()?;
            while !self.is_word("CURSOR") && !self.at_end() {
                self.advance();
            }
            self.expect_word("CURSOR")?;
            if self.eat_words(&["WITH", "HOLD"]) {
                return Err(Self::unsupported("FOR ... CURSOR WITH HOLD"));
            }
            self.expect_word("FOR")?;
            Some(c)
        } else {
            None
        };
        let query = self.pl_sql_until(&["DO"])?;
        self.expect_word("DO")?;
        let body = self.pl_list()?;
        self.expect_words(&["END", "FOR"])?;
        self.end_label();
        Ok(PlKind::For { var, cursor, query, body })
    }

    /// `GET [CURRENT|STACKED] DIAGNOSTICS [CONDITION|EXCEPTION n] v = ITEM, ...`.
    fn get_diagnostics(&mut self) -> Result<GetDiag> {
        self.expect_word("GET")?;
        if self.eat_word("STACKED") {
            return Err(Self::unsupported("GET STACKED DIAGNOSTICS"));
        }
        self.eat_word("CURRENT");
        self.expect_word("DIAGNOSTICS")?;
        let condition = if self.eat_word("CONDITION") || self.eat_word("EXCEPTION") {
            let n = u32::try_from(self.integer()?).map_err(|_| self.expected("a condition number"))?;
            Some(n)
        } else {
            None
        };
        let mut items = Vec::new();
        loop {
            let t = self.target()?;
            self.expect_sym("=")?;
            let Tok::Word(item) = self.advance() else {
                return Err(self.expected("a diagnostics item"));
            };
            items.push((t, item));
            if !self.eat_sym(",") {
                break;
            }
        }
        Ok(GetDiag { condition, items })
    }

    /// An SQL statement, to the `;` that ends it.
    fn pl_sql(&mut self) -> Result<PlSql> {
        self.pl_sql_until(&[";"])
    }

    /// An SQL statement up to a `;`, `END` of the routine, or one of
    /// `stops` at parenthesis depth 0; `SELECT|VALUES ... INTO` targets
    /// are taken out of its text.
    #[allow(clippy::many_single_char_names)] // token positions
    fn pl_sql_until(&mut self, stops: &[&str]) -> Result<PlSql> {
        let start = self.at;
        let mut depth = 0i32;
        let mut into: Option<(usize, usize)> = None;
        let is_query = matches!(self.tok(), Tok::Word(w) if w == "SELECT" || w == "VALUES" || w == "WITH");
        let mut i = start;
        loop {
            let t = self.tok_at(i - self.at).clone();
            match &t {
                Tok::End => break,
                Tok::Sym(s) if depth == 0 && (*s == ";" || stops.contains(s)) => break,
                Tok::Word(w) if depth == 0 && stops.contains(&w.as_str()) => break,
                Tok::Sym("(") => depth += 1,
                Tok::Sym(")") => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                Tok::Word(w) if depth == 0 && is_query && into.is_none() && w == "INTO" => {
                    // The targets run to FROM (or the end).
                    let mut j = i + 1;
                    loop {
                        match self.tok_at(j - self.at) {
                            Tok::Word(x) if x == "FROM" || x == "WHERE" || x == "GROUP" || x == "ORDER" => break,
                            Tok::Sym(";") | Tok::End => break,
                            Tok::Sym(s) if stops.contains(s) => break,
                            _ => j += 1,
                        }
                    }
                    into = Some((i, j));
                    i = j;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        let end = i;
        let piece = |p: &Self, a: usize, b: usize| p.src.get(p.offset_of(a)..p.offset_of(b).max(p.offset_of(a))).unwrap_or("").to_string();
        let (text, targets) = match into {
            Some((a, b)) => {
                let mut tp = Parser::new(&piece(self, a + 1, b), self.naming)?;
                let targets = tp.targets()?;
                (format!("{} {}", piece(self, start, a).trim_end(), piece(self, b, end).trim_start()), targets)
            }
            None => (piece(self, start, end), Vec::new()),
        };
        let text = text.trim().to_string();
        let mut sub = Parser::new(&text, self.naming)?;
        sub.params = self.params;
        let stmt = sub.statement().map_err(|e| self.relocate(e, start))?;
        self.at = end;
        Ok(PlSql { text, stmt, into: targets })
    }

    /// A syntax error in a statement parsed apart, at its place in the
    /// routine.
    fn relocate(&self, e: SqlError, start: usize) -> SqlError {
        match e {
            SqlError::Syntax { line, col, found, expected } => {
                let base = &self.toks[start.min(self.toks.len() - 1)];
                SqlError::Syntax {
                    line: base.line + line - 1,
                    col: if line == 1 { base.col + col - 1 } else { col },
                    found,
                    expected,
                }
            }
            other => other,
        }
    }
}
