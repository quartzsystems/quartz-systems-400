// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list area (R5, R8, R9, R10): option legends, a blank row, the
//! column headings, the entries and the paging indicator.

use super::ctx::Ctx;
use super::layout;
use super::screen::{Attr, Field, Screen, Target};
use crate::model::{BaseType, Display, ListArea, ListColNode, ListColumn, PanelGroup, Shift};

/// A column as laid out.
struct Col<'a> {
    column: &'a ListColumn,
    /// The `:LISTGRP.` heading over it, with the group's index.
    group: Option<(usize, String)>,
    /// The field's width.
    field: usize,
    /// The column's width: the field or the widest heading row.
    width: usize,
    /// The heading rows.
    heading: Vec<String>,
    /// The start column.
    start: usize,
    numeric: bool,
    option: bool,
}

#[allow(clippy::too_many_lines)] // legends, headings, entries, indicator
pub(super) fn render(ctx: &Ctx, area: &ListArea, s: &mut Screen, top: usize, height: usize) {
    let limit = s.limit();
    let last = top + height - 1;
    let mut row = top;

    // F1 anywhere in the area: the list's help, else the panel's.
    for r in top..=last {
        s.help(r, 2, limit, area.help.as_deref());
    }

    // R5: the legends.
    let legends: Vec<(String, Option<&str>)> = area
        .actions
        .iter()
        .filter(|a| a.option.is_some())
        .map(|a| (ctx.text(&a.text), a.help.as_deref().or(area.help.as_deref())))
        .filter(|(t, _)| !t.trim().is_empty())
        .collect();
    if !legends.is_empty() {
        let widths: Vec<usize> = legends.iter().map(|(t, _)| t.chars().count()).collect();
        for r in layout::flow(&widths, 4, limit) {
            for (i, col) in r {
                let (text, help) = &legends[i];
                s.put(row, col, Attr::Blue, text);
                s.help(row, col, col + widths[i] - 1, *help);
            }
            row += 1;
        }
        if !area.compact {
            row += 1;
        }
    }

    // R9: the columns of the view.
    let cols = columns(ctx, area, limit);
    let grouped = cols.iter().any(|c| c.group.is_some());
    // Without `MAXHEAD` a group's heading takes the first of the rows
    // the other columns' wrapped headings need.
    let head_rows = area.maxhead.map_or_else(
        || {
            cols.iter()
                .map(|c| c.heading.len() + usize::from(c.group.is_some()))
                .max()
                .unwrap_or(0)
        },
        |m| usize::from(grouped) + m as usize,
    );
    let shared = if area.maxhead.is_some() { Some(grouped) } else { None };
    headings(s, &cols, row, head_rows, shared, limit);
    let entries_end = last.saturating_sub(1);
    for c in &cols {
        let end = (c.start + c.width - 1).min(limit);
        let help = c.column.help.as_deref().or(area.help.as_deref());
        for r in row..=entries_end.max(row) {
            s.help(r, c.start, end, help);
        }
    }
    row += head_rows + usize::from(area.headsep);

    // The entries.
    let entries = last.saturating_sub(row);
    let state = ctx.list(&area.listdef);
    for (i, entry) in state.rows.iter().take(entries).enumerate() {
        let r = row + i;
        let index = state.top + i;
        for c in &cols {
            let raw = entry
                .iter()
                .find(|(v, _)| *v == c.column.var)
                .map(|(_, val)| val.clone())
                .unwrap_or_default();
            let end = (c.start + c.width - 1).min(limit);
            if c.option {
                let col = c.start + (c.width - c.field) / 2;
                let typed = ctx.dialog.option(&area.listdef, index);
                let mut f = field(
                    r,
                    col,
                    c.field,
                    typed.unwrap_or(raw),
                    Target::ListOption {
                        list: area.listdef.clone(),
                        row_index: index,
                    },
                    c.column,
                );
                // An option that is not valid shows in reverse image.
                if ctx.dialog.option_error(&area.listdef, index) {
                    f.attr = Attr::Reverse;
                }
                s.field(f);
            } else if c.column.usage.is_input() {
                // An entry field narrower than its heading is centered
                // under it (edtobjaut's X fields); a number sits at the
                // right of its field (edtlibl's sequence numbers).
                let col = c.start + (c.width - c.field) / 2;
                let raw = if c.numeric {
                    format!("{:>w$}", raw.trim(), w = c.field)
                } else {
                    raw
                };
                s.field(field(
                    r,
                    col,
                    c.field.min(end + 1 - col),
                    raw,
                    Target::ListCell {
                        list: area.listdef.clone(),
                        row_index: index,
                        var: c.column.var.clone(),
                    },
                    c.column,
                ));
            } else {
                let text = ctx.translate(&c.column.var, &raw);
                if c.numeric {
                    let t = text.trim();
                    let col = c.start + c.field.saturating_sub(t.chars().count());
                    put_cut(s, r, col, end, Attr::Normal, t);
                } else {
                    put_cut(s, r, c.start, end, Attr::Normal, text.trim_end());
                }
            }
        }
    }

    // An empty list shows its `:LIST.` text on the second entry row; a
    // list holding only the blank row option 1 types on, on the row
    // after the next.
    let blank = |r: &Vec<(String, String)>| r.iter().all(|(_, v)| v.trim().is_empty());
    let placeholders = state.rows.len();
    if state.complete && state.rows.iter().all(blank) && placeholders <= 1 && entries > placeholders + 1 {
        let empty = ctx.text(&area.title);
        put_cut(s, row + placeholders + 1, 4, limit, Attr::Normal, empty.trim());
    }

    // R10: lists always say where they stand.
    let shown = state.rows.len().min(entries);
    s.list_shown.insert(area.listdef.clone(), entries);
    let more = state.top + entries < state.count() || (!state.complete && shown >= entries);
    super::indicator(s, last, !more);
}

