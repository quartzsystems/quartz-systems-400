// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The data area (R10, R14 to R17): labels with dot leaders to a
//! shared colon column, fields four columns past it, groups, compact
//! identification rows, extenders, selection fields and columns.

use super::ctx::Ctx;
use super::layout;
use super::screen::{Attr, Field, Screen, Target};
use crate::model::{
    DataArea, DataExtender, DataField, DataNode, DataSelect, Display, Layout, Separator, Usage,
};

/// One row of a column's content.
enum Row<'a> {
    Blank,
    Rule,
    /// Constant text or a group heading (with its colon) at a column.
    Text(usize, String),
    Field {
        label_col: usize,
        shift: usize,
        compact: bool,
        field: &'a DataField,
    },
    Select {
        label_col: usize,
        shift: usize,
        compact: bool,
        select: &'a DataSelect,
    },
}

/// A column of the area: its start, width and rows.
struct Column<'a> {
    start: usize,
    width: usize,
    rows: Vec<Row<'a>>,
}

pub(super) fn render(ctx: &Ctx, area: &DataArea, s: &mut Screen, top: usize, height: usize) {
    let limit = s.limit();
    let mut row = top;
    let title = ctx.text(&area.title);
    if !title.trim().is_empty() {
        s.put(row, 2, Attr::HighIntensity, &title);
        row += 1;
    }
    let columns = columns(ctx, area, limit);
    let body = top + height - row;
    let longest = columns.iter().map(|c| c.rows.len()).max().unwrap_or(0);
    // R10: pages only when the content overflows.
    let (per, overflow) = if longest > body {
        (body.saturating_sub(1), true)
    } else {
        (body, false)
    };
    let pages = if per == 0 {
        1
    } else {
        longest.div_ceil(per).max(1)
    };
    let page = ctx.opts.page.min(pages - 1);
    for c in &columns {
        let colon = colon_of(ctx, c);
        let end = (c.start + c.width - 1).min(limit);
        for (i, r) in c.rows.iter().skip(page * per).take(per).enumerate() {
            put_row(ctx, s, row + i, c.start, end, colon, r);
        }
    }
    if overflow {
        super::indicator(s, top + height - 1, page + 1 == pages);
    }
}

/// R14: the colon column from the longest label of the column.
fn colon_of(ctx: &Ctx, c: &Column) -> usize {
    let mut longest = c.start - 1;
    for r in &c.rows {
        let (col, spans) = match r {
            Row::Field {
                label_col,
                field,
                compact: false,
                ..
            } => (*label_col, &field.prompt),
            Row::Select {
                label_col,
                select,
                compact: false,
                ..
            } => (*label_col, &select.prompt),
            _ => continue,
        };
        let len = ctx.text(spans).trim_end().chars().count();
        if len > 0 {
            longest = longest.max(col + len - 1);
        }
    }
    layout::colon_col(longest)
}

