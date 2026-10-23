// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Documents (help text, information and text areas) flowed into rows
//! of attributed segments: headings at the heading column, paragraphs
//! wrapped between the text column and the limit, a blank row between
//! blocks (R20).

use super::ctx::Ctx;
use super::screen::{Attr, Screen};
use crate::model::{Block, Document, ListKind, Span};

/// A run of one attribute on a row.
pub(super) struct Seg {
    pub col: usize,
    pub attr: Attr,
    pub text: String,
    /// The help module a `:LINK.` shows.
    pub link: Option<String>,
}

/// One row of a flowed document.
pub(super) type Row = Vec<Seg>;

/// Flow a document: text from `text_col` to `limit`, headings at
/// `heading_col`.
pub(super) fn flow(
    ctx: &Ctx,
    doc: &Document,
    text_col: usize,
    limit: usize,
    heading_col: usize,
) -> Vec<Row> {
    let mut f = Flow {
        ctx,
        limit,
        heading_col,
        rows: Vec::new(),
        depth: 0,
    };
    f.blocks(doc, text_col);
    f.rows
}

/// Rows onto the screen from `top`.
pub(super) fn put_rows(s: &mut Screen, rows: &[Row], top: usize) {
    for (i, row) in rows.iter().enumerate() {
        for seg in row {
            s.put(top + i, seg.col, seg.attr, &seg.text);
            if let Some(module) = &seg.link {
                let len = seg.text.trim_end().chars().count().max(1);
                s.links.push(super::screen::HelpArea {
                    row: top + i,
                    col: seg.col,
                    end: seg.col + len - 1,
                    module: module.clone(),
                });
            }
        }
    }
}

/// An information (paged, with the indicator) or text (cut) area.
pub(super) fn render_area(
    ctx: &Ctx,
    doc: &Document,
    s: &mut Screen,
    top: usize,
    height: usize,
    scrolls: bool,
) {
    let rows = flow(ctx, doc, 2, s.limit(), 2);
    if !scrolls || rows.len() <= height {
        put_rows(s, &rows[..rows.len().min(height)], top);
        return;
    }
    let per = height.saturating_sub(1).max(1);
    let pages = rows.len().div_ceil(per);
    let page = ctx.opts.page.min(pages - 1);
    let start = page * per;
    let end = (start + per).min(rows.len());
    put_rows(s, &rows[start..end], top);
    super::indicator(s, top + height - 1, page + 1 == pages);
}

struct Flow<'a> {
    ctx: &'a Ctx<'a>,
    limit: usize,
    heading_col: usize,
    rows: Vec<Row>,
    depth: usize,
}

impl Flow<'_> {
    fn gap(&mut self) {
        if !self.rows.is_empty() {
            self.rows.push(Vec::new());
        }
    }

    fn blocks(&mut self, doc: &Document, col: usize) {
        for b in doc {
            self.block(b, col);
        }
    }

    fn block(&mut self, b: &Block, col: usize) {
        match b {
            Block::Heading { text, .. } => {
                self.gap();
                let t = self.ctx.text(text);
                self.rows.push(vec![Seg {
                    col: self.heading_col,
                    attr: Attr::HighIntensity,
                    text: t.trim().to_string(),
                    link: None,
                }]);
            }
            Block::Para(spans) => {
                self.gap();
                self.para(spans, col);
            }
            Block::ParaCont(spans) => self.para(spans, col),
            Block::Example(lines) | Block::Lines(lines) | Block::Figure(lines) => {
                self.gap();
                for l in lines {
                    let t = self.ctx.text(l);
                    let cut: String = t.chars().take(self.limit + 1 - col).collect();
                    self.rows.push(if cut.trim().is_empty() {
                        Vec::new()
                    } else {
                        vec![Seg {
                            col,
                            attr: Attr::Normal,
                            text: cut.trim_end().to_string(),
                            link: None,
                        }]
                    });
                }
            }
            Block::List { .. } | Block::DefList { .. } | Block::ParmList(_) | Block::Note(_) => {
                self.listing(b, col);
            }
            Block::ImHelp(name) => {
                if self.depth < 8
                    && let Some(h) = self.ctx.group.help(name)
                {
                    self.depth += 1;
                    self.blocks(&h.body, col);
                    self.depth -= 1;
                }
            }
        }
    }

    /// Lists, definition and parameter lists and notes.
    fn listing(&mut self, b: &Block, col: usize) {
        match b {
            Block::List {
                kind,
                compact,
                items,
            } => {
                self.gap();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 && !compact {
                        self.rows.push(Vec::new());
                    }
                    let marker = match kind {
                        ListKind::Unordered => "o".to_string(),
                        ListKind::Ordered => format!("{}.", i + 1),
                        ListKind::Simple => String::new(),
                    };
                    let inner = if marker.is_empty() {
                        col
                    } else {
                        col + marker.len() + 1
                    };
                    self.labelled(&marker, Attr::Normal, col, item, inner);
                }
            }
            Block::DefList {
                compact,
                heads,
                items,
            } => {
                self.gap();
                if let Some((t, d)) = heads {
                    let (t, d) = (self.ctx.text(t), self.ctx.text(d));
                    self.rows.push(vec![
                        Seg {
                            col,
                            attr: Attr::HighIntensity,
                            text: t.trim().to_string(),
                            link: None,
                        },
                        Seg {
                            col: col + 10,
                            attr: Attr::HighIntensity,
                            text: d.trim().to_string(),
                            link: None,
                        },
                    ]);
                }
                for (i, item) in items.iter().enumerate() {
                    if i > 0 && !compact {
                        self.rows.push(Vec::new());
                    }
                    let term = self.ctx.text(&item.term);
                    self.labelled(term.trim(), Attr::HighIntensity, col, &item.def, col + 10);
                }
            }
            Block::ParmList(items) => {
                self.gap();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.rows.push(Vec::new());
                    }
                    let term = self.ctx.text(&item.term);
                    self.labelled(term.trim(), Attr::HighIntensity, col, &item.def, col + 10);
                }
            }
            Block::Note(doc) => {
                self.gap();
                self.labelled("Note:", Attr::HighIntensity, col, doc, col + 6);
            }
            _ => {}
        }
    }

    /// A label at `col` with a document at `inner`; the first row of
    /// the document shares the label's row when the label fits before
    /// it.
    fn labelled(&mut self, label: &str, attr: Attr, col: usize, doc: &Document, inner: usize) {
        let start = self.rows.len();
        self.rows.push(Vec::new());
        let before = self.rows.len();
        self.blocks(doc, inner);
        // Drop the gap the first block adds after our marker row.
        if self.rows.len() > before && self.rows[before].is_empty() {
            self.rows.remove(before);
        }
        let seg = Seg {
            col,
            attr,
            text: label.to_string(),
            link: None,
        };
        let fits = col + label.chars().count() < inner;
        if label.is_empty() {
            self.rows.remove(start);
        } else if fits && self.rows.len() > start + 1 {
            self.rows.remove(start);
            self.rows[start].insert(0, seg);
        } else {
            self.rows[start].push(seg);
        }
    }

    /// A paragraph wrapped at the width, highlights kept per segment.
    fn para(&mut self, spans: &[Span], col: usize) {
        let mut chars: Vec<Ch> = Vec::new();
        let mut links: Vec<String> = Vec::new();
        self.chars(spans, (Attr::Normal, 0), &mut chars, &mut links);
        let width = self.limit + 1 - col;
        for line in wrap_attr(&chars, width) {
            self.rows.push(segments(&line, col, &links));
        }
    }

    fn chars(&self, spans: &[Span], (attr, link): (Attr, usize), out: &mut Vec<Ch>, links: &mut Vec<String>) {
        for s in spans {
            match s {
                Span::Text(t) => out.extend(t.chars().map(|c| (c, attr, link))),
                Span::Var(v) => {
                    let val = self.ctx.shown(v);
                    out.extend(val.trim_end().chars().map(|c| (c, attr, link)));
                }
                Span::Hp(n, inner) => self.chars(inner, (highlight(*n), link), out, links),
                Span::Pk(inner) => self.chars(inner, (Attr::HighIntensity, link), out, links),
                Span::Pv(inner) => self.chars(inner, (Attr::Underline, link), out, links),
                Span::Link { text, perform } => {
                    links.push(link_target(perform));
                    let id = links.len();
                    self.chars(text, (Attr::Underline, id), out, links);
                }
                Span::Rt(inner) => self.chars(inner, (Attr::Reverse, link), out, links),
            }
        }
    }
}

