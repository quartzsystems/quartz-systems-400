// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser: recursive descent over the token stream. Statement
//! families live in their own modules; this one has the cursor, the
//! names and types, and the statement dispatch.

mod change;
mod ddl;
mod integrity;
mod sequence;
mod variable;
mod from;
mod dml;
mod expr;
mod objects;
mod cte;
mod olap;

pub use olap::GROUPING_SETS;
mod json;
mod pl;
mod query;
mod routine;
mod types;

use crate::ast::{
    DataType, DeclareCursor, Fetch, FetchOrientation, Ident, Isolation, Naming, ObjectName,
    PathEntry, SchemaTarget, Statement, TypeSpec,
};
use crate::error::{Result, SqlError};
use crate::lex::{Tok, Token, tokenize};

/// Words that cannot be a correlation name without `AS`.
pub(crate) const RESERVED: &[&str] = &[
    "ALL",
    "AND",
    "AS",
    "ASC",
    "BETWEEN",
    "BY",
    "CASE",
    "CROSS",
    "CURRENT",
    "DESC",
    "DISTINCT",
    "ELSE",
    "END",
    "EXCEPT",
    "EXCEPTION",
    "EXISTS",
    "FETCH",
    "FOR",
    "FROM",
    "FULL",
    "GROUP",
    "HAVING",
    "IN",
    "INNER",
    "INTERSECT",
    "INTO",
    "IS",
    "JOIN",
    "LEFT",
    "LIKE",
    "LIMIT",
    "NOT",
    "NULL",
    "OFFSET",
    "ON",
    "OPTIMIZE",
    "OR",
    "ORDER",
    "OUTER",
    "RIGHT",
    "SELECT",
    "SET",
    "THEN",
    "UNION",
    "USING",
    "VALUES",
    "WHEN",
    "WHERE",
    "WITH",
];

/// The clause words DB2 for i answers `SQL0199` for where they cannot be
/// (pub400); any other token out of place is `SQL0104`.
const KEYWORDS: &[&str] = &["FROM", "GROUP", "HAVING", "INTO", "NULLS", "WHERE"];

/// How deeply expressions and subqueries may nest.
pub const MAX_DEPTH: u16 = 64;

/// The parser state.
pub struct Parser {
    toks: Vec<Token>,
    at: usize,
    naming: Naming,
    params: usize,
    /// OLAP specifications so far, each its slot.
    windows: usize,
    /// A clause parsed only to be refused, reported once the statement parses.
    refused: Option<SqlError>,
    /// `SKIP LOCKED DATA` seen and not yet taken by a query.
    skip_locked: bool,
    /// Expressions and queries open around the current token.
    depth: u16,
    /// The statement text, for the parts kept as written (a view's query).
    src: String,
}

impl Parser {
    /// Over `text`.
    pub fn new(text: &str, naming: Naming) -> Result<Self> {
        Ok(Self {
            toks: tokenize(text)?,
            at: 0,
            naming,
            params: 0,
            windows: 0,
            refused: None,
            skip_locked: false,
            depth: 0,
            src: text.to_string(),
        })
    }

    fn tok(&self) -> &Tok {
        &self.toks[self.at.min(self.toks.len() - 1)].tok
    }

    fn tok_at(&self, ahead: usize) -> &Tok {
        &self.toks[(self.at + ahead).min(self.toks.len() - 1)].tok
    }

    fn advance(&mut self) -> Tok {
        let t = self.tok().clone();
        if self.at < self.toks.len() - 1 {
            self.at += 1;
        }
        t
    }

    fn at_end(&self) -> bool {
        matches!(self.tok(), Tok::End)
    }

    /// The error for the current token.
    fn expected(&self, what: &str) -> SqlError {
        let t = &self.toks[self.at.min(self.toks.len() - 1)];
        // A reserved word where it cannot be is SQL0199, as on DB2 for i.
        if let Tok::Word(w) = &t.tok
            && KEYWORDS.contains(&w.as_str())
        {
            return SqlError::Keyword { word: t.tok.describe(), line: t.line, col: t.col };
        }
        SqlError::Syntax {
            line: t.line,
            col: t.col,
            found: t.tok.describe(),
            expected: what.into(),
        }
    }

