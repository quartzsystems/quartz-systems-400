// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text content into spans and documents. Flowed text joins its lines
//! with one blank; `:XMP.`, `:LINES.` and `:FIG.` keep one entry per
//! source line. `&name.` references are recorded for the checks.

use super::Build;
use crate::lex::Piece;
use crate::model::{Block, DefItem, Document, ListKind, ParmItem, Span};
use crate::tags::Kind;
use crate::tree::{Element, Node};

fn node_line(n: &Node) -> usize {
    match n {
        Node::Text(t) => t.line,
        Node::Element(e) => e.line,
    }
}

fn is_inline(n: &Node) -> bool {
    match n {
        Node::Text(_) => true,
        Node::Element(e) => e.def.is_some_and(|d| d.kind == Kind::Inline),
    }
}

/// Consecutive nodes on one source line.
fn by_line(nodes: &[Node]) -> Vec<&[Node]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..=nodes.len() {
        if i == nodes.len() || node_line(&nodes[i]) != node_line(&nodes[start]) {
            out.push(&nodes[start..i]);
            start = i;
        }
    }
    out
}

fn push_text(out: &mut Vec<Span>, t: &str) {
    if t.is_empty() {
        return;
    }
    if let Some(Span::Text(last)) = out.last_mut() {
        last.push_str(t);
    } else {
        out.push(Span::Text(t.to_string()));
    }
}

fn trim_start(spans: &mut [Span]) {
    if let Some(Span::Text(t)) = spans.first_mut() {
        *t = t.trim_start().to_string();
    }
}

fn trim_end(spans: &mut [Span]) {
    if let Some(Span::Text(t)) = spans.last_mut() {
        *t = t.trim_end().to_string();
    }
}

fn drop_empty(spans: Vec<Span>) -> Vec<Span> {
    spans
        .into_iter()
        .filter(|s| !matches!(s, Span::Text(t) if t.is_empty()))
        .collect()
}

impl Build {
    /// The spans of one line's nodes, as written.
    fn raw_spans(&mut self, nodes: &[Node]) -> Vec<Span> {
        let mut out = Vec::new();
        for n in nodes {
            match n {
                Node::Text(t) => {
                    for p in &t.pieces {
                        match p {
                            Piece::Text(s) => push_text(&mut out, s),
                            Piece::Var(v) => {
                                self.refer("Symbol", v, t.line);
                                out.push(Span::Var(v.clone()));
                            }
                        }
                    }
                }
                Node::Element(e) if is_inline(n) => {
                    let inner = self.spans(&e.children);
                    let span = match e.name.as_str() {
                        "PK" => Span::Pk(inner),
                        "PV" => Span::Pv(inner),
                        "RT" => Span::Rt(inner),
                        "LINK" => {
                            let perform = self.text(e, "PERFORM").unwrap_or_default();
                            Span::Link {
                                perform,
                                text: inner,
                            }
                        }
                        hp => {
                            let level = hp
                                .strip_prefix("HP")
                                .and_then(|n| n.parse().ok())
                                .unwrap_or(0);
                            Span::Hp(level, inner)
                        }
                    };
                    out.push(span);
                }
                Node::Element(_) => {}
            }
        }
        out
    }

    /// Flowed text: lines trimmed and joined with one blank.
    pub(super) fn spans(&mut self, nodes: &[Node]) -> Vec<Span> {
        let mut out: Vec<Span> = Vec::new();
        for line in by_line(nodes) {
            let mut s = self.raw_spans(line);
            trim_start(&mut s);
            trim_end(&mut s);
            let s = drop_empty(s);
            if s.is_empty() {
                continue;
            }
            if !out.is_empty() {
                push_text(&mut out, " ");
            }
            for span in s {
                match span {
                    Span::Text(t) => push_text(&mut out, &t),
                    other => out.push(other),
                }
            }
        }
        out
    }

    /// One entry per source line, leading blanks kept.
    pub(super) fn lines(&mut self, nodes: &[Node]) -> Vec<Vec<Span>> {
        let mut out = Vec::new();
        for line in by_line(nodes) {
            let mut s = self.raw_spans(line);
            trim_end(&mut s);
            out.push(drop_empty(s));
        }
        while out.first().is_some_and(Vec::is_empty) {
            out.remove(0);
        }
        while out.last().is_some_and(Vec::is_empty) {
            out.pop();
        }
        out
    }

