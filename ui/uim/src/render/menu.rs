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
    Item(u16, String, Option<String>, Option<String>),
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
    for r in top..top + height {
        s.help(r, 2, limit, area.help.as_deref());
    }
    let rows = rows(ctx, area);
    let body = height.saturating_sub(2);
    let (per, overflow) = if rows.len() > body {
        (body.saturating_sub(1), true)
    } else {
        (body, false)
    };
    let pages = paginate(&rows, per);
    let page = ctx.opts.page.min(pages.len() - 1);
    let first = top + 2;
    for (i, r) in pages[page].iter().enumerate() {
        let row = first + i;
        match r {
            Row::Blank => {}
            Row::Heading(h) => s.put(row, 4, Attr::Normal, h),
            Row::Item(n, text, right, help) => {
                s.help(row, 5, limit, help.as_deref());
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
        super::indicator(s, top + height - 1, page + 1 == pages.len());
    }
}

/// The rows in pages of `per`; a page does not start with a blank row.
fn paginate(rows: &[Row], per: usize) -> Vec<Vec<&Row>> {
    let mut pages: Vec<Vec<&Row>> = vec![Vec::new()];
    if per == 0 {
        return pages;
    }
    for r in rows {
        if pages.last().is_some_and(|p| p.len() >= per) {
            pages.push(Vec::new());
        }
        let page = pages.last_mut().expect("a page");
        if page.is_empty() && matches!(r, Row::Blank) {
            continue;
        }
        page.push(r);
    }
    if pages.len() > 1 && pages.last().is_some_and(Vec::is_empty) {
        pages.pop();
    }
    pages
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
    // A reserved option: no text, a blank row, the number kept.
    if text.trim().is_empty() {
        out.push(Row::Blank);
    } else {
        out.push(Row::Item(n, text, right, item.help.clone()));
    }
    *prev = Some(n);
}

/// Whether a word is an object name for the right column.
fn is_name(word: &str) -> bool {
    (2..=10).contains(&word.len())
        && word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && word
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "@#$_".contains(c))
}

/// R13: a trailing name after two blanks goes to the right column, as
/// does one of four characters or more after a single blank and
/// lower-case text (`Interactive SQL` stays whole).
fn split_right(text: &str) -> (String, Option<String>) {
    let t = text.trim_end();
    if let Some((head, last)) = t.rsplit_once("  ")
        && is_name(last)
        && !head.trim().is_empty()
    {
        return (head.trim_end().to_string(), Some(last.to_string()));
    }
    let Some((head, last)) = t.rsplit_once(' ') else {
        return (t.to_string(), None);
    };
    let prev_word = head.rsplit(' ').next().unwrap_or("");
    if is_name(last) && last.len() >= 4 && prev_word.chars().any(|c| c.is_ascii_lowercase()) {
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
        assert_eq!(split_right("Interactive SQL"), ("Interactive SQL".into(), None));
        assert_eq!(
            split_right("Change Job  CHGJOB"),
            ("Change Job".into(), Some("CHGJOB".into()))
        );
        assert_eq!(split_right("Do  DO"), ("Do".into(), Some("DO".into())));
    }
}
