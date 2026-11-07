// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console as a 5250 terminal: a `tn5250` terminal fed with the
//! records of the console display's TN5250E session (held by the
//! console job `QCONSOLE`, over a local socket), its cells drawn on the
//! framebuffer, the operator's keys typed into it. The screens then come
//! from the jobs like any other display's (`docs/TN5250.md`); the
//! console's own screens (SRCs, DST, the installer) show only while the
//! link is down.

use tn5250::ds::attr;
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::term::{Key as TermKey, KeyResult, Terminal};

use crate::keyboard::map::Key;
use crate::screen::{Attr, COLS, Color, ROWS, Screen};

/// The operator status for a locked keyboard, as the service shows it.
const INHIBITED: &str = "X II";

/// The console's terminal.
pub struct ConsoleTerminal {
    term: Terminal,
}

/// The console key as the terminal takes it.
#[must_use]
pub fn term_key(key: Key) -> Option<TermKey> {
    Some(match key {
        Key::Char(c) => TermKey::Char(c),
        Key::Enter => TermKey::Enter,
        Key::FieldAdvance => TermKey::Tab,
        Key::FieldBackspace => TermKey::BackTab,
        Key::FieldExit => TermKey::FieldExit,
        Key::Backspace => TermKey::Backspace,
        Key::Home => TermKey::Home,
        Key::Insert => TermKey::Insert,
        Key::Delete => TermKey::Delete,
        Key::Function(n) => TermKey::F(n),
        Key::Attention => TermKey::Attn,
        Key::SysReq => TermKey::SysReq,
        Key::Reset => TermKey::Reset,
        Key::Left => TermKey::Left,
        Key::Right => TermKey::Right,
        Key::Up => TermKey::Up,
        Key::Down => TermKey::Down,
        Key::PageUp => TermKey::PageUp,
        Key::PageDown => TermKey::PageDown,
    })
}

/// The framebuffer attribute of a 5250 attribute byte.
#[must_use]
pub fn fb_attr(a: u8) -> Attr {
    let color = match a & 0x18 {
        0x00 => {
            if a & 0x02 != 0 {
                Color::White
            } else {
                Color::Green
            }
        }
        0x08 => Color::Red,
        // Turquoise and yellow (column separators): the console's
        // palette has one of them.
        0x10 => Color::Turquoise,
        _ => {
            if a & 0x02 != 0 {
                Color::Blue
            } else {
                Color::Pink
            }
        }
    };
    Attr {
        color,
        high: matches!(a, attr::WHITE | attr::WHITE_RI | attr::WHITE_UL),
        reverse: a & 0x01 != 0 && !attr::is_nondisplay(a),
        underline: a & 0x04 != 0 && !attr::is_nondisplay(a),
        blink: (0x28..=0x2F).contains(&a) && a & 0x02 != 0,
        column_sep: (0x30..=0x37).contains(&a),
        nondisplay: attr::is_nondisplay(a),
        ..Attr::GREEN
    }
}

impl ConsoleTerminal {
    /// A 24x80 color terminal (a 3179-2), the framebuffer's size.
    #[must_use]
    pub fn new() -> Self {
        Self {
            term: Terminal::new(Capabilities {
                model: Model::M3179,
                gui: false,
                color: true,
            }),
        }
    }

    /// A record from the session.
    pub fn apply(&mut self, rec: &Record) {
        let _ = self.term.apply(rec);
    }

    /// A key from the operator: the records it produced, encoded.
    pub fn key(&mut self, key: Key) -> Vec<Vec<u8>> {
        let Some(k) = term_key(key) else {
            return Vec::new();
        };
        let _ = self.term.key(k);
        self.term.take_outbox().iter().map(Record::encode).collect()
    }

    /// Whether the keyboard is locked.
    #[must_use]
    pub fn locked(&self) -> bool {
        self.term.is_locked() || self.term.error().is_some()
    }

    /// The screen to draw: the cells with their attributes, the cursor.
    #[must_use]
    pub fn screen(&self) -> Screen {
        let mut s = Screen::new();
        let cells = self.term.cells();
        let cols = self.term.cols();
        // An attribute holds until the next one, across row ends (a
        // field that wraps keeps its underline on the next row); the
        // first cell has the screen's last attribute, as the buffer
        // wraps.
        let mut a = cells
            .iter()
            .rev()
            .copied()
            .find(|b| attr::is_attr(*b))
            .unwrap_or(attr::GREEN);
        for r in 0..ROWS.min(self.term.rows()) {
            for c in 0..COLS.min(cols) {
                let b = cells[r * cols + c];
                if attr::is_attr(b) {
                    a = b;
                    continue;
                }
                let ch = if b == 0 { ' ' } else { ccsid::char_of(b) };
                s.put(r + 1, c + 1, &ch.to_string(), fb_attr(a));
            }
        }
        let (row, col) = self.term.cursor();
        s.cursor = (row.saturating_sub(1), col.saturating_sub(1));
        if self.locked() {
            // Input inhibited: the operator sees the indicator, not the
            // message the host has yet to replace.
            s.put(ROWS, 1, &" ".repeat(COLS), Attr::GREEN);
            s.put(ROWS, 1, INHIBITED, Attr::WHITE);
        }
        s
    }