/// `:HPn.` as an attribute.
fn highlight(n: u8) -> Attr {
    match n {
        0 => Attr::Normal,
        1 | 5 => Attr::Underline,
        3 | 7 => Attr::Reverse,
        4 | 8 => Attr::Blue,
        _ => Attr::HighIntensity,
    }
}

/// Words packed into rows of at most `width` characters, blank runs
/// kept, each character with its attribute.
fn wrap_attr(chars: &[Ch], width: usize) -> Vec<Vec<Ch>> {
    let mut rows: Vec<Vec<Ch>> = Vec::new();
    let mut row: Vec<Ch> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let mut blanks = 0;
        while i < chars.len() && chars[i].0 == ' ' {
            blanks += 1;
            i += 1;
        }
        let start = i;
        while i < chars.len() && chars[i].0 != ' ' {
            i += 1;
        }
        let word = &chars[start..i];
        if word.is_empty() {
            break;
        }
        if row.is_empty() {
            row.extend_from_slice(word);
        } else if row.len() + blanks + word.len() <= width {
            let (attr, link) = row.last().map_or((Attr::Normal, 0), |c| (c.1, c.2));
            row.extend(std::iter::repeat_n((' ', attr, link), blanks));
            row.extend_from_slice(word);
        } else {
            rows.push(std::mem::take(&mut row));
            row.extend_from_slice(word);
        }
        if row.len() > width {
            row.truncate(width);
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

/// Consecutive same-attribute characters as segments from `col`; a
/// link's characters are a segment of their own.
fn segments(line: &[Ch], col: usize, links: &[String]) -> Row {
    let mut out: Row = Vec::new();
    let mut last_link = 0;
    for (i, (ch, attr, link)) in line.iter().enumerate() {
        match out.last_mut() {
            Some(seg) if seg.attr == *attr && last_link == *link => seg.text.push(*ch),
            _ => out.push(Seg {
                col: col + i,
                attr: *attr,
                text: ch.to_string(),
                link: link.checked_sub(1).and_then(|k| links.get(k).cloned()),
            }),
        }
        last_link = *link;
    }
    out
}

/// A character with its attribute and link (0 for none, else the
/// link's index plus one).
type Ch = (char, Attr, usize);

/// The help module a `:LINK PERFORM=` names: `DSPHELP module [group]`
/// gives `group/module`; any other value is taken as the module.
#[must_use]
pub fn link_target(perform: &str) -> String {
    let mut w = perform.split_whitespace();
    match w.next() {
        Some(verb) if verb.eq_ignore_ascii_case("DSPHELP") => {
            let module = w.next().unwrap_or_default().to_ascii_uppercase();
            match w.next() {
                Some(g) => format!("{}/{module}", g.to_ascii_uppercase()),
                None => module,
            }
        }
        Some(_) => perform.trim().to_ascii_uppercase(),
        None => String::new(),
    }
}
