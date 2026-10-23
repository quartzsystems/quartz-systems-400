// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The full-screen help panel (R20): `Help` and the centered title on
//! row 1, the text from row 3, the indicator, the fixed help keys.

use super::ctx::Ctx;
use super::dialog::{Dialog, ListState};
use super::screen::{Attr, Screen};
use super::{Options, RenderError, doc, layout};
use crate::model::PanelGroup;

/// Help has no dialog: variables show their declared values.
struct NoDialog;

impl Dialog for NoDialog {
    fn var(&self, _name: &str) -> Option<String> {
        None
    }

    fn list(&self, _name: &str) -> Option<ListState> {
        None
    }
}

/// The help keys, in order.
const KEYS: [&str; 5] = [
    "F3=Exit help",
    "F10=Move to top",
    "F12=Cancel",
    "F13=Information Assistant",
    "F14=Print help",
];

pub(super) fn render(
    group: &PanelGroup,
    name: &str,
    opts: &Options,
) -> Result<Screen, RenderError> {
    let h = group
        .help(name)
        .ok_or_else(|| RenderError::NoHelp(name.to_string()))?;
    let cols = match h.width {
        Some(w) if w > 80 && opts.wide => w as usize,
        _ => 80,
    };
    let rows = match h.depth {
        Some(d) if d > 24 && opts.wide => d as usize,
        _ => 24,
    };
    let ctx = Ctx {
        group,
        dialog: &NoDialog,
        opts,
    };
    let mut s = Screen::new(rows, cols);
    s.put(1, 2, Attr::Blue, "Help");
    let title = ctx.text(&h.title);
    let title = title.trim();
    s.put(
        1,
        layout::center(cols, title.chars().count()),
        Attr::HighIntensity,
        title,
    );

    // Text rows 3 to rows-4, the indicator on rows-3, keys on rows-2 and rows-1.
    let text_rows = rows - 6;
    let body = doc::flow(&ctx, &h.body, 6, cols - 7, 2);
    let pages = body.len().div_ceil(text_rows).max(1);
    let page = opts.page.min(pages - 1);
    let start = page * text_rows;
    let end = (start + text_rows).min(body.len());
    doc::put_rows(&mut s, &body[start..end], 3);
    super::indicator(&mut s, rows - 3, page + 1 == pages);

    let widths: Vec<usize> = KEYS.iter().map(|k| k.len()).collect();
    let key_rows = layout::flow(&widths, 2, s.limit());
    let key_top = rows - key_rows.len();
    for (i, row) in key_rows.iter().enumerate() {
        for &(k, col) in row {
            s.put(key_top + i, col, Attr::Blue, KEYS[k]);
        }
    }
    if let Some((m, _)) = &opts.message {
        s.put(rows, 2, Attr::HighIntensity, m);
    }
    Ok(s)
}
