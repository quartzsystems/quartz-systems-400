// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Command help on the command pages: the help modules of the shipped
//! command help groups (`HLPPNLGRP`, module `HLPID` and `HLPID/KWD`),
//! the same text the prompter's F1 shows, as HTML.

use std::collections::HashMap;
use std::fmt::Write as _;

use cmd::def::CmdDef;
use uim::model::{Block, Document, HelpModule, ListKind, PanelGroup, Span};

use crate::md::esc;

/// The shipped help groups, compiled once.
#[derive(Default)]
pub struct Help {
    groups: HashMap<String, Option<PanelGroup>>,
}

impl Help {
    fn group(&mut self, name: &str) -> Option<&PanelGroup> {
        let key = name.to_ascii_uppercase();
        self.groups
            .entry(key.clone())
            .or_insert_with(|| {
                let src = uim::shipped::source(&key)?;
                uim::compile(
                    src,
                    &uim::NoImbeds,
                    &uim::NoImports,
                    &uim::CompileOpts::default(),
                )
                .group
            })
            .as_ref()
    }

    /// A command's help module, or one of its parameters' (`kwd`).
    pub fn module(&mut self, def: &CmdDef, kwd: Option<&str>) -> Option<HelpModule> {
        let g = def.hlppnlgrp.as_ref()?;
        let id = def.hlpid.clone().unwrap_or_else(|| def.name.clone());
        let name = match kwd {
            Some(k) => format!("{id}/{k}"),
            None => id,
        };
        self.group(&g.name)?.help(&name).cloned()
    }
}

/// Inline text as HTML.
fn spans(s: &[Span]) -> String {
    let mut out = String::new();
    for x in s {
        match x {
            Span::Text(t) => out.push_str(&esc(t)),
            Span::Var(v) => {
                let _ = write!(out, "&amp;{}.", esc(v));
            }
            Span::Hp(_, inner) | Span::Rt(inner) => {
                let _ = write!(out, "<b>{}</b>", spans(inner));
            }
            Span::Pk(inner) => {
                let _ = write!(out, "<span class=\"mono\">{}</span>", spans(inner));
            }
            Span::Pv(inner) => {
                let _ = write!(out, "<i>{}</i>", spans(inner));
            }
            Span::Link { text, .. } => out.push_str(&spans(text)),
        }
    }
    out
}

/// A help document as HTML.
#[must_use]
pub fn html(doc: &Document) -> String {
    let mut out = String::new();
    for b in doc {
        match b {
            Block::Heading { text, .. } => {
                let _ = writeln!(out, "<h4>{}</h4>", spans(text));
            }
            Block::Para(s) | Block::ParaCont(s) => {
                let _ = writeln!(out, "<p>{}</p>", spans(s));
            }
            Block::Example(lines) | Block::Lines(lines) | Block::Figure(lines) => {
                out.push_str("<pre>");
                for l in lines {
                    out.push_str(&spans(l));
                    out.push('\n');
                }
                out.push_str("</pre>\n");
            }
            Block::List { kind, items, .. } => {
                let tag = if *kind == ListKind::Ordered {
                    "ol"
                } else {
                    "ul"
                };
                let _ = writeln!(out, "<{tag}>");
                for i in items {
                    let _ = writeln!(out, "<li>{}</li>", html(i));
                }
                let _ = writeln!(out, "</{tag}>");
            }
            Block::DefList { items, .. } => {
                out.push_str("<dl>\n");
                for i in items {
                    let _ = writeln!(out, "<dt>{}</dt><dd>{}</dd>", spans(&i.term), html(&i.def));
                }
                out.push_str("</dl>\n");
            }
            Block::ParmList(items) => {
                out.push_str("<dl class=\"parml\">\n");
                for i in items {
                    let _ = writeln!(
                        out,
                        "<dt class=\"mono\">{}</dt><dd>{}</dd>",
                        spans(&i.term),
                        html(&i.def)
                    );
                }
                out.push_str("</dl>\n");
            }
            Block::Note(d) => {
                let _ = writeln!(out, "<div class=\"note\"><b>Note:</b> {}</div>", html(d));
            }
            Block::ImHelp(_) => {}
        }
    }
    out
}
