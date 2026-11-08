// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDS as a library for language servers: one source in, its
//! specifications and the definition, every record format and field
//! declared and every key, select/omit and keyword use of a name with
//! its span, and the compiler's diagnostics in [`compdiag`]'s form.
//! Columns are the record's own (name in 19-28, keywords from 45).
//! DDS includes no source, so
//! [`Analysis::includes`](compdiag::front::Analysis::includes) stays
//! empty; the files `REF`, `REFFLD`, `PFILE` and `JFILE` name are
//! references the caller's [`RefResolver`] answers.

use compdiag::front::{Cursor, Declaration, Lines, Reference, Span, SymbolKind};
use compdiag::{Diagnostic, Where};

use crate::compile::CompileOpts;
use crate::diag::Diagnostics;
use crate::lex::{self, Keyword, NameType, Parm, Spec};
use crate::model::{LfDefinition, PfDefinition};
use crate::prtf::PrtfDefinition;
use crate::refs::{NoRefs, RefResolver};
use crate::{compile_lf, compile_pf, compile_prtf};

pub use compdiag::front::Source;

/// Which compiler the source is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// `CRTPF` (source type `PF`).
    #[default]
    Physical,
    /// `CRTLF` (`LF`).
    Logical,
    /// `CRTPRTF` (`PRTF`).
    Printer,
    /// `CRTDSPF` (`DSPF`): only the checks this crate shares with the
    /// display-file compiler (option indicators, [`crate::cond`]);
    /// `dspf::analysis::analyze` runs the compiler itself.
    Display,
}

impl Kind {
    /// The kind of a source type (`PF`, `LF`, `PRTF`).
    #[must_use]
    pub fn of_srctype(srctype: &str) -> Option<Self> {
        match srctype.trim().to_ascii_uppercase().as_str() {
            "PF" => Some(Self::Physical),
            "LF" => Some(Self::Logical),
            "PRTF" => Some(Self::Printer),
            "DSPF" => Some(Self::Display),
            _ => None,
        }
    }
}

/// A compiled definition.
#[derive(Clone, Debug)]
pub enum Definition {
    /// A physical file.
    Physical(PfDefinition),
    /// A logical file.
    Logical(LfDefinition),
    /// A printer file.
    Printer(PrtfDefinition),
}

/// The tree: the specifications and, when nothing at severity 20 or
/// more was found, the definition.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    /// The specifications, in order.
    pub specs: Vec<Spec>,
    /// The definition.
    pub definition: Option<Definition>,
}

/// What `analyze` produced.
pub type Analysis = compdiag::front::Analysis<Tree>;

/// How to analyze.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// The compiler.
    pub kind: Kind,
    /// Where referenced files come from; `None` finds none.
    pub refs: Option<&'a dyn RefResolver>,
}

/// The keywords whose parameters name fields, and those that name
/// files (`lib/file`).
const FIELD_KWS: &[&str] = &["REFFLD", "JFLD", "CONCAT", "RENAME", "SST", "ALIAS"];
const FILE_KWS: &[&str] = &["REF", "PFILE", "JFILE", "FORMAT", "REFACCPTH"];

/// Analyze a DDS source.
#[must_use]
pub fn analyze(source: &Source, options: &Options<'_>) -> Analysis {
    let resolver: &dyn RefResolver = options.refs.unwrap_or(&NoRefs);
    let opts = CompileOpts::default();
    analyze_with(source, |src| match options.kind {
        Kind::Physical => {
            let c = compile_pf(src, resolver, &opts);
            (c.definition.map(Definition::Physical), c.diagnostics)
        }
        Kind::Logical => {
            let c = compile_lf(src, resolver, &opts);
            (c.definition.map(Definition::Logical), c.diagnostics)
        }
        Kind::Printer => {
            let c = compile_prtf(src, resolver, &opts);
            (c.definition.map(Definition::Printer), c.diagnostics)
        }
        Kind::Display => {
            let mut d = Diagnostics::default();
            lex::lex(src, &mut d);
            crate::cond::check_lines(src, &mut d);
            (None, d.into_vec())
        }
    })
}

