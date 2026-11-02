// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements the test database runs: `SELECT` of `*` or columns
//! with `WHERE col = x AND ...`, `INSERT ... VALUES`, `UPDATE ... SET`
//! and `DELETE`, each over one table; `x` is `?` or a literal. Anything
//! else is a syntax error (`SQL0104`), as the parser of a database that
//! does not know the statement would say.

/// A name as written: `schema.table`, `lib/file` or bare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRef {
    /// The schema or library, when qualified.
    pub schema: Option<String>,
    /// The table.
    pub name: String,
    /// Qualified with `/` (system naming).
    pub slash: bool,
}

/// A literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    /// `NULL`.
    Null,
    /// A number as written.
    Num(String),
    /// A string.
    Str(String),
}

/// A value in a statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// The `n`th parameter marker, from 0.
    Param(usize),
    /// A literal.
    Lit(Lit),
}

/// `col = x` terms joined by `AND`.
pub type Filter = Vec<(String, Expr)>;

/// A statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `SELECT`; `None` columns for `*`.
    Select {
        /// The columns.
        cols: Option<Vec<String>>,
        /// The table.
        table: TableRef,
        /// The `WHERE`.
        filter: Filter,
    },
    /// `INSERT INTO t [(cols)] VALUES (...)`.
    Insert {
        /// The table.
        table: TableRef,
        /// The columns named; `None` for all.
        cols: Option<Vec<String>>,
        /// The values.
        values: Vec<Expr>,
    },
    /// `UPDATE t SET c = x, ... [WHERE ...]`.
    Update {
        /// The table.
        table: TableRef,
        /// The assignments.
        sets: Vec<(String, Expr)>,
        /// The `WHERE`.
        filter: Filter,
    },
    /// `DELETE FROM t [WHERE ...]`.
    Delete {
        /// The table.
        table: TableRef,
        /// The `WHERE`.
        filter: Filter,
    },
}

impl Stmt {
    /// The table it names.
    #[must_use]
    pub fn table(&self) -> &TableRef {
        match self {
            Stmt::Select { table, .. } | Stmt::Insert { table, .. } | Stmt::Update { table, .. } | Stmt::Delete { table, .. } => table,
        }
    }

    /// The parameter markers it has.
    #[must_use]
    pub fn markers(&self) -> usize {
        let count = |e: &Expr| usize::from(matches!(e, Expr::Param(_)));
        match self {
            Stmt::Select { filter, .. } | Stmt::Delete { filter, .. } => filter.iter().map(|(_, e)| count(e)).sum(),
            Stmt::Insert { values, .. } => values.iter().map(count).sum(),
            Stmt::Update { sets, filter, .. } => sets.iter().chain(filter).map(|(_, e)| count(e)).sum(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Word(String),
    Num(String),
    Str(String),
    Sym(char),
}

/// Where the text stops making sense: the token found.
pub type Syntax = String;

fn tokens(text: &str) -> Result<Vec<Tok>, Syntax> {
    let c: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_alphabetic() || matches!(ch, '_' | '$' | '#' | '@') {
            let s = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || matches!(c[i], '_' | '$' | '#' | '@')) {
                i += 1;
            }
            out.push(Tok::Word(c[s..i].iter().collect::<String>().to_ascii_uppercase()));
        } else if ch.is_ascii_digit() || (ch == '-' && c.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let s = i;
            i += 1;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(c[s..i].iter().collect()));
        } else if ch == '\'' {
            let mut s = String::new();
            i += 1;
            loop {
                match c.get(i) {
                    None => return Err("'".into()),
                    Some('\'') if c.get(i + 1) == Some(&'\'') => {
                        s.push('\'');
                        i += 2;
                    }
                    Some('\'') => {
                        i += 1;
                        break;
                    }
                    Some(x) => {
                        s.push(*x);
                        i += 1;
                    }
                }
            }
            out.push(Tok::Str(s));
        } else if ch == '"' {
            let s = i + 1;
            let e = c[s..].iter().position(|x| *x == '"').ok_or_else(|| "\"".to_string())? + s;
            out.push(Tok::Word(c[s..e].iter().collect()));
            i = e + 1;
        } else if "(),*=?./;".contains(ch) {
            out.push(Tok::Sym(ch));
            i += 1;
        } else {
            return Err(ch.to_string());
        }
    }
    while out.last() == Some(&Tok::Sym(';')) {
        out.pop();
    }
    Ok(out)
}

