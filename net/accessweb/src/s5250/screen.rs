// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A 5250 screen as the session page shows it: the cells, the cursor
//! and the keyboard, and the status row's state; drawn to a picture
//! with the console's rules ([`super::look`]).

use super::look::render::{self, Canvas, Cell, Keyboard, Look, Oia, Paint, Scheme};
use super::look::{font, png};

/// A screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Rows.
    pub rows: usize,
    /// Columns.
    pub cols: usize,
    /// `rows * cols` cells, row by row.
    pub cells: Vec<Cell>,
    /// The cursor, 1-based row and column.
    pub cursor: (usize, usize),
    /// The keyboard.
    pub keyboard: Keyboard,
    /// Insert mode.
    pub insert: bool,
    /// The message light.
    pub message_waiting: bool,
}

impl Screen {
    /// A blank screen.
    pub fn blank(rows: usize, cols: usize) -> Self {
        let cell = Cell {
            ch: ' ',
            attr: 0x20,
            is_attr: false,
        };
        Self {
            rows,
            cols,
            cells: vec![cell; rows * cols],
            cursor: (1, 1),
            keyboard: Keyboard::Open,
            insert: false,
            message_waiting: false,
        }
    }

    /// A screen from its text form: rows of text, each followed by a row
    /// of styles (`.` or `g` green, `w` white, `r` red, `t` turquoise,
    /// `p` pink, `b` blue; upper case underlined); `#` lines are
    /// comments. Placed at the top left of a `rows` by `cols` screen.
    pub fn from_text(text: &str, rows: usize, cols: usize, cursor: (usize, usize)) -> Self {
        let mut s = Self::blank(rows, cols);
        s.cursor = cursor;
        let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
        for (r, pair) in lines.chunks(2).enumerate().take(rows) {
            let chars: Vec<char> = pair[0].chars().collect();
            let styles: Vec<char> = pair.get(1).map(|l| l.chars().collect()).unwrap_or_default();
            for (c, &ch) in chars.iter().enumerate().take(cols) {
                let st = styles.get(c).copied().unwrap_or('.');
                let base = match st.to_ascii_lowercase() {
                    'w' => 0x22,
                    'r' => 0x28,
                    't' => 0x30,
                    'p' => 0x38,
                    'b' => 0x3A,
                    _ => 0x20,
                };
                let attr = if st.is_ascii_uppercase() {
                    base | 0x04
                } else {
                    base
                };
                s.cells[r * cols + c] = Cell {
                    ch,
                    attr,
                    is_attr: false,
                };
            }
        }
        s
    }

    /// The text of each row, attribute bytes and nondisplay fields as
    /// blanks.
    pub fn text_rows(&self) -> Vec<String> {
        self.cells
            .chunks(self.cols)
            .map(|row| {
                row.iter()
                    .map(|c| {
                        if c.is_attr || render::style(c.attr).nondisplay {
                            ' '
                        } else {
                            c.ch
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// The text of a rectangle (1-based, inclusive), one line per row,
    /// trailing blanks dropped: what Copy puts on the clipboard.
    pub fn copy(&self, from: (usize, usize), to: (usize, usize)) -> String {
        let rows = self.text_rows();
        let (r0, r1) = (from.0.min(to.0).max(1), from.0.max(to.0).min(self.rows));
        let (c0, c1) = (from.1.min(to.1).max(1), from.1.max(to.1).min(self.cols));
        let mut out = Vec::new();
        for row in rows.iter().take(r1).skip(r0 - 1) {
            let s: String = row.chars().skip(c0 - 1).take(c1 + 1 - c0).collect();
            out.push(s.trim_end().to_string());
        }
        out.join("\n")
    }

    /// The cells to draw, the cursor marked.
    pub fn paints(&self) -> Vec<Paint> {
        let mut p: Vec<Paint> = self.cells.iter().map(|c| Paint::of(*c)).collect();
        let (r, c) = self.cursor;
        if r >= 1 && c >= 1 && r <= self.rows && c <= self.cols {
            p[(r - 1) * self.cols + c - 1].cursor = true;
        }
        p
    }
}

/// A screen, its session's state and the look it is drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The screen (the last one, when disconnected).
    pub screen: Screen,
    /// The system name.
    pub system: String,
    /// Whether the session is connected.
    pub connected: bool,
    /// Whether it is over TLS.
    pub tls: bool,
    /// The colour map.
    pub scheme: Scheme,
}

impl View {
    /// The status row.
    pub fn oia(&self) -> Oia {
        let s = &self.screen;
        Oia {
            system: self.system.clone(),
            state: if self.connected {
                String::new()
            } else {
                "Not connected".into()
            },
            keyboard: self.connected.then_some(s.keyboard),
            insert: s.insert && self.connected,
            message_waiting: s.message_waiting && self.connected,
            tls: self.tls && self.connected,
            cursor: s.cursor,
        }
    }

    /// How the cells draw.
    pub fn look(&self) -> Look {
        Look {
            scheme: self.scheme,
            greyed: !self.connected,
        }
    }

    /// The picture at `scale`.
    pub fn picture(&self, scale: usize) -> Canvas {
        let s = &self.screen;
        let status = render::oia_cells(&self.oia(), s.cols);
        render::render(s.rows, s.cols, scale, &s.paints(), &status, self.look())
    }

    /// The picture as a PNG.
    pub fn png(&self, scale: usize) -> Vec<u8> {
        let c = self.picture(scale);
        png::encode(c.w, c.h, &c.px)
    }

    /// Width and height of the picture at `scale`.
    pub fn size(&self, scale: usize) -> (usize, usize) {
        let s = scale.max(1);
        (
            self.screen.cols * font::GLYPH_W * s,
            (self.screen.rows + 1) * font::GLYPH_H * s,
        )
    }
}