    /// The text before the first block child.
    pub(super) fn leading_spans(&mut self, el: &Element) -> Vec<Span> {
        self.spans(el.leading())
    }

    /// Help-style content: paragraphs, lists, examples, notes.
    pub(super) fn document(&mut self, el: &Element) -> Document {
        let mut doc = Vec::new();
        let mut pending: Vec<Node> = Vec::new();
        for n in &el.children {
            if is_inline(n) {
                pending.push(n.clone());
                continue;
            }
            self.flush_para(&mut pending, &mut doc);
            let Node::Element(e) = n else { continue };
            if let Some(b) = self.block(e) {
                doc.push(b);
            }
        }
        self.flush_para(&mut pending, &mut doc);
        doc
    }

    fn flush_para(&mut self, pending: &mut Vec<Node>, doc: &mut Document) {
        if pending.is_empty() {
            return;
        }
        let spans = self.spans(pending);
        pending.clear();
        if !spans.is_empty() {
            doc.push(Block::Para(spans));
        }
    }

    fn block(&mut self, e: &Element) -> Option<Block> {
        Some(match e.name.as_str() {
            "H1" | "H2" | "H3" | "H4" => Block::Heading {
                level: e.name[1..].parse().unwrap_or(1),
                text: self.spans(&e.children),
            },
            "P" => Block::Para(self.spans(&e.children)),
            "PC" => Block::ParaCont(self.spans(&e.children)),
            "XMP" => Block::Example(self.lines(&e.children)),
            "LINES" => Block::Lines(self.lines(&e.children)),
            "FIG" => Block::Figure(self.lines(&e.children)),
            "UL" | "OL" | "SL" => {
                let kind = match e.name.as_str() {
                    "UL" => ListKind::Unordered,
                    "OL" => ListKind::Ordered,
                    _ => ListKind::Simple,
                };
                let compact = self.flag(e, "COMPACT");
                let items = e.named("LI").map(|li| self.document(li)).collect();
                Block::List {
                    kind,
                    compact,
                    items,
                }
            }
            "DL" => self.def_list(e),
            "PARML" => {
                let mut items: Vec<ParmItem> = Vec::new();
                for c in e.elements() {
                    match c.name.as_str() {
                        "PT" => items.push(ParmItem {
                            term: self.spans(&c.children),
                            def: Vec::new(),
                        }),
                        "PD" => {
                            let def = self.document(c);
                            match items.last_mut() {
                                Some(it) => it.def.extend(def),
                                None => items.push(ParmItem {
                                    term: Vec::new(),
                                    def,
                                }),
                            }
                        }
                        _ => {}
                    }
                }
                Block::ParmList(items)
            }
            "NT" | "NOTE" => Block::Note(self.document(e)),
            "IMHELP" => {
                let name = self.upper(e, "NAME")?;
                self.refer("Help module", &name, e.line);
                Block::ImHelp(name)
            }
            _ => return None,
        })
    }

    fn def_list(&mut self, e: &Element) -> Block {
        let compact = self.flag(e, "COMPACT");
        let mut heads: Option<(Vec<Span>, Vec<Span>)> = None;
        let mut items: Vec<DefItem> = Vec::new();
        for c in e.elements() {
            match c.name.as_str() {
                "DTHD" => {
                    let t = self.spans(&c.children);
                    heads.get_or_insert_with(Default::default).0 = t;
                }
                "DDHD" => {
                    let t = self.spans(&c.children);
                    heads.get_or_insert_with(Default::default).1 = t;
                }
                "DT" => items.push(DefItem {
                    term: self.spans(&c.children),
                    def: Vec::new(),
                }),
                "DD" => {
                    let def = self.document(c);
                    match items.last_mut() {
                        Some(it) => it.def.extend(def),
                        None => items.push(DefItem {
                            term: Vec::new(),
                            def,
                        }),
                    }
                }
                _ => {}
            }
        }
        Block::DefList {
            compact,
            heads,
            items,
        }
    }
}
