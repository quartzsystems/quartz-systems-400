// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command definition compiler as a library for language servers:
//! one definition source in, its statements and the definition, the
//! keywords, lists and prompt controls it declares and every use of
//! them with its span, and the diagnostics `CRTCMD` would give, in
//! [`compdiag`]'s form. Definition sources include nothing, so
//! [`Analysis::includes`](compdiag::front::Analysis::includes) stays
//! empty.

use std::sync::OnceLock;

use compdiag::front::{Cursor, Declaration, Lines, Reference, Span, SymbolKind};
use compdiag::msgtext::Table;
use compdiag::{Diagnostic, Where, severity};

use crate::def::CmdDef;
use crate::error::Diag;
use crate::source::{self, Statement};
use crate::token::{self, Options as TokOptions, Tok};

pub use compdiag::front::Source;

/// The tree: the statements read and, when the source compiles, the
/// definition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tree {
    /// Every statement that tokenized, in order.
    pub statements: Vec<Statement>,
    /// The definition, when there is no error.
    pub definition: Option<CmdDef>,
}

/// What `analyze` produced.
pub type Analysis = compdiag::front::Analysis<Tree>;

/// How to analyze.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The command's name (`CRTCMD CMD`); the member's name when not
    /// given.
    pub name: Option<String>,
}

