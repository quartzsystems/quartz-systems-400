// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help displays (R20, R22, R23): a full-screen help panel with `Help`
//! and the centered title on row 1, the text from row 3, the
//! indicator, the help keys; or the same content in a window over the
//! panel. Extended help joins several modules, each after the first
//! under its title as a heading.

use super::ctx::Ctx;
use super::dialog::{Dialog, ListState};
use super::screen::{Attr, Screen};
use super::{Options, RenderError, doc, layout, window};
use crate::model::{Document, PanelGroup, Span};

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

/// The help keys of a full-screen help module (R20).
pub const FULL_KEYS: [&str; 5] = [
    "F3=Exit help",
    "F10=Move to top",
    "F12=Cancel",
    "F13=Information Assistant",
    "F14=Print help",
];

/// One module of a help display.
#[derive(Clone, Debug)]
pub struct HelpPart<'a> {
    /// The group the module's variables and imbeds come from.
    pub group: &'a PanelGroup,
    /// Its title.
    pub title: String,
    /// Its text.
    pub body: Document,
}

impl<'a> HelpPart<'a> {
    /// A module of a group.
    #[must_use]
    pub fn of(group: &'a PanelGroup, module: &str) -> Option<Self> {
        let h = group.help(module)?;
        Some(Self {
            group,
            title: Span::plain(&h.title).trim().to_string(),
            body: h.body.clone(),
        })
    }
}

/// Where a help window goes: its top border's row and left border's
/// column, and the most rows and the columns inside the borders; the
/// window is as tall as its text needs (`render_view`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelpWindow {
    /// The top border's row.
    pub top: usize,
    /// The left border's column.
    pub left: usize,
    /// The most rows inside the borders.
    pub rows: usize,
    /// Columns inside the borders, the attribute columns included.
    pub cols: usize,
}

/// What a help display shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpView {
    /// The title on the first row, as shown (see [`help_title`]).
    pub title: String,
    /// The key texts, in order.
    pub keys: Vec<String>,
    /// The page, from 0.
    pub page: usize,
    /// A window over the panel, or full screen.
    pub window: Option<HelpWindow>,
}

/// The title of a help display: the module's title with `- Help`, as
/// the goldens show it (`Options - Help`), unless it says so already.
#[must_use]
pub fn help_title(title: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        "Help".to_string()
    } else if t.to_ascii_lowercase().ends_with("help") {
        t.to_string()
    } else {
        format!("{t} - Help")
    }
}

/// Render one help module full screen with the R20 keys (the help
/// module as the first wave showed it).
pub(super) fn render(
    group: &PanelGroup,
    name: &str,
    opts: &Options,
) -> Result<Screen, RenderError> {
    let h = group
        .help(name)
        .ok_or_else(|| RenderError::NoHelp(name.to_string()))?;
    let part = HelpPart {
        group,
        title: Span::plain(&h.title).trim().to_string(),
        body: h.body.clone(),
    };
    let view = HelpView {
        title: part.title.clone(),
        keys: FULL_KEYS.iter().map(ToString::to_string).collect(),
        page: opts.page,
        window: None,
    };
    let (rows, cols) = match (h.depth, h.width) {
        (Some(d), Some(w)) if d > 24 && w > 80 && opts.wide => (d as usize, w as usize),
        _ => (24, 80),
    };
    Ok(render_view(&[part], &view, opts, rows, cols, None))
}

/// The rows of the parts: the first module's text, then each further
/// module's title as a heading with its text under it.
fn body_rows(
    parts: &[HelpPart],
    opts: &Options,
    text_col: usize,
    limit: usize,
    heading_col: usize,
) -> Vec<doc::Row> {
    let mut rows: Vec<doc::Row> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let ctx = Ctx {
            group: p.group,
            dialog: &NoDialog,
            opts,
            window: false,
        };
        if i > 0 {
            rows.push(Vec::new());
            rows.push(vec![doc::Seg {
                col: heading_col,
                attr: Attr::HighIntensity,
                text: p.title.clone(),
                link: None,
            }]);
            rows.push(Vec::new());
        }
        rows.extend(doc::flow(&ctx, &p.body, text_col, limit, heading_col));
    }
    rows
}