fn put_row(ctx: &Ctx, s: &mut Screen, row: usize, start: usize, end: usize, colon: usize, r: &Row) {
    match r {
        Row::Blank => {}
        Row::Rule => s.put(row, start, Attr::Normal, &"-".repeat(end + 1 - start)),
        Row::Text(col, text) => put_cut(s, row, *col, end, Attr::Normal, text),
        Row::Field {
            label_col,
            shift,
            compact,
            field,
        } => {
            let label = ctx.text(&field.prompt);
            let value_col =
                label_and_colon(s, row, *label_col, end, colon, *shift, *compact, &label);
            let width = field_width(ctx, &field.var, field.maxwidth, field.minwidth);
            let mut col = value_col;
            put_field(
                ctx,
                s,
                row,
                col,
                end,
                width,
                &field.var,
                field.usage,
                field.required,
                field.autoskip,
                field.csrloc,
            );
            col += width;
            let gap = if *compact { 5 } else { 3 };
            for x in &field.extenders {
                col = extender(ctx, s, row, col + gap, end, x);
            }
        }
        Row::Select {
            label_col,
            shift,
            compact,
            select,
        } => {
            let label = ctx.text(&select.prompt);
            let value_col =
                label_and_colon(s, row, *label_col, end, colon, *shift, *compact, &label);
            let choices: Vec<String> = select
                .choices
                .iter()
                .filter(|c| c.display == Display::Yes && ctx.cond(c.cond.as_deref()))
                .enumerate()
                .map(|(i, c)| format!("{}={}", i + 1, ctx.text(&c.text).trim()))
                .collect();
            let width = if choices.len() > 9 { 2 } else { 1 };
            let value = ctx.value(&select.var);
            let shown = select
                .choices
                .iter()
                .position(|c| c.value == value.trim_end())
                .map(|i| (i + 1).to_string())
                .unwrap_or_default();
            if select.usage.is_input() {
                let f = Field {
                    row,
                    col: value_col,
                    len: width,
                    attr: Attr::Underline,
                    value: shown,
                    target: Target::Var(select.var.clone()),
                    usage: select.usage,
                    shift: crate::model::Shift::None,
                    required: select.required,
                    autoskip: false,
                };
                if select.csrloc {
                    s.cursor = Some((row, value_col));
                }
                s.field(f);
            } else {
                put_cut(s, row, value_col, end, Attr::Normal, &shown);
            }
            let first = value_col + width + 3;
            let widths: Vec<usize> = choices.iter().map(|c| c.chars().count()).collect();
            for (i, r) in layout::flow(&widths, first, end).into_iter().enumerate() {
                for (k, col) in r {
                    put_cut(s, row + i, col, end, Attr::Normal, &choices[k]);
                }
            }
        }
    }
}

/// The label, its leaders and colon; returns the value column.
#[allow(clippy::too_many_arguments)]
fn label_and_colon(
    s: &mut Screen,
    row: usize,
    label_col: usize,
    end: usize,
    colon: usize,
    shift: usize,
    compact: bool,
    label: &str,
) -> usize {
    let label = label.trim_end();
    let len = label.chars().count();
    if compact {
        // R16: `Label:   value`.
        let text = format!("{label}:");
        put_cut(s, row, label_col, end, Attr::Normal, &text);
        label_col + len + 4
    } else {
        let label_end = label_col + len - 1;
        let text = format!("{label}{}", layout::leaders(label_end, colon));
        put_cut(s, row, label_col, end, Attr::Normal, &text);
        colon + 4 + shift
    }
}

/// An extender after `col`: its text with a colon, then its field;
/// returns the column after the field.
fn extender(
    ctx: &Ctx,
    s: &mut Screen,
    row: usize,
    col: usize,
    end: usize,
    x: &DataExtender,
) -> usize {
    if x.display == Display::No {
        return col;
    }
    let mut col = col;
    let text = ctx.text(&x.text);
    let text = text.trim();
    if !text.is_empty() {
        put_cut(s, row, col, end, Attr::Normal, &format!("{text}:"));
        col += text.chars().count() + 4;
    }
    let width = field_width(ctx, &x.var, x.maxwidth, x.minwidth);
    put_field(
        ctx, s, row, col, end, width, &x.var, x.usage, false, false, false,
    );
    col + width
}

fn field_width(ctx: &Ctx, var: &str, max: Option<u32>, min: Option<u32>) -> usize {
    let mut w = max.map_or_else(|| ctx.width_of(var), |m| m as usize);
    if let Some(m) = min {
        w = w.max(m as usize);
    }
    w.max(1)
}

/// A field or value at `col`, cut at `end`.
#[allow(clippy::too_many_arguments)]
fn put_field(
    ctx: &Ctx,
    s: &mut Screen,
    row: usize,
    col: usize,
    end: usize,
    width: usize,
    var: &str,
    usage: Usage,
    required: bool,
    autoskip: bool,
    csrloc: bool,
) {
    if col > end {
        return;
    }
    let len = width.min(end + 1 - col);
    if usage.is_input() {
        if csrloc {
            s.cursor = Some((row, col));
        }
        s.field(Field {
            row,
            col,
            len,
            attr: Attr::Underline,
            value: ctx.value(var),
            target: Target::Var(var.to_string()),
            usage,
            shift: ctx.class_of(var).map(|c| c.shift).unwrap_or_default(),
            required,
            autoskip,
        });
    } else {
        let text = ctx.shown(var);
        if ctx.numeric(var) {
            let t = text.trim();
            let c = col + len.saturating_sub(t.chars().count());
            put_cut(s, row, c, col + len - 1, Attr::Normal, t);
        } else {
            put_cut(s, row, col, col + len - 1, Attr::Normal, text.trim_end());
        }
    }
}