/// Analyze a DDS source whose diagnostics and definition come from
/// `compile` (another crate's compiler: `dspf` passes `CRTDSPF`'s).
#[must_use]
pub fn analyze_with(
    source: &Source,
    compile: impl FnOnce(&crate::Source) -> (Option<Definition>, Vec<crate::Diagnostic>),
) -> Analysis {
    let (src, mut first) = crate::Source::from_text(&source.text);
    let (definition, diags) = compile(&src);
    first.extend(diags);
    let specs = lex::lex(&src, &mut Diagnostics::default());
    let lines = Lines::new(&source.text);
    let mut a = Analysis {
        sources: vec![source.clone()],
        diagnostics: first
            .iter()
            .map(|d| diagnostic(d, &source.at, &lines))
            .collect(),
        ..Analysis::default()
    };
    let mut format: Option<String> = None;
    for (i, s) in specs.iter().enumerate() {
        let line = line_no(s.line + 1);
        let last = specs.get(i + 1).map_or_else(
            || u32::try_from(lines.len()).unwrap_or(u32::MAX),
            |n| line_no(n.line),
        );
        if let Some(name) = &s.name {
            let span = lines.find(line, 19, name);
            match s.name_type {
                Some(NameType::Record) => {
                    format = Some(name.clone());
                    a.declarations.push(decl(
                        name,
                        SymbolKind::Format,
                        span,
                        "record format",
                        None,
                    ));
                }
                None => {
                    let detail = field_detail(s);
                    a.declarations.push(decl(
                        name,
                        SymbolKind::Field,
                        span,
                        &detail,
                        format.clone(),
                    ));
                }
                Some(NameType::Key | NameType::Select | NameType::Omit) => {
                    a.references.push(reference(name, SymbolKind::Field, span));
                }
                Some(NameType::Join | NameType::Help) => {}
            }
        }
        for k in &s.keywords {
            let mut c = Cursor::new(&lines, line_no(k.line + 1), last.max(line_no(k.line + 1)))
                .from_column(45);
            keyword(k, &mut c, &mut a.references);
        }
    }
    let scopes: Vec<(String, u32)> = a
        .declarations
        .iter()
        .filter(|d| d.kind == SymbolKind::Format)
        .map(|d| (d.name.clone(), d.span.line))
        .collect();
    a.link(|r| {
        scopes
            .iter()
            .rev()
            .find(|(_, l)| *l <= r.span.line)
            .map(|(n, _)| n.clone())
    });
    a.ast = Tree { specs, definition };
    a
}

fn line_no(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn decl(
    name: &str,
    kind: SymbolKind,
    span: Span,
    detail: &str,
    scope: Option<String>,
) -> Declaration {
    Declaration {
        name: name.to_string(),
        kind,
        source: 0,
        span,
        detail: detail.to_string(),
        scope,
    }
}

fn reference(name: &str, kind: SymbolKind, span: Span) -> Reference {
    Reference {
        name: name.to_string(),
        kind,
        source: 0,
        span,
        declaration: None,
    }
}

/// A field's length, type and decimals as written.
fn field_detail(s: &Spec) -> String {
    let mut d = String::new();
    if s.reference {
        d.push_str("R ");
    }
    if let Some(l) = s.length {
        d.push_str(&l.to_string());
    }
    if let Some(t) = s.data_type {
        d.push(t);
    }
    if let Some(p) = s.decimals {
        d.push(' ');
        d.push_str(&p.to_string());
    }
    d.trim().to_string()
}

/// The names a keyword's parameters use.
fn keyword(k: &Keyword, c: &mut Cursor<'_>, out: &mut Vec<Reference>) {
    if c.next(&k.name).is_none() {
        return;
    }
    let field = FIELD_KWS.contains(&k.name.as_str());
    let file = FILE_KWS.contains(&k.name.as_str());
    if !field && !file {
        return;
    }
    let mut words = Vec::new();
    for p in &k.parms {
        flatten(p, &mut words);
    }
    for (n, w) in words.iter().enumerate() {
        if w.starts_with('*') || w.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // `REFFLD(field file)` and `REFFLD(format/field)`: a field, then
        // the file it is in.
        let kind = if file || (k.name == "REFFLD" && n > 0) {
            SymbolKind::File
        } else {
            SymbolKind::Field
        };
        let name = w.rsplit('/').next().unwrap_or(w);
        if let Some(span) = c.next(name) {
            out.push(reference(name, kind, span));
        }
    }
}

fn flatten(p: &Parm, out: &mut Vec<String>) {
    match p {
        Parm::Word(w) => out.push(w.clone()),
        Parm::Group(g) => {
            for x in g {
                flatten(x, out);
            }
        }
        Parm::Quoted(_) | Parm::Hex(_) => {}
    }
}

fn diagnostic(d: &crate::Diagnostic, at: &Where, lines: &Lines) -> Diagnostic {
    let line = d.line.map_or(0, |l| line_no(l + 1));
    let column = if line > 0 && !lines.text(line).trim().is_empty() {
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
        severity: d.severity.code(),
        id: d.id.into(),
        text: d.text.clone(),
        help: String::new(),
    }
}

#[cfg(test)]
mod tests;
