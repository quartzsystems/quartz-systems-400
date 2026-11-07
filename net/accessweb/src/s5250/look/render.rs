// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console's look for a 5250 screen: its palette, its attribute
//! rules and its way of drawing a cell, as QS/400 Access for Desktop
//! reproduces them (`qsa-ui/src/console.rs`): the same values, so the
//! browser, the desktop and the console show one picture. [`render`] is
//! pure; the PNG, Print Screen and the page's script share its rules.

// Bit, byte and pixel arithmetic on values the formats bound.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

use super::font::{GLYPH_H, GLYPH_W, glyph};

/// The console palette, `0x00RRGGBB` (`lic/console/src/palette.rs`).
pub mod palette {
    /// Background.
    pub const BACKGROUND: u32 = 0x0000_0000;
    /// Green.
    pub const GREEN: u32 = 0x0033_CC33;
    /// White (high intensity).
    pub const WHITE: u32 = 0x00E6_E6E6;
    /// Red.
    pub const RED: u32 = 0x00E0_3030;
    /// Blue.
    pub const BLUE: u32 = 0x0050_80FF;
    /// Turquoise.
    pub const TURQUOISE: u32 = 0x0020_C8C8;
    /// Pink.
    pub const PINK: u32 = 0x00E0_60C0;
}

/// A 5250 colour as the console knows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Green.
    Green,
    /// White.
    White,
    /// Red.
    Red,
    /// Blue.
    Blue,
    /// Turquoise.
    Turquoise,
    /// Pink.
    Pink,
}

/// How an attribute draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
    /// The colour.
    pub color: Color,
    /// Reverse image.
    pub reverse: bool,
    /// Underline.
    pub underline: bool,
    /// Column separators.
    pub column_sep: bool,
    /// Nondisplay.
    pub nondisplay: bool,
}

/// The style of attribute byte `a` (`fb_attr` in the console).
pub fn style(a: u8) -> Style {
    let color = match a & 0x18 {
        0x00 if a & 0x02 != 0 => Color::White,
        0x00 => Color::Green,
        0x08 => Color::Red,
        // Turquoise and yellow: the console's palette has one of them.
        0x10 => Color::Turquoise,
        _ if a & 0x02 != 0 => Color::Blue,
        _ => Color::Pink,
    };
    let nd = a & 0x07 == 0x07;
    Style {
        color,
        reverse: a & 0x01 != 0 && !nd,
        underline: a & 0x04 != 0 && !nd,
        column_sep: (0x30..=0x37).contains(&a),
        nondisplay: nd,
    }
}

/// An attribute byte's own cell: blank and green.
pub const BLANK: Style = Style {
    color: Color::Green,
    reverse: false,
    underline: false,
    column_sep: false,
    nondisplay: false,
};

/// Colour scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scheme {
    /// The console's: green on black.
    #[default]
    Console,
    /// Black on the console's white.
    BlackOnWhite,
}

impl Scheme {
    /// The background.
    pub fn background(self) -> u32 {
        match self {
            Scheme::Console => palette::BACKGROUND,
            Scheme::BlackOnWhite => palette::WHITE,
        }
    }

    /// A colour's foreground.
    pub fn foreground(self, c: Color) -> u32 {
        match (self, c) {
            (Scheme::BlackOnWhite, Color::Green | Color::White) => palette::BACKGROUND,
            (_, Color::Green) => palette::GREEN,
            (_, Color::White) => palette::WHITE,
            (_, Color::Red) => palette::RED,
            (_, Color::Blue) => palette::BLUE,
            (_, Color::Turquoise) => palette::TURQUOISE,
            (_, Color::Pink) => palette::PINK,
        }
    }

    /// The cursor.
    pub fn cursor(self) -> u32 {
        match self {
            Scheme::Console => palette::WHITE,
            Scheme::BlackOnWhite => palette::BACKGROUND,
        }
    }
}

/// A screen cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// The character.
    pub ch: char,
    /// The attribute in effect, or the attribute byte itself.
    pub attr: u8,
    /// Whether the cell holds an attribute byte.
    pub is_attr: bool,
}

/// One cell to draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    /// The character.
    pub ch: char,
    /// How it draws.
    pub style: Style,
    /// The cursor is here.
    pub cursor: bool,
    /// Selected with the mouse: reversed once more.
    pub selected: bool,
}

impl Paint {
    /// A screen cell as the console paints it.
    pub fn of(cell: Cell) -> Paint {
        Paint {
            ch: if cell.is_attr { ' ' } else { cell.ch },
            style: if cell.is_attr { BLANK } else { style(cell.attr) },
            cursor: false,
            selected: false,
        }
    }