/// R9: the heading rows from `head_top`, a group's heading over its
/// columns on the first.
fn headings(
    s: &mut Screen,
    cols: &[Col],
    head_top: usize,
    head_rows: usize,
    shared: Option<bool>,
    limit: usize,
) {
    let mut seen_group = None;
    for c in cols {
        let end = c.start + c.width - 1;
        if let Some((g, text)) = &c.group
            && seen_group != Some(*g)
        {
            seen_group = Some(*g);
            // A group of numbers ends its heading where its last field ends.
            let members: Vec<&Col> = cols
                .iter()
                .filter(|k| k.group.as_ref().is_some_and(|(x, _)| x == g))
                .collect();
            let len = text.chars().count();
            let col = match members.last() {
                Some(last)
                    if members
                        .iter()
                        .all(|k| k.numeric && !k.column.usage.is_input()) =>
                {
                    (last.start + last.field).saturating_sub(len).max(c.start)
                }
                _ => c.start,
            };
            put_cut(s, head_top, col, limit, Attr::HighIntensity, text);
        }
        // `MAXHEAD`: every column has the rows under the group's.
        let above = shared.unwrap_or(c.group.is_some());
        let col_rows = head_rows - usize::from(above);
        let skip = c.heading.len().saturating_sub(col_rows);
        for (j, h) in c.heading.iter().skip(skip).enumerate() {
            let r = head_top + head_rows - (c.heading.len() - skip) + j;
            // A number's heading rows end where its field ends.
            let len = h.chars().count();
            // A column wider than its field centers its rows (R9) first.
            let col = if c.width > c.field && !c.option {
                heading_col(c, h)
            } else if c.numeric && !c.column.usage.is_input() && len <= c.field {
                c.start + c.field - len
            } else {
                c.start
            };
            put_cut(s, r, col, end.min(limit), Attr::HighIntensity, h);
        }
    }
}

/// R9: where a heading row starts: centered when the heading is wider
/// than its field (dspsfwrsc `ID`, dspautusr `No`), else at the start.
fn heading_col(c: &Col, h: &str) -> usize {
    if c.width > c.field && !c.option {
        c.start + (c.width - h.chars().count()) / 2
    } else {
        c.start
    }
}