    /// Runs `f` one nesting level deeper; past [`MAX_DEPTH`] the statement
    /// is too complex (`SQL0101`), before the recursion can exhaust the stack.
    pub(crate) fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.depth >= MAX_DEPTH {
            return Err(SqlError::TooComplex);
        }
        self.depth += 1;
        let r = f(self);
        self.depth -= 1;
        r
    }

    /// `SQL0199`: the current keyword is not expected.
    fn keyword(&self) -> SqlError {
        let t = &self.toks[self.at.min(self.toks.len() - 1)];
        SqlError::Keyword { word: t.tok.describe(), line: t.line, col: t.col }
    }

    fn unsupported(what: &str) -> SqlError {
        SqlError::Unsupported { what: what.into() }
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.tok(), Tok::Word(x) if x == w)
    }

    fn is_word_at(&self, ahead: usize, w: &str) -> bool {
        matches!(self.tok_at(ahead), Tok::Word(x) if x == w)
    }

    fn is_words(&self, ws: &[&str]) -> bool {
        ws.iter().enumerate().all(|(i, w)| self.is_word_at(i, w))
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if self.is_word(w) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn eat_words(&mut self, ws: &[&str]) -> bool {
        if self.is_words(ws) {
            for _ in ws {
                self.advance();
            }
            true
        } else {
            false
        }
    }

    fn expect_word(&mut self, w: &str) -> Result<()> {
        if self.eat_word(w) {
            Ok(())
        } else {
            Err(self.expected(w))
        }
    }

    fn expect_words(&mut self, ws: &[&str]) -> Result<()> {
        for w in ws {
            self.expect_word(w)?;
        }
        Ok(())
    }

    fn is_sym(&self, s: &str) -> bool {
        matches!(self.tok(), Tok::Sym(x) if *x == s)
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, s: &str) -> Result<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(self.expected(s))
        }
    }

    /// An identifier, regular or delimited.
    fn ident(&mut self) -> Result<Ident> {
        match self.tok().clone() {
            Tok::Word(w) => {
                self.advance();
                Ok(Ident {
                    value: w,
                    delimited: false,
                })
            }
            Tok::Delimited(d) => {
                self.advance();
                Ok(Ident {
                    value: d,
                    delimited: true,
                })
            }
            _ => Err(self.expected("a name")),
        }
    }

    /// An identifier that is not a reserved word; `None` leaves the
    /// token alone.
    fn optional_name(&mut self) -> Result<Option<Ident>> {
        match self.tok() {
            // Not the concurrent access resolution clause.
            Tok::Word(w)
                if self.is_words(&["WAIT", "FOR"]) || self.is_words(&["SKIP", "LOCKED"]) || self.is_words(&["USE", "CURRENTLY"]) =>
            {
                let _ = w;
                Ok(None)
            }
            Tok::Word(w) if !RESERVED.contains(&w.as_str()) => Ok(Some(self.ident()?)),
            Tok::Delimited(_) => Ok(Some(self.ident()?)),
            _ => Ok(None),
        }
    }

    /// The separator of the naming mode.
    fn separator(&self) -> &'static str {
        match self.naming {
            Naming::Sys => "/",
            Naming::Sql => ".",
        }
    }

    /// A one- or two-part object name in the naming mode.
    fn object_name(&mut self) -> Result<ObjectName> {
        let first = self.ident()?;
        // System naming takes the period as well as the slash (V7R5M0,
        // `rpg/perf/PERF04.sqlrpgle`).
        let sep = if self.naming == Naming::Sys && self.is_sym(".") { "." } else { self.separator() };
        if !self.eat_sym(sep) {
            return Ok(ObjectName {
                schema: None,
                name: first,
            });
        }
        let second = self.ident()?;
        if self.is_sym(sep) {
            return Err(SqlError::ThreePartName {
                name: format!("{first}{sep}{second}{sep}..."),
            });
        }
        Ok(ObjectName {
            schema: Some(first),
            name: second,
        })
    }

    /// A list of identifiers in parentheses.
    fn ident_list(&mut self) -> Result<Vec<Ident>> {
        self.expect_sym("(")?;
        let mut out = vec![self.ident()?];
        while self.eat_sym(",") {
            out.push(self.ident()?);
        }
        self.expect_sym(")")?;
        Ok(out)
    }

    /// An unsigned integer literal.
    fn integer(&mut self) -> Result<u64> {
        match self.tok().clone() {
            Tok::Num(n) => {
                let v = n.parse().map_err(|_| self.expected("an integer"))?;
                self.advance();
                Ok(v)
            }
            _ => Err(self.expected("an integer")),
        }
    }

    /// A string literal.
    fn string(&mut self) -> Result<String> {
        match self.tok().clone() {
            Tok::Str(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.expected("a string")),
        }
    }

    /// `(n)` or `(p, s)`; `None` when absent.
    fn length_args(&mut self) -> Result<Option<(u32, Option<u32>)>> {
        if !self.eat_sym("(") {
            return Ok(None);
        }
        let n = u32::try_from(self.integer()?).map_err(|_| self.expected("a length"))?;
        let s = if self.eat_sym(",") {
            Some(u32::try_from(self.integer()?).map_err(|_| self.expected("a scale"))?)
        } else {
            None
        };
        self.expect_sym(")")?;
        Ok(Some((n, s)))
    }

    /// A LOB's `(n[K|M|G])`, 1M when absent.
    fn lob_length(&mut self) -> Result<u32> {
        if !self.eat_sym("(") {
            return Ok(1 << 20);
        }
        let n = self.integer()?;
        let unit = match self.tok().clone() {
            Tok::Word(w) if matches!(w.as_str(), "K" | "M" | "G") => {
                self.advance();
                match w.as_str() {
                    "K" => 1 << 10,
                    "M" => 1 << 20,
                    _ => 1 << 30,
                }
            }
            _ => 1,
        };
        self.expect_sym(")")?;
        u32::try_from(n * unit)
            .ok()
            .filter(|n| (1..=2_147_483_647).contains(n))
            .ok_or_else(|| self.expected("a LOB length of 1 to 2G"))
    }

    fn precision(&self, p: u32, s: Option<u32>) -> Result<(u8, u8)> {
        let s = s.unwrap_or(0);
        if p == 0 || p > 63 || s > p {
            return Err(self.expected("a precision of 1 to 63 and a scale within it"));
        }
        Ok((
            u8::try_from(p).expect("small"),
            u8::try_from(s).expect("small"),
        ))
    }

    /// A data type with `CCSID` and `FOR BIT DATA`.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn type_spec(&mut self) -> Result<TypeSpec> {
        let Tok::Word(w) = self.tok().clone() else {
            return Err(self.expected("a data type"));
        };
        self.advance();
        let data_type = match w.as_str() {
            "CHAR" | "CHARACTER" => {
                if self.eat_word("VARYING") {
                    let (n, _) = self
                        .length_args()?
                        .ok_or_else(|| self.expected("a length"))?;
                    DataType::Varchar(n)
                } else if self.eat_words(&["LARGE", "OBJECT"]) {
                    DataType::Clob(self.lob_length()?)
                } else {
                    DataType::Char(self.length_args()?.map_or(1, |(n, _)| n))
                }
            }
            "VARCHAR" => {
                let (n, _) = self
                    .length_args()?
                    .ok_or_else(|| self.expected("a length"))?;
                DataType::Varchar(n)
            }
            "GRAPHIC" => DataType::Graphic(self.length_args()?.map_or(1, |(n, _)| n)),
            "VARGRAPHIC" => {
                let (n, _) = self
                    .length_args()?
                    .ok_or_else(|| self.expected("a length"))?;
                DataType::Vargraphic(n)
            }
            "DECIMAL" | "DEC" => {
                let (p, s) = self.length_args()?.unwrap_or((5, Some(0)));
                let (p, s) = self.precision(p, s)?;
                DataType::Decimal(p, s)
            }
            "NUMERIC" | "NUM" => {
                let (p, s) = self.length_args()?.unwrap_or((5, Some(0)));
                let (p, s) = self.precision(p, s)?;
                DataType::Numeric(p, s)
            }
            "SMALLINT" => DataType::SmallInt,
            "INTEGER" | "INT" => DataType::Integer,
            "BIGINT" => DataType::BigInt,
            "REAL" => DataType::Real,
            "DOUBLE" => {
                self.eat_word("PRECISION");
                DataType::Double
            }
            "FLOAT" => match self.length_args()? {
                Some((n, _)) if n <= 24 => DataType::Real,
                _ => DataType::Double,
            },
            "DATE" => DataType::Date,
            "TIME" => DataType::Time,
            "TIMESTAMP" => match self.length_args()? {
                None => DataType::Timestamp(6),
                Some((p, None)) if p <= 12 => DataType::Timestamp(u8::try_from(p).expect("small")),
                _ => return Err(self.expected("a timestamp precision of 0 to 12")),
            },
            "DECFLOAT" => match self.length_args()? {
                None | Some((34, None)) => DataType::DecFloat(34),
                Some((16, None)) => DataType::DecFloat(16),
                _ => return Err(self.expected("DECFLOAT(16) or DECFLOAT(34)")),
            },
            "BINARY" => {
                if self.eat_word("VARYING") {
                    let (n, _) = self.length_args()?.ok_or_else(|| self.expected("a length"))?;
                    DataType::Varbinary(n)
                } else if self.eat_words(&["LARGE", "OBJECT"]) {
                    DataType::Blob(self.lob_length()?)
                } else {
                    DataType::Binary(self.length_args()?.map_or(1, |(n, _)| n))
                }
            }
            "VARBINARY" => {
                let (n, _) = self.length_args()?.ok_or_else(|| self.expected("a length"))?;
                DataType::Varbinary(n)
            }
            "BOOLEAN" => DataType::Boolean,
            "BLOB" => DataType::Blob(self.lob_length()?),
            "CLOB" => DataType::Clob(self.lob_length()?),
            "DBCLOB" => DataType::Dbclob(self.lob_length()?),
            "NCHAR" | "NVARCHAR" | "NCLOB" | "NATIONAL" => return self.national_type(&w),
            // BLOCKED(sql): an XML column needs a stored field type (DB_FORMAT
            // has none); ROWID needs the generated values of SQL-2's generated
            // columns; DATALINK needs file links the IFS does not keep.
            "XML" => return Err(Self::unsupported("the XML data type for a column or a CAST (XMLPARSE and XMLSERIALIZE take XML text)")),
            "ROWID" => return Err(Self::unsupported("ROWID columns (generated row identifiers)")),
            "DATALINK" => return Err(Self::unsupported("the DATALINK data type")),
            // BLOCKED(sql): a distinct type in a column, variable or CAST
            // needs strong typing through the engine; CREATE TYPE makes
            // the type only.
            _ if self.is_sym(".") => return Err(Self::unsupported("a user-defined type as a data type")),
            _ => return Err(self.expected("a data type")),
        };
        let mut spec = TypeSpec {
            data_type,
            ccsid: None,
            bit_data: false,
        };
        loop {
            if self.eat_word("CCSID") {
                spec.ccsid =
                    Some(u16::try_from(self.integer()?).map_err(|_| self.expected("a CCSID"))?);
            } else if self.eat_words(&["FOR", "BIT", "DATA"]) {
                spec.bit_data = true;
            } else if spec.data_type.is_lob_type() && self.eat_words(&["INLINE", "LENGTH"]) {
                // A storage hint: QS/400 keeps a LOB of up to 24 bytes in its
                // LobRef whatever the length asks (docs/DB_FORMAT.md); results
                // are the same either way.
                self.integer()?;
            } else if self.eat_words(&["FOR", "MIXED", "DATA"]) {
                // The job's CCSID 37 has no mixed form (the primary: SQL0604).
                return Err(crate::exec::cast::no_mixed());
            } else if self.eat_words(&["FOR", "SBCS", "DATA"]) {
            } else {
                return Ok(spec);
            }
        }
    }

    /// `WITH NC|UR|CS|RS|RR`, when present.
    fn isolation(&mut self) -> Option<Isolation> {
        let iso = self.isolation_clause();
        // The concurrent access resolution clause.
        if self.eat_words(&["SKIP", "LOCKED", "DATA"]) {
            self.skip_locked = true;
        } else if self.is_words(&["WAIT", "FOR", "OUTCOME"]) {
            self.refuse(Self::unsupported("WAIT FOR OUTCOME"));
            self.advance();
            self.advance();
            self.advance();
        } else if self.is_words(&["USE", "CURRENTLY", "COMMITTED"]) {
            self.refuse(Self::unsupported("USE CURRENTLY COMMITTED"));
            self.advance();
            self.advance();
            self.advance();
        }
        iso
    }

    /// Keep the first refusal found while parsing goes on.
    fn refuse(&mut self, e: SqlError) {
        self.refused.get_or_insert(e);
    }

    fn isolation_clause(&mut self) -> Option<Isolation> {
        let iso = match self.tok_at(1) {
            Tok::Word(w) if self.is_word("WITH") => match w.as_str() {
                "NC" | "NONE" => Isolation::Nc,
                "UR" | "CHG" => Isolation::Ur,
                "CS" => Isolation::Cs,
                "RS" | "ALL" => Isolation::Rs,
                "RR" => Isolation::Rr,
                _ => return None,
            },
            _ => return None,
        };
        self.advance();
        self.advance();
        // USE AND KEEP x LOCKS: the engine takes no such locks.
        if self.eat_words(&["USE", "AND", "KEEP"]) {
            self.refuse(Self::unsupported("USE AND KEEP ... LOCKS"));
            self.advance();
            self.eat_word("LOCKS");
        }
        Some(iso)
    }

    /// One statement; the text must end after it (a `;` is allowed).
    pub fn statement(&mut self) -> Result<Statement> {
        let s = self.checked_body()?;
        self.eat_sym(";");
        if !self.at_end() {
            return Err(self.expected("end of statement"));
        }
        Ok(s)
    }

    /// Statements separated by `;` until the text ends.
    pub fn statements(&mut self) -> Result<Vec<Statement>> {
        let mut out = Vec::new();
        while !self.at_end() {
            if self.eat_sym(";") {
                continue;
            }
            out.push(self.checked_body()?);
            if !self.at_end() {
                self.expect_sym(";")?;
            }
        }
        Ok(out)
    }

    /// A statement, then any clause it holds that is refused.
    fn checked_body(&mut self) -> Result<Statement> {
        let s = self.statement_body()?;
        if let Some(e) = self.refused.take() {
            return Err(e);
        }
        if std::mem::take(&mut self.skip_locked) {
            return Err(Self::unsupported("SKIP LOCKED DATA on a statement that is not a query"));
        }
        Ok(s)
    }

    fn statement_body(&mut self) -> Result<Statement> {
        let Tok::Word(w) = self.tok().clone() else {
            if self.is_sym("(") {
                return Ok(Statement::Select(self.query()?));
            }
            return Err(self.expected("a statement"));
        };
        match w.as_str() {
            "SELECT" | "VALUES" | "WITH" => Ok(Statement::Select(self.query()?)),
            "INSERT" => self.insert(),
            "UPDATE" => self.update(),
            "DELETE" => self.delete(),
            "MERGE" => self.merge(),
            "CREATE" => self.create(),
            "ALTER" => self.alter(),
            "DROP" => self.drop(),
            "LABEL" => self.label(false),
            "COMMENT" => self.label(true),
            "RENAME" => self.rename(),
            "DECLARE" => self.declare(),
            "OPEN" => self.open(),
            "FETCH" => self.fetch(),
            "CLOSE" => {
                self.advance();
                Ok(Statement::Close(self.ident()?))
            }
            "COMMIT" => {
                self.advance();
                self.eat_word("WORK");
                Ok(Statement::Commit {
                    hold: self.eat_word("HOLD"),
                })
            }
            "ROLLBACK" => {
                self.advance();
                self.eat_word("WORK");
                Ok(Statement::Rollback {
                    hold: self.eat_word("HOLD"),
                })
            }
            "SET" => self.set(),
            "TRUNCATE" => self.truncate(),
            "EXPLAIN" => {
                self.advance();
                Ok(Statement::Explain(Box::new(self.statement_body()?)))
            }
            "GRANT" => self.grant(false),
            "REVOKE" => self.grant(true),
            "CALL" => self.call_stmt(),
            "SIGNAL" | "RESIGNAL" => Ok(Statement::Signal(self.signal()?)),
            "LOCK" | "PREPARE" | "EXECUTE" | "DESCRIBE"
            | "CONNECT" | "DISCONNECT" | "RELEASE" | "SAVEPOINT" | "REFRESH" => {
                Err(Self::unsupported(&format!("{w} statement")))
            }
            _ => Err(self.expected("a statement")),
        }
    }

    fn declare(&mut self) -> Result<Statement> {
        self.expect_word("DECLARE")?;
        if self.is_words(&["GLOBAL", "TEMPORARY"]) {
            // BLOCKED(sql): QTEMP is in the temporary ASP, and a statement
            // runs in one ASP's transaction.
            return Err(Self::unsupported("DECLARE GLOBAL TEMPORARY TABLE: tables in QTEMP"));
        }
        let name = self.ident()?;
        if self.is_word("SENSITIVE") {
            // BLOCKED(sql): rows are read when the cursor opens.
            return Err(SqlError::Coded {
                code: -243,
                state: "36001",
                what: "a SENSITIVE cursor: its rows are read when it opens".into(),
            });
        }
        let _ = self.eat_word("INSENSITIVE") || self.eat_word("ASENSITIVE");
        self.eat_word("DYNAMIC");
        let scroll = self.eat_word("SCROLL");
        self.eat_words(&["NO", "SCROLL"]);
        self.expect_word("CURSOR")?;
        let mut hold = false;
        loop {
            if self.eat_words(&["WITH", "HOLD"]) {
                hold = true;
            } else if self.eat_words(&["WITHOUT", "HOLD"])
                || self.eat_words(&["WITH", "RETURN"])
                || self.eat_words(&["WITHOUT", "RETURN"])
            {
                // Accepted; RETURN means nothing without stored procedures.
                while self.is_word("TO") || self.is_word("CLIENT") || self.is_word("CALLER") {
                    self.advance();
                }
            } else {
                break;
            }
        }
        self.expect_word("FOR")?;
        if matches!(self.tok(), Tok::Word(w) if !matches!(w.as_str(), "SELECT" | "VALUES" | "WITH"))
            && !self.is_sym("(")
        {
            return Err(Self::unsupported(
                "DECLARE CURSOR FOR a prepared statement name",
            ));
        }
        let query = self.query()?;
        Ok(Statement::DeclareCursor(DeclareCursor {
            name,
            scroll,
            hold,
            query,
        }))
    }

    fn open(&mut self) -> Result<Statement> {
        self.expect_word("OPEN")?;
        let cursor = self.ident()?;
        let mut using = Vec::new();
        if self.eat_word("USING") {
            if self.eat_word("DESCRIPTOR") {
                return Err(Self::unsupported("OPEN USING DESCRIPTOR"));
            }
            using.push(self.expr()?);
            while self.eat_sym(",") {
                using.push(self.expr()?);
            }
        }
        Ok(Statement::Open { cursor, using })
    }

    fn fetch(&mut self) -> Result<Statement> {
        self.expect_word("FETCH")?;
        let orientation = if self.eat_word("NEXT") {
            FetchOrientation::Next
        } else if self.eat_word("PRIOR") || self.eat_word("PREVIOUS") {
            FetchOrientation::Prior
        } else if self.eat_word("FIRST") {
            FetchOrientation::First
        } else if self.eat_word("LAST") {
            FetchOrientation::Last
        } else if self.eat_word("BEFORE") {
            FetchOrientation::Before
        } else if self.eat_word("AFTER") {
            FetchOrientation::After
        } else if self.eat_word("CURRENT") {
            FetchOrientation::Current
        } else if self.eat_word("RELATIVE") {
            FetchOrientation::Relative(self.expr()?)
        } else if self.eat_word("ABSOLUTE") {
            FetchOrientation::Absolute(self.expr()?)
        } else {
            FetchOrientation::Next
        };
        self.eat_word("FROM");
        let cursor = self.ident()?;
        let mut into = Vec::new();
        if self.eat_word("INTO") {
            if self.eat_word("DESCRIPTOR") {
                return Err(Self::unsupported("FETCH INTO DESCRIPTOR"));
            }
            into.push(self.expr()?);
            while self.eat_sym(",") {
                into.push(self.expr()?);
            }
        }
        Ok(Statement::Fetch(Fetch {
            orientation,
            cursor,
            into,
        }))
    }

    fn set(&mut self) -> Result<Statement> {
        self.expect_word("SET")?;
        if self.is_words(&["RESULT", "SETS"]) {
            return self.set_result_sets();
        }
        if self.eat_word("SCHEMA")
            || self.eat_words(&["CURRENT", "SCHEMA"])
            || self.eat_words(&["CURRENT_SCHEMA"])
        {
            self.eat_sym("=");
            let target = if self.eat_word("USER")
                || self.eat_word("SESSION_USER")
                || self.eat_word("CURRENT_USER")
            {
                SchemaTarget::User
            } else if self.eat_word("DEFAULT") {
                SchemaTarget::Default
            } else if let Tok::Str(s) = self.tok().clone() {
                self.advance();
                SchemaTarget::Name(Ident {
                    value: s,
                    delimited: true,
                })
            } else if let Tok::Host { name, .. } = self.tok().clone() {
                self.advance();
                SchemaTarget::Host(name)
            } else {
                SchemaTarget::Name(self.ident()?)
            };
            return Ok(Statement::SetSchema(target));
        }
        if self.eat_word("PATH")
            || self.eat_words(&["CURRENT", "PATH"])
            || self.eat_word("CURRENT_PATH")
        {
            self.eat_sym("=");
            let mut entries = vec![self.path_entry()?];
            while self.eat_sym(",") {
                entries.push(self.path_entry()?);
            }
            return Ok(Statement::SetPath(entries));
        }
        for (words, name) in [
            (&["CURRENT", "DEGREE"][..], "CURRENT DEGREE"),
            (&["CURRENT_DEGREE"][..], "CURRENT DEGREE"),
            (&["CURRENT", "IMPLICIT", "XMLPARSE", "OPTION"][..], "CURRENT IMPLICIT XMLPARSE OPTION"),
        ] {
            if self.eat_words(words) {
                self.eat_sym("=");
                // A keyword (ANY, ROUND_HALF_UP, ...) stands for its text.
                let value = match self.tok().clone() {
                    Tok::Word(w) if !self.is_sym_at(1, "(") => {
                        self.advance();
                        crate::ast::Expr::Literal(crate::ast::Literal::Str(w))
                    }
                    _ => self.expr()?,
                };
                return Ok(Statement::SetRegister { name, value });
            }
        }
        if self.eat_words(&["CURRENT", "DECFLOAT", "ROUNDING", "MODE"]) {
            return self.set_decfloat_rounding();
        }
        if self.eat_words(&["TRANSACTION", "ISOLATION", "LEVEL"]) {
            return self.set_transaction();
        }
        if self.eat_words(&["TRANSACTION", "ISOLATION"]) || self.eat_word("OPTION") {
            return Err(Self::unsupported("SET TRANSACTION / SET OPTION"));
        }
        if let Some(st) = self.set_variables()? {
            return Ok(st);
        }
        Err(Self::unsupported("SET of that register or variable"))
    }

    /// The level of `SET TRANSACTION ISOLATION LEVEL`, in words or
    /// as the short form.
    fn set_transaction(&mut self) -> Result<Statement> {
        let iso = if self.eat_words(&["NO", "COMMIT"]) {
            Isolation::Nc
        } else if self.eat_words(&["READ", "UNCOMMITTED"]) {
            Isolation::Ur
        } else if self.eat_words(&["READ", "COMMITTED"]) {
            Isolation::Cs
        } else if self.eat_words(&["REPEATABLE", "READ"]) {
            Isolation::Rs
        } else if self.eat_word("SERIALIZABLE") {
            Isolation::Rr
        } else {
            let iso = match self.tok() {
                Tok::Word(w) => match w.as_str() {
                    "NC" | "NONE" => Isolation::Nc,
                    "UR" | "CHG" => Isolation::Ur,
                    "CS" => Isolation::Cs,
                    "RS" | "ALL" => Isolation::Rs,
                    "RR" => Isolation::Rr,
                    _ => return Err(self.expected("an isolation level")),
                },
                _ => return Err(self.expected("an isolation level")),
            };
            self.advance();
            iso
        };
        Ok(Statement::SetTransaction(iso))
    }

    fn path_entry(&mut self) -> Result<PathEntry> {
        if self.eat_words(&["SYSTEM", "PATH"]) {
            return Ok(PathEntry::SystemPath);
        }
        if self.eat_words(&["CURRENT", "PATH"]) || self.eat_word("CURRENT_PATH") {
            return Ok(PathEntry::CurrentPath);
        }
        if self.eat_word("USER") || self.eat_word("SESSION_USER") || self.eat_word("CURRENT_USER") {
            return Ok(PathEntry::User);
        }
        match self.tok().clone() {
            Tok::Str(s) => {
                self.advance();
                if s == "*LIBL" {
                    return Ok(PathEntry::Libl);
                }
                Ok(PathEntry::Name(Ident {
                    value: s,
                    delimited: true,
                }))
            }
            Tok::Sym("*") if self.is_word_at(1, "LIBL") => {
                self.advance();
                self.advance();
                Ok(PathEntry::Libl)
            }
            _ => Ok(PathEntry::Name(self.ident()?)),
        }
    }
}

/// Parse one statement.
pub fn parse(text: &str, naming: Naming) -> Result<Statement> {
    Parser::new(text, naming)?.statement()
}

/// Parse one expression, as a column default is kept.
pub fn parse_expr(text: &str, naming: Naming) -> Result<crate::ast::Expr> {
    Parser::new(text, naming)?.expr()
}

/// Parse a `;`-separated script.
pub fn parse_all(text: &str, naming: Naming) -> Result<Vec<Statement>> {
    Parser::new(text, naming)?.statements()
}

/// Parse a table name given as text (`DESCRIBE TABLE :hv`).
pub fn parse_object_name(text: &str, naming: Naming) -> Result<ObjectName> {
    let mut p = Parser::new(text, naming)?;
    let n = p.object_name()?;
    if !p.at_end() {
        return Err(p.expected("the end of the name"));
    }
    Ok(n)
}