    /// Plain text in a colour, for the status row.
    pub fn text(ch: char, color: Color) -> Paint {
        Paint {
            ch,
            style: Style { color, ..BLANK },
            cursor: false,
            selected: false,
        }
    }
}

/// The scheme, and whether the session is gone (the last screen stays,
/// every colour halfway to the background).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    /// The scheme.
    pub scheme: Scheme,
    /// Disconnected.
    pub greyed: bool,
}

fn grey(c: u32, bg: u32) -> u32 {
    let ch = |v: u32, s: u32| ((v >> s) & 0xFF).midpoint((bg >> s) & 0xFF) << s;
    ch(c, 16) | ch(c, 8) | ch(c, 0)
}

/// An RGB buffer, three bytes a pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// The pixels, row by row.
    pub px: Vec<u8>,
}

impl Canvas {
    /// A canvas filled with `rgb`.
    pub fn new(w: usize, h: usize, rgb: u32) -> Canvas {
        let mut c = Canvas { w, h, px: vec![0; w * h * 3] };
        c.fill(0, 0, w, h, rgb);
        c
    }

    /// The `0x00RRGGBB` pixel at (x, y).
    pub fn at(&self, x: usize, y: usize) -> u32 {
        let i = (y * self.w + x) * 3;
        (u32::from(self.px[i]) << 16) | (u32::from(self.px[i + 1]) << 8) | u32::from(self.px[i + 2])
    }

    /// Fill a rectangle, clipped to the canvas.
    pub fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, rgb: u32) {
        let [_, r, g, b] = rgb.to_be_bytes();
        let x1 = (x + w).min(self.w);
        for yy in y..(y + h).min(self.h) {
            let row = yy * self.w * 3;
            for xx in x..x1 {
                let i = row + xx * 3;
                self.px[i..i + 3].copy_from_slice(&[r, g, b]);
            }
        }
    }
}

/// The colours a cell draws with: foreground, background, cursor.
pub fn colors(p: Paint, look: Look) -> (u32, u32, u32) {
    let scheme = look.scheme;
    let mut fg = scheme.foreground(p.style.color);
    let mut bg = scheme.background();
    if p.style.reverse {
        (fg, bg) = (bg, fg);
    }
    if p.selected {
        (fg, bg) = (bg, fg);
    }
    let mut cursor = scheme.cursor();
    if look.greyed {
        let base = scheme.background();
        fg = grey(fg, base);
        bg = if bg == base { bg } else { grey(bg, base) };
        cursor = grey(cursor, base);
    }
    (fg, bg, cursor)
}

/// Which pixels of a cell's scanline `gy` are lit (bit 8 leftmost),
/// and the colour that fills the whole line instead, if one does.
pub fn scanline(p: Paint, gy: usize) -> (u16, Option<Line>) {
    if p.cursor && gy == GLYPH_H - 1 {
        return (0, Some(Line::Cursor));
    }
    if p.style.underline && gy == GLYPH_H - 2 {
        return (0, Some(Line::Foreground));
    }
    let row = if p.style.nondisplay { 0 } else { glyph(p.ch)[gy] };
    let sep = if p.style.column_sep && gy.is_multiple_of(4) { 0x101 } else { 0 };
    (row | sep, None)
}

/// A scanline drawn whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line {
    /// The cursor's line.
    Cursor,
    /// The underline.
    Foreground,
}

/// Draw one cell with its top-left glyph pixel at (x, y), each glyph
/// pixel an s×s block: the console's `cells_line`, scanline by scanline.
pub fn paint_cell(c: &mut Canvas, x: usize, y: usize, s: usize, p: Paint, look: Look) {
    let (fg, bg, cursor) = colors(p, look);
    for gy in 0..GLYPH_H {
        match scanline(p, gy) {
            (_, Some(line)) => {
                let color = if line == Line::Cursor { cursor } else { fg };
                c.fill(x, y + gy * s, GLYPH_W * s, s, color);
            }
            (bits, None) => {
                for gx in 0..GLYPH_W {
                    let lit = bits & (1 << (GLYPH_W - 1 - gx)) != 0;
                    c.fill(x + gx * s, y + gy * s, s, s, if lit { fg } else { bg });
                }
            }
        }
    }
}

/// The keyboard as the status row shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyboard {
    /// Open.
    Open,
    /// `X SYSTEM`: the host has it.
    System,
    /// `X II`: input inhibited.
    HostError,
    /// `X II nnnn`: an operator error.
    OperatorError(u16),
}

