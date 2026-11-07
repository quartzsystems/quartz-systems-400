// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names in a statement's tokens: what `CREATE` and `DECLARE` declare,
//! the tables after `FROM`, `JOIN`, `INTO`, `UPDATE`, `TABLE`, `ON`
//! and `REFERENCES`, the routines `CALL` names, and then every other
//! word that is a name the script declared.

use std::collections::HashSet;

use compdiag::front::{Declaration, Lines, Reference, Span, SymbolKind};

use crate::lex::{Tok, Token};

/// The span of a token as written.
pub(super) fn token_span(lines: &Lines, t: &Token) -> Span {
    let text = lines.text(t.line);
    let rest: Vec<char> = text
        .chars()
        .skip(usize::try_from(t.col.saturating_sub(1)).unwrap_or(0))
        .collect();
    let ident = |c: char| c.is_alphanumeric() || "_#@$".contains(c);
    let n = match &t.tok {
        Tok::Word(_) => rest.iter().take_while(|c| ident(**c)).count(),
        Tok::Delimited(_) => {
            let mut i = 1;
            while i < rest.len() {
                if rest[i] == '"' {
                    if rest.get(i + 1) == Some(&'"') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            (i + 1).min(rest.len())
        }
        Tok::Host { .. } => {
            1 + rest
                .iter()
                .skip(1)
                .take_while(|c| ident(**c) || **c == ':')
                .count()
        }
        Tok::Sym(s) => s.chars().count(),
        other => other.describe().chars().count().min(rest.len()),
    };
    lines.span(t.line, t.col, u32::try_from(n).unwrap_or(0))
}

/// A name token: the name (upper case unless delimited).
fn ident(t: Option<&Token>) -> Option<String> {
    match t.map(|t| &t.tok) {
        Some(Tok::Word(w) | Tok::Delimited(w)) => Some(w.clone()),
        _ => None,
    }
}

fn word(t: Option<&Token>) -> Option<&str> {
    match t.map(|t| &t.tok) {
        Some(Tok::Word(w)) => Some(w.as_str()),
        _ => None,
    }
}

fn sym(t: Option<&Token>, s: &str) -> bool {
    matches!(t.map(|t| &t.tok), Some(Tok::Sym(x)) if *x == s)
}

/// What a `CREATE` makes.
fn created(w: &str) -> Option<SymbolKind> {
    Some(match w {
        "TABLE" | "VIEW" | "ALIAS" => SymbolKind::Table,
        "INDEX" => SymbolKind::Index,
        "PROCEDURE" | "FUNCTION" | "TRIGGER" | "SEQUENCE" | "TYPE" | "MASK" | "PERMISSION" => {
            SymbolKind::Routine
        }
        "VARIABLE" => SymbolKind::Variable,
        "SCHEMA" | "COLLECTION" => SymbolKind::Schema,
        _ => return None,
    })
}

/// Words a table name never is after `FROM` or `JOIN`.
const NOT_TABLES: &[&str] = &[
    "SELECT",
    "VALUES",
    "TABLE",
    "LATERAL",
    "FINAL",
    "NEW",
    "OLD",
    "UNNEST",
    "JSON_TABLE",
    "XMLTABLE",
    "WITH",
];

/// Words that end a table reference instead of naming a correlation.
const CLAUSES: &[&str] = &[
    "WHERE",
    "GROUP",
    "HAVING",
    "ORDER",
    "FETCH",
    "OFFSET",
    "LIMIT",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "CROSS",
    "EXCEPTION",
    "ON",
    "USING",
    "SET",
    "FOR",
    "WITH",
    "OPTIMIZE",
    "VALUES",
    "SELECT",
    "OVERRIDING",
    "SKIP",
    "WAIT",
];

struct Scan<'a> {
    lines: &'a Lines,
    t: &'a [Token],
    decls: &'a mut Vec<Declaration>,
    refs: &'a mut Vec<Reference>,
}

impl Scan<'_> {
    fn decl(&mut self, i: usize, kind: SymbolKind, detail: &str, scope: Option<String>) {
        let Some(name) = ident(self.t.get(i)) else {
            return;
        };
        self.decls.push(Declaration {
            name,
            kind,
            source: 0,
            span: token_span(self.lines, &self.t[i]),
            detail: detail.into(),
            scope,
        });
    }

    fn reference(&mut self, i: usize, kind: SymbolKind) {
        let Some(name) = ident(self.t.get(i)) else {
            return;
        };
        self.refs.push(Reference {
            name,
            kind,
            source: 0,
            span: token_span(self.lines, &self.t[i]),
            declaration: None,
        });
    }

    /// A one- or two-part name at `i`: the name's token and the index
    /// after it.
    fn qname(&self, i: usize) -> Option<(usize, usize)> {
        ident(self.t.get(i))?;
        if (sym(self.t.get(i + 1), ".") || sym(self.t.get(i + 1), "/"))
            && ident(self.t.get(i + 2)).is_some()
        {
            Some((i + 2, i + 3))
        } else {
            Some((i, i + 1))
        }
    }

    /// The parenthesized list at `open`: the index of each element's
    /// first token at depth one, and the index after `)`.
    fn elements(&self, open: usize) -> (Vec<usize>, usize) {
        let mut out = Vec::new();
        let mut depth = 0;
        let mut i = open;
        let mut start = true;
        while i < self.t.len() {
            if sym(self.t.get(i), "(") {
                depth += 1;
                if depth == 1 {
                    start = true;
                    i += 1;
                    continue;
                }
            } else if sym(self.t.get(i), ")") {
                depth -= 1;
                if depth == 0 {
                    return (out, i + 1);
                }
            } else if depth == 1 && sym(self.t.get(i), ",") {
                start = true;
                i += 1;
                continue;
            }
            if start && depth == 1 {
                out.push(i);
                start = false;
            }
            i += 1;
        }
        (out, i)
    }

    fn create(&mut self, i: usize) -> usize {
        let mut j = i + 1;
        let mut kind = None;
        for _ in 0..6 {
            match word(self.t.get(j)) {
                Some(w) => {
                    j += 1;
                    if let Some(k) = created(w) {
                        kind = Some((k, w.to_string()));
                        break;
                    }
                }
                None => return j,
            }
        }
        let Some((kind, what)) = kind else {
            return j;
        };
        let Some((name, next)) = self.qname(j) else {
            return j;
        };
        let scope = ident(self.t.get(name));
        self.decl(name, kind, &what, None);
        let mut j = next;
        match what.as_str() {
            "TABLE" | "VIEW" if sym(self.t.get(j), "(") => {
                let (els, after) = self.elements(j);
                for e in els {
                    let w = word(self.t.get(e));
                    if matches!(
                        w,
                        Some(
                            "CONSTRAINT"
                                | "PRIMARY"
                                | "UNIQUE"
                                | "FOREIGN"
                                | "CHECK"
                                | "PERIOD"
                                | "LIKE"
                        )
                    ) {
                        continue;
                    }
                    self.decl(e, SymbolKind::Field, "column", scope.clone());
                }
                j = after;
            }
            "PROCEDURE" | "FUNCTION" if sym(self.t.get(j), "(") => {
                let (els, after) = self.elements(j);
                for e in els {
                    let e = if matches!(word(self.t.get(e)), Some("IN" | "OUT" | "INOUT")) {
                        e + 1
                    } else {
                        e
                    };
                    self.decl(e, SymbolKind::Parameter, "parameter", scope.clone());
                }
                j = after;
            }
            "INDEX" | "TRIGGER" => {
                while j < self.t.len() && word(self.t.get(j)) != Some("ON") {
                    j += 1;
                }
                if let Some((t, next)) = self.qname(j + 1) {
                    self.reference(t, SymbolKind::Table);
                    j = next;
                    if what == "INDEX" && sym(self.t.get(j), "(") {
                        let (els, after) = self.elements(j);
                        for e in els {
                            self.reference(e, SymbolKind::Field);
                        }
                        j = after;
                    }
                }
            }
            _ => {}
        }
        j
    }

    fn declare(&mut self, i: usize) -> usize {
        if word(self.t.get(i + 1)) == Some("GLOBAL") {
            let mut j = i + 2;
            while j < self.t.len() && word(self.t.get(j)) != Some("TABLE") {
                j += 1;
            }
            if let Some((name, next)) = self.qname(j + 1) {
                self.decl(name, SymbolKind::Table, "temporary table", None);
                return next;
            }
            return j;
        }
        if matches!(word(self.t.get(i + 1)), Some("CONTINUE" | "EXIT" | "UNDO")) {
            return i + 1;
        }
        // `DECLARE c [attributes] CURSOR`, `DECLARE c CONDITION`, or
        // variables `DECLARE a, b type`.
        let cursor = (i + 2..(i + 8).min(self.t.len()))
            .find(|&k| word(self.t.get(k)) == Some("CURSOR"))
            .is_some_and(|k| (i + 2..k).all(|m| word(self.t.get(m)).is_some()));
        if cursor {
            self.decl(i + 1, SymbolKind::Cursor, "cursor", None);
            return i + 2;
        }
        if word(self.t.get(i + 2)) == Some("CONDITION") {
            return i + 2;
        }
        let mut j = i + 1;
        while ident(self.t.get(j)).is_some() {
            self.decl(j, SymbolKind::Variable, "variable", None);
            if !sym(self.t.get(j + 1), ",") {
                break;
            }
            j += 2;
        }
        j + 1
    }

    /// Table names from `i`, `FROM a, b` lists included.
    fn tables(&mut self, mut i: usize, list: bool) -> usize {
        loop {
            if word(self.t.get(i)).is_some_and(|w| NOT_TABLES.contains(&w)) {
                return i;
            }
            let Some((name, next)) = self.qname(i) else {
                return i;
            };
            if sym(self.t.get(next), "(") {
                // A table function.
                self.reference(name, SymbolKind::Routine);
                return next;
            }
            self.reference(name, SymbolKind::Table);
            i = next;
            if word(self.t.get(i)) == Some("AS") {
                i += 1;
            }
            if word(self.t.get(i)).is_none_or(|w| !CLAUSES.contains(&w))
                && ident(self.t.get(i)).is_some()
            {
                i += 1;
            }
            if list && sym(self.t.get(i), ",") {
                i += 1;
                continue;
            }
            return i;
        }
    }

    fn run(&mut self) {
        // Parentheses open around the token, each `true` when it holds
        // a query (so `FROM` in `EXTRACT(YEAR FROM x)` names no table).
        let mut parens: Vec<bool> = Vec::new();
        let mut i = 0;
        while i < self.t.len() {
            if sym(self.t.get(i), "(") {
                parens.push(matches!(
                    word(self.t.get(i + 1)),
                    Some("SELECT" | "WITH" | "VALUES")
                ));
                i += 1;
                continue;
            }
            if sym(self.t.get(i), ")") {
                parens.pop();
                i += 1;
                continue;
            }
            let query = parens.last().is_none_or(|q| *q);
            let prev = if i > 0 { word(self.t.get(i - 1)) } else { None };
            i = match word(self.t.get(i)) {
                Some("CREATE") => self.create(i),
                Some("DECLARE") => self.declare(i),
                Some("FROM") if query => self.tables(i + 1, true),
                Some("INTO") if matches!(prev, Some("INSERT" | "MERGE")) => {
                    self.tables(i + 1, false)
                }
                Some("UPDATE") if prev != Some("FOR") && !matches!(prev, Some("ON" | "OF")) => {
                    self.tables(i + 1, false)
                }
                Some("TABLE")
                    if matches!(prev, Some("LOCK" | "ALTER" | "DROP" | "TRUNCATE" | "ON")) =>
                {
                    self.tables(i + 1, false)
                }
                Some("VIEW" | "INDEX" | "ALIAS") if matches!(prev, Some("DROP" | "ALTER")) => {
                    self.tables(i + 1, false)
                }
                Some("JOIN" | "REFERENCES") => self.tables(i + 1, false),
                Some("CALL") => {
                    if let Some((name, next)) = self.qname(i + 1) {
                        self.reference(name, SymbolKind::Routine);
                        next
                    } else {
                        i + 1
                    }
                }
                _ => i + 1,
            };
        }
    }
}

