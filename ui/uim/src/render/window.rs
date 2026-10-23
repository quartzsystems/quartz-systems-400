// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Windows (R24): a pop-up panel or a help window drawn over the
//! screen under it. The screen under keeps its texts outside the box
//! and shows its fields as text; the box has a row of dots above, colons
//! at its sides and a row of dots between colons below
//! (uim-position-window).

use super::screen::{Attr, Screen, Text};
use super::{Dialog, Options, RenderError};
use crate::model::{Panel, PanelGroup};

/// Whether a panel is a pop-up: narrower or shorter than a screen.
#[must_use]
pub fn is_window(p: &Panel) -> bool {
    !matches!((p.depth, p.width), (24 | 27, 80 | 132))
}

/// The screen under a window: `under`'s texts and field values with a
/// bordered box of `rows` by `cols` inside its borders, the top border
/// on row `top` and the left one at column `left`.
#[must_use]
pub fn frame(under: &Screen, top: usize, left: usize, rows: usize, cols: usize) -> Screen {
    let mut s = Screen::new(under.rows, under.cols);
    let bottom = top + rows + 1;
    let right = left + cols + 1;
    let mut texts: Vec<Text> = under.texts.clone();
    for f in &under.fields {
        if f.attr == Attr::Nondisplay || f.value.trim().is_empty() {
            continue;
        }
        texts.push(Text {
            row: f.row,
            col: f.col,
            attr: Attr::Normal,
            text: f.value.chars().take(f.len).collect(),
        });
    }
    for t in texts {
        if t.row < top || t.row > bottom {
            s.texts.push(t);
            continue;
        }
        // Keep what lies left and right of the box.
        let chars: Vec<char> = t.text.chars().collect();
        let end = t.col + chars.len();
        if t.col < left {
            let n = (left - t.col).min(chars.len());
            push(&mut s, t.row, t.col, t.attr, &chars[..n]);
        }
        if end > right + 1 {
            let from = (right + 1).max(t.col);
            push(&mut s, t.row, from, t.attr, &chars[from - t.col..]);
        }
    }
    let dots = ".".repeat(cols + 2);
    s.put(top, left, Attr::Blue, &dots);
    s.put(bottom, left, Attr::Blue, &format!(":{}:", ".".repeat(cols)));
    for r in top + 1..bottom {
        s.texts.push(Text {
            row: r,
            col: left,
            attr: Attr::Blue,
            text: ":".into(),
        });
        s.texts.push(Text {
            row: r,
            col: right,
            attr: Attr::Blue,
            text: ":".into(),
        });
    }
    s
}

fn push(s: &mut Screen, row: usize, col: usize, attr: Attr, chars: &[char]) {
    let text: String = chars.iter().collect();
    if !text.trim().is_empty() {
        s.texts.push(Text {
            row,
            col,
            attr,
            text,
        });
    }
}

/// Where a pop-up of `rows` by `cols` (inside its borders) goes: under
/// the cursor's row when it fits, else as low as it fits; its left
/// border at column 2 shifted right toward the cursor while it fits.
#[must_use]
pub fn place(
    screen_rows: usize,
    screen_cols: usize,
    rows: usize,
    cols: usize,
    cursor: (usize, usize),
) -> (usize, usize) {
    let height = rows + 2;
    let width = cols + 2;
    // The message line stays free.
    let last = screen_rows - 1;
    let top = if cursor.0 + height <= last {
        cursor.0 + 1
    } else {
        (last + 1).saturating_sub(height).max(1)
    };
    let max_left = screen_cols.saturating_sub(width).max(1);
    let left = cursor.1.saturating_sub(1).clamp(1, max_left);
    (top, left)
}

/// R24: where a pop-up panel's box goes: its top border on row 2, under
/// the title, and centered across the screen (uim-position-window: a
/// 74-column panel's borders at columns 2 and 79).
#[must_use]
pub fn place_popup(screen_cols: usize, cols: usize) -> (usize, usize) {
    (2, 1 + screen_cols.saturating_sub(cols + 2) / 2)
}

/// Render a pop-up panel over `under`: the panel laid out at its own
/// size, then moved into the box.
pub fn render_window(
    group: &PanelGroup,
    panel: &str,
    dialog: &dyn Dialog,
    opts: &Options,
    under: &Screen,
    cursor: (usize, usize),
) -> Result<Screen, RenderError> {
    let p = group
        .panel(panel)
        .ok_or_else(|| RenderError::NoPanel(panel.to_string()))?;
    let inner = super::render_sized(group, p, dialog, opts)?;
    // The box holds the panel and an attribute column at each side.
    let (top, left) = place_popup(under.cols, inner.cols + 2);
    let _ = cursor;
    let mut s = frame(under, top, left, inner.rows, inner.cols + 2);
    let left = left + 1;
    // Panel row r and column c go to row top + r and column left + c.
    for t in inner.texts {
        if t.row == inner.rows && opts.message.is_some() {
            continue;
        }
        s.texts.push(Text {
            row: top + t.row,
            col: left + t.col,
            ..t
        });
    }
    for mut f in inner.fields {
        f.row += top;
        f.col += left;
        s.fields.push(f);
    }
    for mut h in inner.helps {
        h.row += top;
        h.col += left;
        h.end += left;
        s.helps.push(h);
    }
    s.cursor = inner.cursor.map(|(r, c)| (r + top, c + left));
    s.more = inner.more.map(|m| super::More {
        row: m.row + top,
        col: m.col + left,
        bottom: m.bottom,
    });
    s.list_shown = inner.list_shown;
    if let Some((m, _)) = &opts.message {
        s.put(s.rows, 2, Attr::HighIntensity, m);
    }
    Ok(s)
}
