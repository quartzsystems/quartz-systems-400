// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL parser as a library for language servers: a script in (as
//! `RUNSQLSTM` reads it), each statement's tree, the tables, columns,
//! routines, cursors and variables it creates or declares and every
//! use of a name with its span, and the parser's diagnostics in
//! [`compdiag`]'s form. A statement that does not parse is reported
//! and the next one is read. Names are found in the tokens: what a
//! script declares is linked to its uses; what the system holds
//! (a table created elsewhere) is a reference with no declaration.
//! Plain SQL includes nothing, so
//! [`Analysis::includes`](compdiag::front::Analysis::includes) stays
//! empty.

mod scan;

use std::sync::OnceLock;

use compdiag::front::{Lines, Span};
use compdiag::msgtext::Table;
use compdiag::{Diagnostic, Where, severity};

use crate::ast::{Naming, Statement};
use crate::error::SqlError;
use crate::lex::Token;
use crate::parse::Parser;

pub use compdiag::front::Source;

/// One statement of the script.
#[derive(Debug)]
pub struct Located {
    /// The statement, when it parsed.
    pub statement: Option<Statement>,
    /// Its text.
    pub span: Span,
}

/// The tree: the statements, in order.
#[derive(Debug, Default)]
pub struct Tree {
    /// The statements.
    pub statements: Vec<Located>,
}

/// What `analyze` produced.
pub type Analysis = compdiag::front::Analysis<Tree>;

/// How to analyze.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// `NAMING(*SYS)` or `*SQL`.
    pub naming: Naming,
}

/// Analyze an SQL script.
#[must_use]
pub fn analyze(source: &Source, options: &Options) -> Analysis {
    let lines = Lines::new(&source.text);
    let mut a = Analysis {
        sources: vec![source.clone()],
        ..Analysis::default()
    };
    let mut p = match Parser::new(&source.text, options.naming) {
        Ok(p) => p,
        Err(e) => {
            a.diagnostics.push(diagnostic(&e, &source.at, None));
            return a;
        }
    };
    let parsed = p.statements_each();
    let toks: Vec<Token> = p.tokens().to_vec();
    for s in parsed {
        let span = statement_span(&lines, &toks, s.first, s.last);
        let statement = match s.result {
            Ok(st) => Some(st),
            Err(e) => {
                a.diagnostics
                    .push(diagnostic(&e, &source.at, toks.get(s.first)));
                None
            }
        };
        scan::statement(
            &lines,
            &toks[s.first..=s.last],
            &mut a.declarations,
            &mut a.references,
        );
        a.ast.statements.push(Located { statement, span });
    }
    scan::uses(&lines, &toks, &a.declarations, &mut a.references);
    a.references.sort_by_key(|r| r.span.start);
    a.link(|_| None);
    a
}

/// From the first token to the end of the last.
fn statement_span(lines: &Lines, toks: &[Token], first: usize, last: usize) -> Span {
    let (Some(a), Some(b)) = (toks.get(first), toks.get(last)) else {
        return Span::default();
    };
    let start = scan::token_span(lines, a);
    let end = scan::token_span(lines, b);
    Span {
        end: end.end,
        end_line: end.end_line,
        end_column: end.end_column,
        ..start
    }
}

/// A parse error as the listing shows it: where the error is, else
/// where its statement starts.
fn diagnostic(e: &SqlError, at: &Where, start: Option<&Token>) -> Diagnostic {
    let (line, col) = match e {
        SqlError::Syntax { line, col, .. } | SqlError::Keyword { line, col, .. } => (*line, *col),
        _ => start.map_or((0, 0), |t| (t.line, t.col)),
    };
    let id = e.msgid();
    Diagnostic {
        at: at.clone(),
        line,
        column: u16::try_from(col).unwrap_or(u16::MAX),
        end_line: 0,
        end_column: 0,
        severity: severity::SEVERE,
        text: e.to_string(),
        help: texts().get(&id).map(|m| m.help.clone()).unwrap_or_default(),
        id,
    }
}

/// The SQL messages (`QSQLMSG`).
fn texts() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table::parse(include_str!("../../../../runtime/msgf/msgs/qsqlmsg.toml")))
}

#[cfg(test)]
mod tests;