struct P {
    t: Vec<Tok>,
    i: usize,
    markers: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }

    fn here(&self) -> Syntax {
        match self.peek() {
            None => "the end of the statement".into(),
            Some(Tok::Word(w) | Tok::Num(w)) => w.clone(),
            Some(Tok::Str(s)) => format!("'{s}'"),
            Some(Tok::Sym(c)) => c.to_string(),
        }
    }

    fn word(&mut self, w: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Word(x)) if x == w) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn sym(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Sym(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn need_word(&mut self, w: &str) -> Result<(), Syntax> {
        if self.word(w) { Ok(()) } else { Err(self.here()) }
    }

    fn need_sym(&mut self, c: char) -> Result<(), Syntax> {
        if self.sym(c) { Ok(()) } else { Err(self.here()) }
    }

    fn ident(&mut self) -> Result<String, Syntax> {
        match self.peek() {
            Some(Tok::Word(w)) => {
                let w = w.clone();
                self.i += 1;
                Ok(w)
            }
            _ => Err(self.here()),
        }
    }

    fn table(&mut self) -> Result<TableRef, Syntax> {
        let first = self.ident()?;
        for (c, slash) in [('.', false), ('/', true)] {
            if self.sym(c) {
                return Ok(TableRef {
                    schema: Some(first),
                    name: self.ident()?,
                    slash,
                });
            }
        }
        Ok(TableRef {
            schema: None,
            name: first,
            slash: false,
        })
    }

    fn expr(&mut self) -> Result<Expr, Syntax> {
        if self.sym('?') {
            self.markers += 1;
            return Ok(Expr::Param(self.markers - 1));
        }
        if self.word("NULL") {
            return Ok(Expr::Lit(Lit::Null));
        }
        match self.peek().cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(Expr::Lit(Lit::Num(n)))
            }
            Some(Tok::Str(s)) => {
                self.i += 1;
                Ok(Expr::Lit(Lit::Str(s)))
            }
            _ => Err(self.here()),
        }
    }

    fn filter(&mut self) -> Result<Filter, Syntax> {
        let mut f = Vec::new();
        if !self.word("WHERE") {
            return Ok(f);
        }
        loop {
            let c = self.ident()?;
            self.need_sym('=')?;
            f.push((c, self.expr()?));
            if !self.word("AND") {
                return Ok(f);
            }
        }
    }

    fn names(&mut self) -> Result<Vec<String>, Syntax> {
        let mut v = vec![self.ident()?];
        while self.sym(',') {
            v.push(self.ident()?);
        }
        Ok(v)
    }

    fn statement(&mut self) -> Result<Stmt, Syntax> {
        let s = if self.word("SELECT") {
            let cols = if self.sym('*') { None } else { Some(self.names()?) };
            self.need_word("FROM")?;
            let table = self.table()?;
            Stmt::Select {
                cols,
                table,
                filter: self.filter()?,
            }
        } else if self.word("INSERT") {
            self.need_word("INTO")?;
            let table = self.table()?;
            let cols = if self.sym('(') {
                let v = self.names()?;
                self.need_sym(')')?;
                Some(v)
            } else {
                None
            };
            self.need_word("VALUES")?;
            self.need_sym('(')?;
            let mut values = vec![self.expr()?];
            while self.sym(',') {
                values.push(self.expr()?);
            }
            self.need_sym(')')?;
            Stmt::Insert { table, cols, values }
        } else if self.word("UPDATE") {
            let table = self.table()?;
            self.need_word("SET")?;
            let mut sets = Vec::new();
            loop {
                let c = self.ident()?;
                self.need_sym('=')?;
                sets.push((c, self.expr()?));
                if !self.sym(',') {
                    break;
                }
            }
            Stmt::Update {
                table,
                sets,
                filter: self.filter()?,
            }
        } else if self.word("DELETE") {
            self.need_word("FROM")?;
            let table = self.table()?;
            Stmt::Delete {
                table,
                filter: self.filter()?,
            }
        } else {
            return Err(self.here());
        };
        if self.peek().is_some() {
            return Err(self.here());
        }
        Ok(s)
    }
}

/// Parse `text`; the token where it stops making sense when it does not.
///
/// # Errors
/// The token found where the statement went wrong.
pub fn parse(text: &str) -> Result<Stmt, Syntax> {
    let t = tokens(text)?;
    P { t, i: 0, markers: 0 }.statement()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements() {
        let s = parse("select * from hlinv.assets where asset_tag = ? and status = 'ACTIVE';").unwrap();
        assert_eq!(s.markers(), 1);
        assert_eq!(s.table().name, "ASSETS");
        let s = parse("INSERT INTO HLTEST/JRN (ID, NAME) VALUES (?, 'x')").unwrap();
        assert!(s.table().slash);
        assert_eq!(s.markers(), 1);
        assert_eq!(parse("UPDATE T SET A = ?, B = NULL WHERE C = -1.5").unwrap().markers(), 1);
        assert_eq!(parse("CREATE TABLE X (A INT)"), Err("CREATE".into()));
        assert_eq!(parse("SELECT * FROM"), Err("the end of the statement".into()));
    }
}
