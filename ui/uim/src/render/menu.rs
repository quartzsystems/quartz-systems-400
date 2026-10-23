// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The menu area (R10, R12, R13): the heading, the items with their
//! option numbers, group headings, the gaps and the right column.

use super::ctx::Ctx;
use super::screen::{Attr, Screen};
use crate::model::{Area, MenuArea, MenuItem, MenuNode, MenuOption, Panel};

enum Row {
    Blank,
    Heading(String),
    Item(u16, String, Option<String>),
}

/// The width of the selection field: the digits of the largest option.
pub(super) fn selection_width(p: &Panel) -> usize {
    let mut widest = 1;
    for a in &p.areas {
        if let Area::Menu(m) = a {
            for i in m.all_items() {
                if let MenuOption::Number(n) = i.option {
                    widest = widest.max(n.to_string().len());
                }
            }
        }
    }
    widest
}

pub(super) fn render(ctx: &Ctx, area: &MenuArea, s: &mut Screen, top: usize, height: usize) {
    let limit = s.limit();
    let title = ctx.text(&area.title);
    s.put(top, 2, Attr::Blue, title.trim_end());
    let rows = rows(ctx, area);
    let body = height.saturating_sub(2);
    let (per, overflow) = if rows.len() > body {
        (body.saturating_sub(1), true)
    } else {
        (body, false)
    };
    let pages = if per == 0 {
        1
    } else {
        rows.len().div_ceil(per).max(1)
    };
    let page = ctx.opts.page.min(pages - 1);
    let first = top + 2;
    for (i, r) in rows.iter().skip(page * per).take(per).enumerate() {
        let row = first + i;
        match r {
            Row::Blank => {}
            Row::Heading(h) => s.put(row, 4, Attr::Normal, h),
            Row::Item(n, text, right) => {
                s.put(row, 5, Attr::Normal, &format!("{n:>3}."));
                let end = right.as_ref().map_or(limit, |_| limit - 11);
                let cut: String = text.chars().take(end + 1 - 10).collect();
                s.put(row, 10, Attr::Normal, cut.trim_end());
                if let Some(r) = right {
                    s.put(row, limit - 9, Attr::Normal, r);
                }
            }
        }
    }
    if overflow {
        super::indicator(s, top + height - 1, page + 1 == pages);
    }
}

/// The rows: a blank one where option numbers skip and before a group
/// heading that does not start the area.
fn rows(ctx: &Ctx, area: &MenuArea) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let mut prev: Option<u16> = None;
    for node in &area.items {
        match node {
            MenuNode::Item(i) => push_item(ctx, &mut out, &mut prev, i),
            MenuNode::Group { heading, items, .. } => {
                if !out.is_empty() {
                    out.push(Row::Blank);
                }
                let h = ctx.text(heading);
                if !h.trim().is_empty() {
                    out.push(Row::Heading(h.trim_end().to_string()));
                }
                prev = None;
                for i in items {
                    push_item(ctx, &mut out, &mut prev, i);
                }
            }
        }
    }
    out
}

/// One item's row, after a blank one when its number skips.
fn push_item(ctx: &Ctx, out: &mut Vec<Row>, prev: &mut Option<u16>, item: &MenuItem) {
    let MenuOption::Number(n) = item.option else {
        return;
    };
    if !ctx.cond(item.cond.as_deref()) {
        return;
    }
    if let Some(p) = *prev
        && n != p.wrapping_add(1)
    {
        out.push(Row::Blank);
    }
    let (text, right) = split_right(&ctx.text(&item.text));
    out.push(Row::Item(n, text, right));
    *prev = Some(n);
}

/// R13: a trailing upper-case name after lower-case text goes to the
/// right column.
fn split_right(text: &str) -> (String, Option<String>) {
    let t = text.trim_end();
    let Some((head, last)) = t.rsplit_once(' ') else {
        return (t.to_string(), None);
    };
    let is_name = (2..=10).contains(&last.len())
        && last.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && last
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "@#$_".contains(c));
    let prev_word = head.rsplit(' ').next().unwrap_or("");
    if is_name && prev_word.chars().any(|c| c.is_ascii_lowercase()) {
        (head.trim_end().to_string(), Some(last.to_string()))
    } else {
        (t.to_string(), None)
    }
}

#[cfg(test)]
mod tests {
    use super::split_right;

    #[test]
    fn right_column() {
        assert_eq!(
            split_right("Select a command by name SLTCMD"),
            ("Select a command by name".into(), Some("SLTCMD".into()))
        );
        assert_eq!(split_right("User tasks"), ("User tasks".into(), None));
        assert_eq!(split_right("Start TCP/IP"), ("Start TCP/IP".into(), None));
        assert_eq!(split_right("Sign off"), ("Sign off".into(), None));
    }
}