    /// The text rows, for dumps.
    #[must_use]
    pub fn text_rows(&self) -> Vec<String> {
        let mut rows = self.term.text_rows();
        if self.locked()
            && let Some(last) = rows.get_mut(ROWS - 1)
        {
            *last = format!("{INHIBITED:<COLS$}");
        }
        rows
    }

    /// Whether the last key was taken (for tests).
    #[must_use]
    pub fn type_str(&mut self, s: &str) -> KeyResult {
        self.term.type_str(s)
    }
}

impl Default for ConsoleTerminal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tn5250::ds::field::Ffw;
    use tn5250::screen::{Read, ScreenModel};

    #[test]
    fn attributes_map() {
        assert_eq!(fb_attr(attr::WHITE).color, Color::White);
        assert!(fb_attr(attr::WHITE).high);
        assert!(fb_attr(attr::GREEN_UL).underline);
        assert!(fb_attr(attr::GREEN_RI).reverse);
        assert_eq!(fb_attr(attr::RED).color, Color::Red);
        assert_eq!(fb_attr(attr::BLUE).color, Color::Blue);
        assert!(fb_attr(attr::NONDISPLAY).nondisplay);
    }

    #[test]
    fn records_show_and_keys_answer() {
        let mut m = ScreenModel::new(false);
        m.put(1, 36, "Sign On", attr::WHITE);
        m.field(6, 53, 10, attr::GREEN_UL, "", Ffw::default(), &[]);
        let out = m.render(Read::MdtFields);
        let mut t = ConsoleTerminal::new();
        t.apply(&Record::new(out.opcode, out.data));
        let s = t.screen();
        assert_eq!(s.cell(0, 35).ch, 'S');
        assert!(s.cell(0, 35).attr.high);
        assert!(!t.locked());
        for c in "CODY".chars() {
            assert!(t.key(Key::Char(c)).is_empty());
        }
        let sent = t.key(Key::Enter);
        assert_eq!(sent.len(), 1);
        assert!(t.locked());
        assert!(t.text_rows()[23].starts_with("X II"));
        assert!(t.text_rows()[5].contains("CODY"));
    }

    /// The pixel row of a cell's underline.
    fn underline_y(l: crate::scale::Layout, row: usize) -> usize {
        use crate::font::GLYPH_H;
        l.y0 + (row * GLYPH_H + GLYPH_H - 2) * l.scale
    }

    #[test]
    fn a_wrapping_field_is_underlined_on_both_rows() {
        use crate::font::GLYPH_W;
        use crate::scale::{Canvas, Mode, PixelFormat, Scale};
        // The MAIN menu's command line: row 20 from column 7, all of
        // row 21 but its last column (153 cells).
        let mut model = ScreenModel::new(false);
        model.put(20, 2, "===>", attr::GREEN);
        model.field(20, 7, 153, attr::GREEN_UL, "", Ffw::default(), &[]);
        model.put(22, 2, "F3=Exit", attr::BLUE);
        let out = model.render(Read::MdtFields);
        let mut t = ConsoleTerminal::new();
        t.apply(&Record::new(out.opcode, out.data));
        let under = |s: &Screen, r: usize, c: usize| s.cell(r - 1, c - 1).attr.underline;
        let s = t.screen();
        assert!((7..=80).all(|c| under(&s, 20, c)), "row 20");
        assert!((1..=79).all(|c| under(&s, 21, c)), "row 21");
        assert!(!under(&s, 21, 80) && !under(&s, 20, 5) && !under(&s, 22, 3));
        // Typed past row 20: the text goes on at row 21, column 1.
        let cmd = "CRTLINETH LIND(ETHLINE) RSRCNAME(CMN04) LINESPEED(*AUTO) \
                   DUPLEX(*AUTO) ONLINE(*YES) TEXT('Primary Ethernet')";
        for c in cmd.chars() {
            t.key(Key::Char(c));
        }
        let rows = t.text_rows();
        assert_eq!(&rows[19][6..], &cmd[..74]);
        assert!(rows[20].starts_with(&cmd[74..]));
        let s = t.screen();
        assert!((1..=79).all(|c| under(&s, 21, c)), "row 21 with text");
        // Drawn: the underline scanline lit on both rows, empty cells
        // included, and dark past the field.
        let mode = Mode::packed(1024, 768, PixelFormat::XRGB8888);
        let mut canvas = Canvas::new(mode, ROWS, COLS, Scale::Auto);
        canvas.draw(&s);
        let lay = canvas.layout();
        let lit = |r: usize, c: usize| {
            let x = lay.x0 + ((c - 1) * GLYPH_W + 1) * lay.scale;
            canvas.pixel(x, underline_y(lay, r - 1)) != 0
        };
        assert!(lit(20, 80) && lit(21, 1) && lit(21, 50) && lit(21, 79));
        assert!(!lit(21, 80) && !lit(23, 40));
    }
}