/// What the status row below the grid shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Oia {
    /// The system name.
    pub system: String,
    /// `Not connected`, ... (empty when connected).
    pub state: String,
    /// The keyboard.
    pub keyboard: Option<Keyboard>,
    /// Insert mode.
    pub insert: bool,
    /// A message waits.
    pub message_waiting: bool,
    /// The session is TLS.
    pub tls: bool,
    /// 1-based row and column.
    pub cursor: (usize, usize),
}

/// The status row as cells across `cols` columns (1-based): 1-9 the
/// keyboard in white, 12 `MW`, 16 `INS`, 21 `TLS`, 26 the system and
/// state, the last six the cursor `RR/CCC`.
pub fn oia_cells(o: &Oia, cols: usize) -> Vec<Paint> {
    let mut cells = vec![Paint::text(' ', Color::Green); cols];
    let mut put = |col: usize, text: &str, color: Color, max: usize| {
        for (i, ch) in text.chars().take(max).enumerate() {
            if let Some(c) = cells.get_mut(col - 1 + i) {
                *c = Paint::text(ch, color);
            }
        }
    };
    let kb = match o.keyboard {
        Some(Keyboard::System) => "X SYSTEM".to_owned(),
        Some(Keyboard::HostError) => "X II".to_owned(),
        Some(Keyboard::OperatorError(c)) => format!("X II {c:04}"),
        Some(Keyboard::Open) | None => String::new(),
    };
    put(1, &kb, Color::White, 9);
    if o.message_waiting {
        put(12, "MW", Color::Green, 2);
    }
    if o.insert {
        put(16, "INS", Color::Green, 3);
    }
    if o.tls {
        put(21, "TLS", Color::Green, 3);
    }
    let room = cols.saturating_sub(26 + 7);
    let label = if o.state.is_empty() { o.system.clone() } else { format!("{} {}", o.system, o.state) };
    put(26, &label, Color::Green, room);
    let rc = format!("{:02}/{:03}", o.cursor.0, o.cursor.1);
    put(cols.saturating_sub(5).max(1), &rc, Color::Green, 6);
    cells
}

/// A grid of `rows` by `cols` and its status row, at scale `scale`.
pub fn render(rows: usize, cols: usize, scale: usize, cells: &[Paint], status: &[Paint], look: Look) -> Canvas {
    let s = scale.max(1);
    let (w, h) = (cols * GLYPH_W * s, (rows + 1) * GLYPH_H * s);
    let mut c = Canvas::new(w, h, look.scheme.background());
    for (i, p) in cells.iter().enumerate().take(rows * cols) {
        let (r, col) = (i / cols, i % cols);
        paint_cell(&mut c, col * GLYPH_W * s, r * GLYPH_H * s, s, *p, look);
    }
    for (col, p) in status.iter().enumerate().take(cols) {
        paint_cell(&mut c, col * GLYPH_W * s, rows * GLYPH_H * s, s, *p, look);
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_map_as_on_the_console() {
        assert_eq!(style(0x20).color, Color::Green);
        assert_eq!(style(0x22).color, Color::White);
        assert!(style(0x21).reverse && style(0x24).underline);
        assert!(style(0x27).nondisplay && !style(0x27).underline);
        assert_eq!(style(0x28).color, Color::Red);
        assert!(style(0x30).column_sep && style(0x34).color == Color::Turquoise);
        assert_eq!((style(0x38).color, style(0x3A).color), (Color::Pink, Color::Blue));
    }

    #[test]
    fn greyed_is_halfway() {
        let look = Look {
            scheme: Scheme::Console,
            greyed: true,
        };
        let (fg, _, cursor) = colors(Paint::text('H', Color::Green), look);
        assert_eq!((fg, cursor), (0x0019_6619, 0x0073_7373));
    }

    #[test]
    fn status_row() {
        let o = Oia {
            system: "QS400".into(),
            state: "Not connected".into(),
            keyboard: Some(Keyboard::OperatorError(5)),
            insert: true,
            message_waiting: true,
            tls: true,
            cursor: (6, 53),
        };
        let t: String = oia_cells(&o, 80).iter().map(|p| p.ch).collect();
        assert_eq!(&t[..9], "X II 0005");
        assert_eq!((&t[11..13], &t[15..18], &t[20..23]), ("MW", "INS", "TLS"));
        assert_eq!(&t[25..44], "QS400 Not connected");
        assert_eq!(&t[74..], "06/053");
    }
}