/// The declarations and table, routine and cursor uses of one
/// statement's tokens.
pub(super) fn statement(
    lines: &Lines,
    toks: &[Token],
    decls: &mut Vec<Declaration>,
    refs: &mut Vec<Reference>,
) {
    Scan {
        lines,
        t: toks,
        decls,
        refs,
    }
    .run();
}

/// Every other word that names something the script declared, and the
/// host variables.
pub(super) fn uses(
    lines: &Lines,
    toks: &[Token],
    decls: &[Declaration],
    refs: &mut Vec<Reference>,
) {
    let taken: HashSet<usize> = decls
        .iter()
        .map(|d| d.span.start)
        .chain(refs.iter().map(|r| r.span.start))
        .collect();
    for t in toks {
        let span = token_span(lines, t);
        if taken.contains(&span.start) {
            continue;
        }
        match &t.tok {
            Tok::Word(w) | Tok::Delimited(w) => {
                if let Some(d) = decls.iter().find(|d| d.name == *w) {
                    refs.push(Reference {
                        name: w.clone(),
                        kind: d.kind,
                        source: 0,
                        span,
                        declaration: None,
                    });
                }
            }
            Tok::Host { name, .. } => {
                let chars = u32::try_from(name.chars().count()).unwrap_or(0);
                refs.push(Reference {
                    name: name.clone(),
                    kind: SymbolKind::Variable,
                    source: 0,
                    span: lines.span(t.line, t.col + 1, chars),
                    declaration: None,
                });
            }
            _ => {}
        }
    }
}