/// Text cut at `end`.
fn put_cut(s: &mut Screen, row: usize, col: usize, end: usize, attr: Attr, text: &str) {
    if col > end {
        return;
    }
    let cut: String = text.chars().take(end + 1 - col).collect();
    s.put(row, col, attr, &cut);
}

fn field(
    row: usize,
    col: usize,
    len: usize,
    value: String,
    target: Target,
    c: &ListColumn,
) -> Field {
    Field {
        row,
        col,
        len,
        attr: Attr::Underline,
        value,
        target,
        usage: c.usage,
        shift: Shift::None,
        required: false,
        autoskip: c.autoskip,
    }
}

/// The view's columns with their widths and start columns; columns
/// that would start past `limit` are dropped.
fn columns<'a>(ctx: &Ctx, area: &'a ListArea, limit: usize) -> Vec<Col<'a>> {
    let mut picked: Vec<(&'a ListColumn, Option<(usize, String)>)> = Vec::new();
    let view = area
        .list_views
        .get(ctx.opts.view)
        .or_else(|| area.list_views.first());
    let groups: Vec<(usize, &ListColNode)> = area.columns.iter().enumerate().collect();
    match view {
        Some(v) => {
            for name in &v.cols {
                for (gi, node) in &groups {
                    match node {
                        ListColNode::Column(c) if c.name == *name => picked.push((c, None)),
                        ListColNode::Group(g) if g.col.as_deref() == Some(name.as_str()) => {
                            let text = ctx.text(&g.heading);
                            picked.extend(g.columns.iter().map(|c| (c, Some((*gi, text.clone())))));
                        }
                        _ => {}
                    }
                }
            }
        }
        None => {
            for (gi, node) in &groups {
                match node {
                    ListColNode::Column(c) => picked.push((c, None)),
                    ListColNode::Group(g) => {
                        let text = ctx.text(&g.heading);
                        picked.extend(g.columns.iter().map(|c| (c, Some((*gi, text.clone())))));
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut start = 2;
    for (c, group) in picked {
        if c.display == Display::No {
            continue;
        }
        let option = ctx
            .class_of(&c.var)
            .is_some_and(|k| k.basetype == BaseType::Action);
        let mut field = c
            .maxwidth
            .map_or_else(|| ctx.width_of(&c.var), |w| w as usize);
        if option && let Some(a) = area.actlen {
            field = a as usize;
        }
        if let Some(m) = c.minwidth {
            field = field.max(m as usize);
        }
        let text = ctx.text(&c.heading);
        // Blanks only: a non-breaking blank at an end holds a heading row
        // off its edge.
        let heading = layout::wrap(text.trim_matches(' '), field);
        let width = heading
            .iter()
            .map(|h| h.chars().count())
            .max()
            .unwrap_or(0)
            .max(field);
        if start > limit {
            break;
        }
        out.push(Col {
            column: c,
            group,
            field,
            width,
            heading,
            start,
            numeric: ctx.numeric(&c.var),
            option,
        });
        start += width + 2;
    }
    out
}

/// The position-to variable of a list: the first variable of the
/// `PARMS` variable record (the others narrow the search).
#[must_use]
pub fn position_var(group: &PanelGroup, area: &ListArea) -> Option<String> {
    let rec = area.parms.as_deref()?;
    group
        .var_records
        .iter()
        .find(|r| r.name == rec)
        .and_then(|r| r.vars.first().cloned())
}

/// The columns a position-to record's variables go with, in order: the
/// list's columns without the option column.
#[must_use]
pub fn position_columns(group: &PanelGroup, area: &ListArea) -> Vec<String> {
    area.all_columns()
        .into_iter()
        .filter(|c| {
            group
                .variable(&c.var)
                .and_then(|v| group.class(&v.class))
                .is_none_or(|k| k.basetype != BaseType::Action)
        })
        .map(|c| c.var.clone())
        .collect()
}