/// The text of a help display as lines, for printing it: the parts
/// flowed as a full-screen display flows them.
#[must_use]
pub fn help_lines(parts: &[HelpPart], opts: &Options, cols: usize) -> Vec<String> {
    let rows = body_rows(parts, opts, 6, cols - 7, 2);
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for seg in row {
                let at = seg.col - 1;
                while line.chars().count() < at {
                    line.push(' ');
                }
                line.push_str(&seg.text);
            }
            line.trim_end().to_string()
        })
        .collect()
}

/// Render a help display: full screen (`rows` by `cols`) or a window
/// over `under`.
#[must_use]
pub fn render_view(
    parts: &[HelpPart],
    view: &HelpView,
    opts: &Options,
    rows: usize,
    cols: usize,
    under: Option<&Screen>,
) -> Screen {
    match (view.window, under) {
        (Some(w), Some(u)) => render_window(parts, view, opts, w, u),
        _ => render_full(parts, view, opts, rows, cols),
    }
}

fn render_full(
    parts: &[HelpPart],
    view: &HelpView,
    opts: &Options,
    rows: usize,
    cols: usize,
) -> Screen {
    let mut s = Screen::new(rows, cols);
    s.put(1, 2, Attr::Blue, "Help");
    let title = view.title.trim();
    s.put(
        1,
        layout::center(cols, title.chars().count()),
        Attr::HighIntensity,
        title,
    );
    // Text rows 3 to rows-4, the indicator on rows-3, keys on rows-2 and rows-1.
    let text_rows = rows - 6;
    let body = body_rows(parts, opts, 6, cols - 7, 2);
    let pages = body.len().div_ceil(text_rows).max(1);
    let page = view.page.min(pages - 1);
    let start = page * text_rows;
    let end = (start + text_rows).min(body.len());
    doc::put_rows(&mut s, &body[start..end], 3);
    super::indicator(&mut s, rows - 3, page + 1 == pages);
    let limit = s.limit();
    put_keys(&mut s, &view.keys, 2, limit, rows - 2);
    if let Some((m, _)) = &opts.message {
        s.put(rows, 2, Attr::HighIntensity, m);
    }
    s
}

/// R23: a help window, as the 7.1 oracle draws one over a prompt: the
/// title centered on its first row, a blank row, the text, the
/// indicator, two key rows and the message row, each from the third
/// column inside the border and ending two columns before the right
/// one. The window is as tall as the text needs, up to `w.rows`; a
/// window that would pass row `rows - 3` moves up.
fn render_window(
    parts: &[HelpPart],
    view: &HelpView,
    opts: &Options,
    w: HelpWindow,
    under: &Screen,
) -> Screen {
    let first = w.left + 3;
    let last = w.left + w.cols - 2;
    let body = body_rows(parts, opts, first, last, first);
    let chrome = 6;
    let rows = (chrome + body.len()).min(w.rows).max(chrome);
    let lowest = under.rows.saturating_sub(3);
    let top = if w.top + rows + 1 > lowest {
        lowest.saturating_sub(rows + 1).max(1)
    } else {
        w.top
    };
    let mut s = window::frame(under, top, w.left, rows, w.cols);
    // The column before the left border holds the window's attribute.
    if w.left > 1 {
        let attr_col = w.left - 1;
        for t in &mut s.texts {
            if (top..=top + rows + 1).contains(&t.row)
                && t.col <= attr_col
                && attr_col < t.col + t.text.chars().count()
            {
                t.text = t.text.chars().take(attr_col - t.col).collect();
            }
        }
    }
    let title = view.title.trim();
    s.put(
        top + 1,
        w.left + 1 + w.cols.saturating_sub(title.chars().count()) / 2,
        Attr::HighIntensity,
        title,
    );
    let text_top = top + 3;
    let indicator_row = top + rows - 3;
    let text_rows = indicator_row.saturating_sub(text_top).max(1);
    let pages = body.len().div_ceil(text_rows).max(1);
    let page = view.page.min(pages - 1);
    let start = page * text_rows;
    let end = (start + text_rows).min(body.len());
    if indicator_row > text_top {
        doc::put_rows(&mut s, &body[start..end], text_top);
    }
    let text = if page + 1 == pages {
        "Bottom"
    } else {
        "More..."
    };
    s.put_right(indicator_row, last, Attr::HighIntensity, text);
    s.more = Some(super::More {
        row: indicator_row,
        col: last + 1 - text.len(),
        bottom: page + 1 == pages,
    });
    put_keys(&mut s, &view.keys, first, last, indicator_row + 1);
    if let Some((m, _)) = &opts.message {
        let m: String = m.chars().take(last + 1 - first).collect();
        s.put(top + rows, first, Attr::HighIntensity, &m);
    }
    s
}

