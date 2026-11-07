// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CL front end as a library for language servers: one source in,
//! the program's statements, every variable, file, label and
//! subroutine declared and every use of them with its span, and the
//! diagnostics in [`compdiag`]'s form. The front end's own checks run
//! (statements, declarations, the tables) and, from the generator's,
//! the names that resolve to nothing: an undeclared variable
//! (`CPD0727`), a `GOTO` to no label (`CPD0735`), a `CALLSUBR` of no
//! subroutine (`CPD0795`). Command parameters are checked against
//! their definitions only by the compiler. CL has no `/COPY`, so
//! [`Analysis::includes`](compdiag::front::Analysis::includes) stays
//! empty.

mod scan;

use std::sync::OnceLock;

use compdiag::front::{Cursor, Declaration, Lines, Reference, Span, SymbolKind};
use compdiag::msgtext::Table;
use compdiag::{Diagnostic, Where};

use crate::ast::{Kind, Program, VarType};
use crate::diag::{Diag, ids};
use crate::sem::{self, Files, Format, Tables};
use crate::{lex, parse};

pub use compdiag::front::Source;

/// The tree: the statements and the tables built from them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    /// The program.
    pub program: Program,
    /// Its variables, labels, subroutines and files.
    pub tables: Tables,
}

/// What `analyze` produced.
pub type Analysis = compdiag::front::Analysis<Tree>;

/// Where a `DCLF` file's formats come from: the file and library
/// named, to whether it is a display file and its formats.
pub type FileFetch<'a> = &'a dyn Fn(&str, &str) -> Option<(bool, Vec<Format>)>;

/// How to analyze.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// The files `DCLF` declares; `None` reports each as not found, as
    /// the compiler does without the file.
    pub files: Option<FileFetch<'a>>,
}

struct Fetched<'a>(Option<FileFetch<'a>>);

impl Files for Fetched<'_> {
    fn file(&mut self, name: &str, lib: &str) -> Option<(bool, Vec<Format>)> {
        (self.0?)(name, lib)
    }
}

