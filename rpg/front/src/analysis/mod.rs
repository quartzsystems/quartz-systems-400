// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The front end as a library for language servers: one source in,
//! the tree, every declaration and reference with its span, the
//! `/COPY` and `/INCLUDE` directives with what the caller supplied for
//! them, and the diagnostics in [`compdiag`]'s form. Spans count the
//! columns of the record itself, fixed form or `**FREE`.

mod symbols;

use std::sync::OnceLock;

use compdiag::front::{Fetch, Include, IncludeRequest, Lines, Span};
use compdiag::msgtext::Table;
use compdiag::{Diagnostic, Where};

use crate::ast::Module;
use crate::reader::{self, Class, Directive, Read, STMF};
use crate::source::{Includes, Member};

pub use compdiag::front::Source;

/// What `analyze` produced.
pub type Analysis = compdiag::front::Analysis<Module>;

/// How to analyze.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// Condition names defined from the start (`DEFINE`).
    pub defines: &'a [String],
    /// The deepest `/COPY` nesting (`COPYNEST`); 0 for the default 32.
    pub copynest: u8,
    /// Where `/COPY`, `/INCLUDE` and `EXEC SQL INCLUDE` members come
    /// from; `None` leaves every one unresolved.
    pub fetch: Option<Fetch<'a>>,
}

/// Analyze `source`.
#[must_use]
pub fn analyze(source: &Source, options: &Options<'_>) -> Analysis {
    let main = member_of(source, "*LIBL", "QRPGLESRC", "MAIN");
    let mut fetcher = Fetcher {
        fetch: options.fetch,
        got: Vec::new(),
    };
    let opts = reader::Options {
        defines: options.defines.to_vec(),
        copynest: if options.copynest == 0 {
            32
        } else {
            options.copynest
        },
    };
    let (read, module, diags) = crate::front(main, &mut fetcher, &opts);
    let mut sources = vec![source.clone()];
    let got = fetcher.got;
    let mut used = vec![false; got.len()];
    for m in read.members.iter().skip(1) {
        let i = got.iter().enumerate().position(|(i, (k, _))| {
            !used[i] && k.lib == m.lib && k.file == m.file && k.name == m.name
        });
        match i {
            Some(i) => {
                used[i] = true;
                sources.push(got[i].1.clone());
            }
            None => sources.push(Source {
                at: where_of(m),
                text: m.records.iter().fold(String::new(), |mut s, r| {
                    s.push_str(&r.text);
                    s.push('\n');
                    s
                }),
            }),
        }
    }
    let map = LineMap::new(&read, &sources);
    let diagnostics = diags.iter().map(|d| map.diagnostic(d)).collect();
    let includes = includes(&read, &map);
    let mut a = Analysis {
        ast: module,
        sources,
        diagnostics,
        includes,
        ..Analysis::default()
    };
    symbols::collect(&mut a, &map);
    a
}

/// A source as the reader's [`Member`]; a member with no name gets
/// `lib`, `file` and `name`.
fn member_of(s: &Source, lib: &str, file: &str, name: &str) -> Member {
    match &s.at {
        Where::Member(m) => Member::from_text(&m.library, &m.file, &m.member, &s.text),
        Where::Path(p) => {
            let dir = p.rsplit_once('/').map_or("", |(d, _)| d);
            Member::from_text(STMF, dir, p, &s.text)
        }
        Where::None => Member::from_text(lib, file, name, &s.text),
    }
}

/// Where a reader's member is.
fn where_of(m: &Member) -> Where {
    if m.lib == STMF {
        Where::Path(m.name.clone())
    } else {
        Where::Member(compdiag::MemberName {
            library: m.lib.clone(),
            file: m.file.clone(),
            member: m.name.clone(),
        })
    }
}

/// The reader's [`Includes`] over the caller's [`Fetch`], keeping what
/// it handed out.
struct Fetcher<'a> {
    fetch: Option<Fetch<'a>>,
    got: Vec<(Member, Source)>,
}

impl Includes for Fetcher<'_> {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        let fetch = self.fetch?;
        let req = if lib == STMF {
            IncludeRequest {
                file: file.into(),
                path: Some(name.into()),
                ..IncludeRequest::default()
            }
        } else {
            IncludeRequest {
                library: lib.into(),
                file: file.into(),
                member: name.into(),
                path: None,
            }
        };
        let s = fetch(&req)?;
        let m = if lib == STMF {
            Member::from_text(STMF, file, name, &s.text)
        } else {
            member_of(&s, lib, file, name)
        };
        self.got.push((m.clone(), s));
        Some(m)
    }
}

/// From the reader's listing lines to sources, lines and columns.
pub(crate) struct LineMap {
    /// By listing line (from 1): the source and its line (from 1).
    at: Vec<(usize, u32)>,
    /// Each source's lines.
    lines: Vec<Lines>,
    /// Each source's `Where`.
    wheres: Vec<Where>,
}