/// R22: help keys on two rows from `top`: flowed from `start`, three
/// blanks apart, each item of the second row moved right to the next
/// column the first row has (`F14=Print help` under `F10=Move to top`).
fn put_keys(s: &mut Screen, keys: &[String], start: usize, limit: usize, top: usize) {
    let widths: Vec<usize> = keys.iter().map(|k| k.chars().count()).collect();
    let mut rows = greedy(&widths, start, limit);
    if rows.len() == 2 {
        let stops: Vec<usize> = rows[0].iter().map(|&(_, c)| c).collect();
        let mut floor = start;
        let second: Vec<usize> = rows[1].iter().map(|&(k, _)| k).collect();
        for (j, item) in rows[1].iter_mut().enumerate() {
            let col = item.1.max(floor);
            let snapped = stops.iter().copied().find(|&c| c >= col).unwrap_or(col);
            // Snap only when the keys after it still fit on the row.
            let rest_fit = second[j..]
                .iter()
                .try_fold(snapped, |c, &k| {
                    (c + widths[k] <= limit + 1).then_some(c + widths[k] + layout::GAP)
                })
                .is_some();
            if rest_fit {
                item.1 = snapped;
            } else {
                item.1 = col;
            }
            floor = item.1 + widths[item.0] + layout::GAP;
        }
        // Rows of as many keys line up a column where both still fit
        // (the 7.1 oracle's help window: F10 over F14).
        if rows[0].len() == rows[1].len() {
            for i in 1..rows[0].len() {
                let col = rows[0][i].1.max(rows[1][i].1);
                let fits = |row: &[(usize, usize)]| {
                    let mut c = col;
                    for &(k, _) in &row[i..] {
                        if c + widths[k] > limit + 1 {
                            return false;
                        }
                        c += widths[k] + layout::GAP;
                    }
                    true
                };
                if fits(&rows[0]) && fits(&rows[1]) {
                    for r in &mut rows {
                        let mut c = col;
                        for item in &mut r[i..] {
                            item.1 = item.1.max(c);
                            c = item.1 + widths[item.0] + layout::GAP;
                        }
                    }
                }
            }
        }
    }
    for (i, row) in rows.iter().take(2).enumerate() {
        for &(k, col) in row {
            s.put(top + i, col, Attr::Blue, &keys[k]);
        }
    }
}

/// Items flowed left to right, `GAP` blanks apart, wrapping before
/// one that would pass `limit`.
fn greedy(widths: &[usize], start: usize, limit: usize) -> Vec<Vec<(usize, usize)>> {
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut col = start;
    for (i, &w) in widths.iter().enumerate() {
        if rows.is_empty() || (col + w > limit + 1 && col != start) {
            rows.push(Vec::new());
            col = start;
        }
        if let Some(r) = rows.last_mut() {
            r.push((i, col));
        }
        col += w + layout::GAP;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_keys_snap_to_the_row_above() {
        let keys: Vec<String> = [
            "F2=Extended help",
            "F3=Exit help",
            "F10=Move to top",
            "F12=Cancel",
            "F13=Information Assistant",
            "F14=Print help",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let mut s = Screen::new(24, 80);
        put_keys(&mut s, &keys, 2, 79, 22);
        let rows = s.text_rows();
        assert_eq!(&rows[21][1..17], "F2=Extended help");
        assert_eq!(&rows[21][53..63], "F12=Cancel");
        assert_eq!(&rows[22][35..49], "F14=Print help");
    }

    #[test]
    fn titles() {
        assert_eq!(help_title("Options"), "Options - Help");
        assert_eq!(help_title("Main Menu - Help"), "Main Menu - Help");
    }
}