/// Analyze a CL source.
#[must_use]
pub fn analyze(source: &Source, options: &Options<'_>) -> Analysis {
    let (raws, mut diags) = lex::statements(&source.text);
    let program = parse::program(&raws, &mut diags);
    let tables = sem::tables(&program, &mut Fetched(options.files), &mut diags);
    let lines = Lines::new(&source.text);
    let mut a = Analysis {
        sources: vec![source.clone()],
        ..Analysis::default()
    };
    // Statement n's lines.
    let range = |n: usize| {
        raws.iter()
            .find(|r| r.n == n)
            .map_or((0, 0), |r| (line_no(r.line), line_no(r.last)))
    };
    for s in &program.stmts {
        let (first, last) = range(s.n);
        if first == 0 {
            continue;
        }
        let mut c = Cursor::new(&lines, first, last);
        declarations(s, &tables, &mut c, &mut a.declarations);
        let mut c = Cursor::new(&lines, first, last);
        names(s, &mut c, &mut a.references);
        for (line, col, name) in scan::variables(&lines, first, last) {
            let span = lines.span(line, col, u32::try_from(name.chars().count()).unwrap_or(0));
            let decl_here = a
                .declarations
                .iter()
                .any(|d| d.kind != SymbolKind::File && d.span.start == span.start + 1);
            if !decl_here {
                a.references.push(Reference {
                    name: name[1..].to_ascii_uppercase(),
                    kind: SymbolKind::Variable,
                    source: 0,
                    span: Span {
                        start: span.start + 1,
                        column: span.column + 1,
                        ..span
                    },
                    declaration: None,
                });
            }
        }
    }
    // Fields of declared files: declared by their DCLF.
    for v in tables.order.iter().filter_map(|n| tables.var(n)) {
        if v.file.is_some()
            && let Some(f) = a
                .declarations
                .iter()
                .find(|d| d.kind == SymbolKind::File && d.span.line == line_no(v.line))
        {
            let span = f.span;
            a.declarations.push(Declaration {
                name: v.name.clone(),
                kind: SymbolKind::Field,
                source: 0,
                span,
                detail: type_detail(v.ty, v.len, v.dec),
                scope: Some(f.name.clone()),
            });
        }
    }
    a.references.sort_by_key(|r| r.span.start);
    a.link(|_| None);
    unresolved(&a, &program, &tables, &mut diags);
    diags.sort_by_key(|d| (d.line, d.msgid.clone()));
    a.diagnostics = diags
        .iter()
        .map(|d| diagnostic(d, &source.at, &lines))
        .collect();
    a.ast = Tree { program, tables };
    a
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn type_detail(ty: VarType, len: u32, dec: u8) -> String {
    let t = match ty {
        VarType::Char => "*CHAR",
        VarType::Dec => "*DEC",
        VarType::Lgl => "*LGL",
        VarType::Int => "*INT",
        VarType::Uint => "*UINT",
        VarType::Ptr => "*PTR",
    };
    match ty {
        VarType::Dec => format!("{t} ({len} {dec})"),
        VarType::Ptr | VarType::Lgl => t.into(),
        _ => format!("{t} {len}"),
    }
}

/// The statement's own declarations: its label, a `DCL` variable, a
/// `DCLF` file, a `SUBR`.
fn declarations(s: &crate::ast::Stmt, t: &Tables, c: &mut Cursor<'_>, out: &mut Vec<Declaration>) {
    let mut push = |name: &str, kind, span: Span, detail: String| {
        out.push(Declaration {
            name: name.to_ascii_uppercase(),
            kind,
            source: 0,
            span,
            detail,
            scope: None,
        });
    };
    if let Some(l) = &s.label
        && let Some(span) = c.next(l)
    {
        push(l, SymbolKind::Label, span, "label".into());
    }
    match &s.kind {
        Kind::Dcl(d) => {
            if let Some(span) = c.next(&format!("&{}", d.var)) {
                let detail = t.var(&d.var).map_or_else(String::new, |v| {
                    let mut s = type_detail(v.ty, v.len, v.dec);
                    if v.parm {
                        s.push_str(" parameter");
                    }
                    s
                });
                push(
                    &d.var,
                    SymbolKind::Variable,
                    Span {
                        start: span.start + 1,
                        column: span.column + 1,
                        ..span
                    },
                    detail,
                );
            }
        }
        Kind::DclF(f) => {
            if let Some(span) = c.next(&f.file) {
                push(
                    &f.file,
                    SymbolKind::File,
                    span,
                    format!("{}/{}", f.lib, f.file),
                );
            }
        }
        Kind::Subr(name) => {
            if c.next("SUBR").is_some()
                && let Some(span) = c.next(name)
            {
                push(name, SymbolKind::Subroutine, span, "subroutine".into());
            }
        }
        _ => {}
    }
}

/// Uses of labels and subroutines.
fn names(s: &crate::ast::Stmt, c: &mut Cursor<'_>, out: &mut Vec<Reference>) {
    let (word, name, kind) = match &s.kind {
        Kind::Goto(l) => ("GOTO", l, SymbolKind::Label),
        Kind::Leave(Some(l)) => ("LEAVE", l, SymbolKind::Label),
        Kind::Iterate(Some(l)) => ("ITERATE", l, SymbolKind::Label),
        Kind::CallSubr(n, _) => ("CALLSUBR", n, SymbolKind::Subroutine),
        Kind::If(_, Some(inner))
        | Kind::Else(Some(inner))
        | Kind::When(_, Some(inner))
        | Kind::Otherwise(Some(inner))
        | Kind::MonMsg(_, _, Some(inner)) => {
            // The command after THEN, DO or EXEC.
            let _ = c.next(match &s.kind {
                Kind::MonMsg(..) => "EXEC",
                Kind::Else(_) | Kind::Otherwise(_) => "CMD",
                _ => "THEN",
            });
            return names(inner, c, out);
        }
        _ => return,
    };
    if c.next(word).is_none() {
        return;
    }
    if let Some(span) = c.next(name) {
        out.push(Reference {
            name: name.to_ascii_uppercase(),
            kind,
            source: 0,
            span,
            declaration: None,
        });
    }
}

/// The generator's checks of names that resolve to nothing.
fn unresolved(a: &Analysis, p: &Program, t: &Tables, diags: &mut Vec<Diag>) {
    let has = |diags: &Vec<Diag>, line: usize, id: &str| {
        diags.iter().any(|d| d.line == line && d.msgid == id)
    };
    for r in &a.references {
        let line = r.span.line as usize;
        let d = match r.kind {
            SymbolKind::Variable if t.var(&r.name).is_none() => Diag::at(
                line,
                ids::UNDECLARED,
                format!("&{} is not declared", r.name),
            ),
            _ => continue,
        };
        if !has(diags, line, &d.msgid) {
            diags.push(d);
        }
    }
    let mut stmts: Vec<&crate::ast::Stmt> = Vec::new();
    for s in &p.stmts {
        let mut s = s;
        loop {
            stmts.push(s);
            match &s.kind {
                Kind::If(_, Some(i))
                | Kind::Else(Some(i))
                | Kind::When(_, Some(i))
                | Kind::Otherwise(Some(i))
                | Kind::MonMsg(_, _, Some(i)) => s = i,
                _ => break,
            }
        }
    }
    for s in stmts {
        let d = match &s.kind {
            Kind::Goto(l) if !t.labels.contains_key(l) => {
                Diag::at(s.line, ids::NO_LABEL, format!("label {l} is not defined"))
            }
            Kind::CallSubr(n, _) if !t.subrs.contains_key(n) => {
                Diag::at(s.line, ids::SUBR, format!("subroutine {n} is not defined"))
            }
            _ => continue,
        };
        if !has(diags, s.line, &d.msgid) {
            diags.push(d);
        }
    }
}

fn diagnostic(d: &Diag, at: &Where, lines: &Lines) -> Diagnostic {
    let line = line_no(d.line);
    let column = if line > 0 {
        u16::try_from(
            lines
                .text(line)
                .chars()
                .position(|c| !c.is_whitespace())
                .map_or(0, |i| i + 1),
        )
        .unwrap_or(0)
    } else {
        0
    };
    Diagnostic {
        at: at.clone(),
        line,
        column,
        end_line: 0,
        end_column: 0,
        severity: d.severity.number(),
        id: d.msgid.clone(),
        text: d.text.clone(),
        help: texts()
            .get(&d.msgid)
            .map(|m| m.help.clone())
            .unwrap_or_default(),
    }
}

/// The CL compiler's messages (`QCPFMSG`).
fn texts() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table::parse(include_str!("../../../../runtime/msgf/msgs/qcpfmsg.toml")))
}

#[cfg(test)]
mod tests;