/// Analyze a command definition source.
#[must_use]
pub fn analyze(source: &Source, options: &Options) -> Analysis {
    let name = options.name.clone().unwrap_or_else(|| match &source.at {
        Where::Member(m) => m.member.clone(),
        Where::Path(p) => p
            .rsplit('/')
            .next()
            .unwrap_or("")
            .split('.')
            .next()
            .unwrap_or("")
            .to_string(),
        Where::None => "CMD".into(),
    });
    let lines = Lines::new(&source.text);
    let raw: Vec<&str> = source.text.lines().collect();
    let mut a = Analysis {
        sources: vec![source.clone()],
        ..Analysis::default()
    };
    let mut ranges = Vec::new();
    for j in token::join(&raw) {
        let Ok(t) = token::tokenize(&j.text, TokOptions::default()) else {
            continue;
        };
        let mut kws = Vec::new();
        for tok in t.toks {
            if let Tok::Kw { name, args, .. } = tok {
                kws.push((name, args));
            }
        }
        a.ast.statements.push(Statement {
            label: t.label,
            kind: t.name,
            kws,
            line: j.first,
        });
        ranges.push((line_no(j.first), line_no(j.last)));
    }
    for (s, &(first, last)) in a.ast.statements.iter().zip(&ranges) {
        let mut c = Cursor::new(&lines, first, last);
        symbols(s, &name, &mut c, &mut a.declarations, &mut a.references);
    }
    match source::compile(&name, &source.text) {
        Ok(d) => a.ast.definition = Some(d),
        Err(diags) => {
            a.diagnostics = diags
                .iter()
                .map(|d| diagnostic(d, &source.at, &a.ast.statements, &lines))
                .collect();
        }
    }
    a.link(|_| None);
    a
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One statement's declarations and references.
fn symbols(
    s: &Statement,
    cmd: &str,
    c: &mut Cursor<'_>,
    decls: &mut Vec<Declaration>,
    refs: &mut Vec<Reference>,
) {
    let mut decl = |name: &str, kind, span: Span, detail: String| {
        decls.push(Declaration {
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
        decl(l, SymbolKind::Label, span, format!("{} list", s.kind));
    }
    let Some(kind_span) = c.next(&s.kind) else {
        return;
    };
    if s.kind == "CMD" {
        let prompt = s
            .kws
            .iter()
            .find(|(k, _)| k == "PROMPT")
            .map(|(_, a)| text_of(a))
            .unwrap_or_default();
        decl(cmd, SymbolKind::Command, kind_span, prompt);
    }
    for (k, args) in &s.kws {
        if c.next(k).is_none() {
            continue;
        }
        let word = match args.as_slice() {
            [Tok::Word(w)] => Some(w.as_str()),
            _ => None,
        };
        match (s.kind.as_str(), k.as_str(), word) {
            ("PARM", "KWD", Some(w)) => {
                if let Some(span) = c.next(w) {
                    decl(w, SymbolKind::Keyword, span, parm_detail(s));
                }
                continue;
            }
            ("PARM" | "ELEM" | "QUAL", "TYPE" | "PMTCTL", Some(w)) if !w.starts_with('*') => {
                if let Some(span) = c.next(w) {
                    refs.push(reference(w, SymbolKind::Label, span));
                }
                continue;
            }
            ("PMTCTL", "CTL", Some(w)) if !w.starts_with('*') => {
                if let Some(span) = c.next(w) {
                    refs.push(reference(w, SymbolKind::Keyword, span));
                }
                continue;
            }
            _ => {}
        }
        vars(args, c, refs);
    }
}

/// Every `&KWD` in `args`: a use of a parameter's keyword.
fn vars(args: &[Tok], c: &mut Cursor<'_>, refs: &mut Vec<Reference>) {
    for t in args {
        match t {
            Tok::Var(v) => {
                if let Some(mut span) = c.next(&format!("&{v}")) {
                    span.start += 1;
                    span.column += 1;
                    refs.push(reference(v, SymbolKind::Keyword, span));
                }
            }
            Tok::Kw { args, .. } | Tok::Builtin { args, .. } | Tok::Paren(args) => {
                vars(args, c, refs);
            }
            _ => {}
        }
    }
}

fn reference(name: &str, kind: SymbolKind, span: Span) -> Reference {
    Reference {
        name: name.to_ascii_uppercase(),
        kind,
        source: 0,
        span,
        declaration: None,
    }
}

/// A `PARM`'s type and length, as the declaration's detail.
fn parm_detail(s: &Statement) -> String {
    let get = |k: &str| {
        s.kws
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, a)| Tok::join(a))
    };
    match (get("TYPE"), get("LEN")) {
        (Some(t), Some(l)) => format!("{t} {l}"),
        (Some(t), None) => t,
        _ => "*CHAR".into(),
    }
}

/// A quoted argument's text.
fn text_of(args: &[Tok]) -> String {
    match args {
        [Tok::Str(s)] => s.clone(),
        _ => Tok::join(args),
    }
}

/// A compile diagnostic: its line from the `Line n:` the compiler
/// notes, else the `PARM` of the keyword in error.
fn diagnostic(d: &Diag, at: &Where, stmts: &[Statement], lines: &Lines) -> Diagnostic {
    let (line, detail) = match d
        .detail
        .strip_prefix("Line ")
        .and_then(|r| r.split_once(": "))
        .and_then(|(n, rest)| Some((n.parse::<u32>().ok()?, rest)))
    {
        Some((n, rest)) => (n, rest.to_string()),
        None => {
            // The rules name the keyword in the first 10 bytes of the data.
            let kwd = d
                .parm
                .clone()
                .or_else(|| (d.data.len() >= 10).then(|| ccsid::decode_trimmed(&d.data[..10])));
            let n = kwd.as_ref().and_then(|p| {
                stmts.iter().rev().find(|s| {
                    s.kind == "PARM"
                        && s.kws.iter().any(|(k, a)| {
                            k == "KWD" && matches!(a.as_slice(), [Tok::Word(w)] if w == p)
                        })
                })
            });
            (n.map_or(0, |s| line_no(s.line)), d.detail.clone())
        }
    };
    let msg = texts().get(&d.msgid);
    let values = msg
        .map(|m| data_values(&d.data, &m.vars))
        .unwrap_or_default();
    let (text, help) = match msg {
        Some(m) if detail.is_empty() => (
            compdiag::msgtext::fill(&m.text, &values),
            compdiag::msgtext::fill(&m.help, &values),
        ),
        Some(m) => (detail, compdiag::msgtext::fill(&m.help, &values)),
        None => (detail, String::new()),
    };
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
        severity: msg.map_or(severity::SEVERE, |m| m.severity.max(severity::SEVERE)),
        id: d.msgid.clone(),
        text,
        help,
    }
}

/// CP037 message data cut by the message's `*CHAR n` variables.
fn data_values(data: &[u8], vars: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    for v in vars {
        let mut w = v.split_whitespace();
        let (Some("*CHAR"), Some(n)) = (w.next(), w.next().and_then(|n| n.parse::<usize>().ok()))
        else {
            break;
        };
        let end = (at + n).min(data.len());
        out.push(ccsid::from_ebcdic(&data[at..end]).trim_end().to_string());
        at = end;
    }
    out
}

/// The command messages (`QCPFMSG`).
fn texts() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table::parse(include_str!("../../../../runtime/msgf/msgs/qcpfmsg.toml")))
}

#[cfg(test)]
mod tests;