impl LineMap {
    fn new(read: &Read, sources: &[Source]) -> Self {
        // A member's records are read in order, one listing line each;
        // a copy starts its member over at the next depth.
        let mut stack: Vec<(usize, usize)> = Vec::new();
        let mut at = Vec::with_capacity(read.lines.len());
        for l in &read.lines {
            let depth = usize::from(l.depth);
            stack.truncate(depth + 1);
            let idx = match stack.get_mut(depth) {
                Some((m, next)) if *m == l.member => {
                    *next += 1;
                    *next - 1
                }
                _ => {
                    stack.truncate(depth);
                    stack.push((l.member, 1));
                    0
                }
            };
            let recs = &read.members[l.member].records;
            let idx = if recs.get(idx).is_some_and(|r| r.seq == l.seq) {
                idx
            } else {
                recs.iter().position(|r| r.seq == l.seq).unwrap_or(idx)
            };
            at.push((l.member, u32::try_from(idx + 1).unwrap_or(u32::MAX)));
        }
        Self {
            at,
            lines: sources.iter().map(|s| Lines::new(&s.text)).collect(),
            wheres: sources.iter().map(|s| s.at.clone()).collect(),
        }
    }

    /// The source and line of listing line `no`.
    pub(crate) fn place(&self, no: u32) -> Option<(usize, u32)> {
        let i = usize::try_from(no).ok()?.checked_sub(1)?;
        self.at.get(i).copied()
    }

    /// The span of `name` near column `col` of listing line `no`.
    pub(crate) fn find(&self, no: u32, col: u16, name: &str) -> Option<(usize, Span)> {
        let (s, line) = self.place(no)?;
        Some((s, self.lines[s].find(line, u32::from(col), name)))
    }

    /// The whole of listing line `no`.
    fn line_span(&self, no: u32) -> Option<(usize, Span)> {
        let (s, line) = self.place(no)?;
        Some((s, self.lines[s].line_span(line)))
    }

    fn diagnostic(&self, d: &crate::Diag) -> Diagnostic {
        let (text, help) = texts()
            .texts(d.msgid, &d.args)
            .unwrap_or_else(|| (d.args.join(" "), String::new()));
        let (at, line, column) = match self.place(d.at.line) {
            Some((s, line)) => (self.wheres[s].clone(), line, d.at.col),
            None => (self.wheres[0].clone(), 0, 0),
        };
        Diagnostic {
            at,
            line,
            column,
            end_line: 0,
            end_column: 0,
            severity: d.severity,
            id: d.msgid.into(),
            text,
            help,
        }
    }
}

/// The compiler's message texts (`QRPGLEMSG`).
fn texts() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table::parse(include_str!("../../../../runtime/msgf/msgs/qrpglemsg.toml")))
}

/// The directives and what each brought in: the next line one level
/// deeper.
fn includes(read: &Read, map: &LineMap) -> Vec<Include> {
    let mut out = Vec::new();
    let into = |i: usize| {
        let l = &read.lines[i];
        read.lines
            .get(i + 1)
            .filter(|n| n.depth == l.depth + 1)
            .map(|n| n.member)
    };
    for (i, l) in read.lines.iter().enumerate() {
        let Some((source, span)) = map.line_span(l.no) else {
            continue;
        };
        if let Class::Directive(Directive::Copy {
            lib,
            file,
            member,
            include,
            word,
        }) = &l.class
        {
            let request = if lib == STMF {
                IncludeRequest {
                    path: Some(member.clone()),
                    ..IncludeRequest::default()
                }
            } else {
                IncludeRequest {
                    library: lib.clone(),
                    file: file.clone(),
                    member: member.clone(),
                    path: (read.members[0].lib == STMF && !word.contains(','))
                        .then(|| word.clone()),
                }
            };
            out.push(Include {
                directive: if *include { "/INCLUDE" } else { "/COPY" }.into(),
                request,
                source,
                span,
                resolved: into(i),
            });
        }
    }
    for inc in &read.sql_includes {
        let Some((source, span)) = map.line_span(inc.line) else {
            continue;
        };
        let text = map.lines[source].text(span.line).to_ascii_uppercase();
        let name = text
            .split("INCLUDE")
            .nth(1)
            .and_then(|r| {
                r.split(|c: char| c == ';' || c.is_whitespace())
                    .find(|w| !w.is_empty())
            })
            .unwrap_or("")
            .to_string();
        out.push(Include {
            directive: "EXEC SQL INCLUDE".into(),
            request: IncludeRequest {
                library: read.members[0].lib.clone(),
                file: read.members[0].file.clone(),
                member: name,
                path: None,
            },
            source,
            span,
            resolved: inc.member,
        });
    }
    out.sort_by_key(|i| (i.source, i.span.start));
    out
}

#[cfg(test)]
mod tests;