fn put_cut(s: &mut Screen, row: usize, col: usize, end: usize, attr: Attr, text: &str) {
    if col > end {
        return;
    }
    let cut: String = text.chars().take(end + 1 - col).collect();
    s.put(row, col, attr, &cut);
}

/// R17: the columns, from `:DATACOL.` or an even split of `LAYOUT`.
fn columns<'a>(ctx: &Ctx, area: &'a DataArea, limit: usize) -> Vec<Column<'a>> {
    let has_datacol = area.items.iter().any(|i| matches!(i, DataNode::Column(_)));
    let mut out: Vec<Column<'a>> = Vec::new();
    if has_datacol {
        let mut start = 2;
        let mut current: Option<Column<'a>> = None;
        for item in &area.items {
            if let DataNode::Column(w) = item {
                if let Some(c) = current.take() {
                    start += c.width;
                    out.push(c);
                }
                current = Some(Column {
                    start,
                    width: *w as usize,
                    rows: Vec::new(),
                });
            } else {
                let c = current.get_or_insert_with(|| Column {
                    start,
                    width: limit + 1 - start,
                    rows: Vec::new(),
                });
                walk(ctx, item, c.start, 0, area.compact, &mut c.rows);
            }
        }
        if let Some(c) = current {
            out.push(c);
        }
    } else {
        let n = match area.layout {
            Layout::One => 1,
            Layout::Two => 2,
            Layout::Three => 3,
        };
        let width = (limit - 1) / n;
        let per = area.items.len().div_ceil(n).max(1);
        for (k, chunk) in area.items.chunks(per).enumerate() {
            let start = 2 + k * width;
            let mut rows = Vec::new();
            for item in chunk {
                walk(ctx, item, start, 0, area.compact, &mut rows);
            }
            out.push(Column { start, width, rows });
        }
    }
    out
}

/// R15: items into rows; groups indent their items, headingless
/// groups shift the values too.
fn walk<'a>(
    ctx: &Ctx,
    item: &'a DataNode,
    label_col: usize,
    shift: usize,
    compact: bool,
    rows: &mut Vec<Row<'a>>,
) {
    match item {
        DataNode::Column(_) => {}
        DataNode::Field(f) => {
            if f.display == Display::Yes {
                rows.push(Row::Field {
                    label_col,
                    shift,
                    compact,
                    field: f,
                });
            }
        }
        DataNode::Const(c) => {
            if c.display == Display::Yes {
                rows.push(Row::Text(label_col, ctx.text(&c.text)));
            }
        }
        DataNode::Select(sel) => {
            if sel.display == Display::Yes {
                rows.push(Row::Select {
                    label_col,
                    shift,
                    compact,
                    select: sel,
                });
            }
        }
        DataNode::Group(g) => {
            if !rows.is_empty() {
                match g.grpsep {
                    Separator::Space | Separator::SysName | Separator::DateTime => rows.push(Row::Blank),
                    Separator::Rule => rows.push(Row::Rule),
                    Separator::None => {}
                }
            }
            let title = ctx.text(&g.title);
            let heading = !title.trim().is_empty();
            if heading {
                rows.push(Row::Text(label_col, format!("{}:", title.trim_end())));
            }
            // A heading indents the labels; no heading indents the
            // values too; a compact group is an identification block
            // and indents nothing.
            let compact = compact || g.compact;
            let (col, inner_shift) = match (compact, heading) {
                (true, _) => (label_col, shift),
                (false, true) => (label_col + 2, shift),
                (false, false) => (label_col + 2, shift + 2),
            };
            for i in &g.items {
                walk(ctx, i, col, inner_shift, compact, rows);
            }
        }
    }
}
