// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The grid on a serial terminal: ordinary ANSI control sequences out
//! ([`AnsiDisplay`]), the terminal's keys in ([`KeyDecoder`]).
//!
//! A terminal has no 5250 keys of its own, so some come from control
//! characters: Ctrl+K is Field Exit, Ctrl+R Reset, Ctrl+A Attention,
//! Ctrl+T System Request. Function keys arrive as the terminal sends
//! them (F13 to F24 as Shift+F1 to Shift+F12) or as Esc and a key of
//! the top row: Esc 1 to Esc 9, Esc 0, Esc -, Esc = for F1 to F12, the
//! same with Shift held for F13 to F24.

use std::fmt::Write as _;
use std::io::{self, Write};

use crate::display::Display;
use crate::keyboard::map::Key;
use crate::scale::Cells;
use crate::screen::{Attr, Cell, Color};

/// Draws a grid on a terminal that understands ANSI sequences.
pub struct AnsiDisplay<W: Write + Send> {
    out: W,
    /// The cells the terminal shows, once something was drawn.
    shown: Option<Vec<Cell>>,
    size: (usize, usize),
}

/// The SGR parameters of an attribute.
fn sgr(a: Attr) -> String {
    let fg = match (a.color, a.high) {
        (Color::Green, false) => 32,
        (Color::Green, true) | (Color::White, _) => 97,
        (Color::Red, false) => 31,
        (Color::Red, true) => 91,
        (Color::Blue, false) => 34,
        (Color::Blue, true) => 94,
        (Color::Turquoise, false) => 36,
        (Color::Turquoise, true) => 96,
        (Color::Pink, false) => 35,
        (Color::Pink, true) => 95,
        (Color::Yellow, false) => 33,
        (Color::Yellow, true) => 93,
    };
    let mut s = format!("\x1b[0;{fg}");
    for (on, code) in [(a.underline, 4), (a.blink, 5), (a.reverse, 7)] {
        if on {
            let _ = write!(s, ";{code}");
        }
    }
    s.push('m');
    s
}

/// Unchanged cells between two changed ones that are sent again rather
/// than jumped over: moving the cursor costs as many bytes, and a
/// label's words stay together on the line.
const BRIDGE: usize = 7;

/// What a cell shows: nothing for a nondisplay cell or the second half
/// of a double-width character.
fn glyph(c: Cell) -> char {
    if c.attr.nondisplay || c.attr.continuation || c.ch.is_control() {
        ' '
    } else {
        c.ch
    }
}

/// What the terminal would show differently: the character and the
/// attributes a terminal renders.
fn look(c: Cell) -> (char, Attr) {
    let mut a = c.attr;
    a.column_sep = false;
    a.continuation = false;
    a.double_width = false;
    a.nondisplay = false;
    (glyph(c), a)
}

impl<W: Write + Send> AnsiDisplay<W> {
    /// A display writing to `out`. Nothing is sent until the first draw.
    pub fn new(out: W) -> Self {
        Self {
            out,
            shown: None,
            size: (0, 0),
        }
    }

    /// Forget what the terminal shows: the next draw repaints all of it
    /// (the terminal was cleared, or another one was plugged in).
    pub fn invalidate(&mut self) {
        self.shown = None;
    }

    /// The writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.out
    }

    /// The bytes that turn what the terminal shows into `cells`.
    fn frame(&mut self, cells: &dyn Cells) -> Vec<u8> {
        let (rows, cols) = (cells.rows(), cells.cols());
        if self.size != (rows, cols) {
            self.shown = None;
            self.size = (rows, cols);
        }
        let mut out = String::new();
        // The cursor stays out of sight while the cells change.
        out.push_str("\x1b[?25l");
        if self.shown.is_none() {
            out.push_str("\x1b[0m\x1b[2J");
        }
        let blank = Cell::default();
        let mut now = Vec::with_capacity(rows * cols);
        let mut pen: Option<Attr> = None;
        for r in 0..rows {
            let row: Vec<Cell> = (0..cols).map(|c| cells.cell(r, c)).collect();
            let mut send: Vec<bool> = (0..cols)
                .map(|c| {
                    let before = self.shown.as_ref().map_or(blank, |s| s[r * cols + c]);
                    look(row[c]) != look(before)
                })
                .collect();
            // Short runs of unchanged cells between changed ones go too.
            let mut last: Option<usize> = None;
            for c in 0..cols {
                if !send[c] {
                    continue;
                }
                if let Some(l) = last
                    && c - l - 1 <= BRIDGE
                {
                    send[l + 1..c].fill(true);
                }
                last = Some(c);
            }
            now.extend_from_slice(&row);
            let mut at: Option<usize> = None;
            for c in 0..cols {
                let cell = row[c];
                if !send[c] {
                    continue;
                }
                if at != Some(c) {
                    let _ = write!(out, "\x1b[{};{}H", r + 1, c + 1);
                }
                let (ch, attr) = look(cell);
                if pen != Some(attr) {
                    out.push_str(&sgr(attr));
                    pen = Some(attr);
                }
                out.push(ch);
                at = Some(c + 1);
            }
        }
        self.shown = Some(now);
        let (row, col) = cells.cursor();
        let _ = write!(out, "\x1b[0m\x1b[{};{}H\x1b[?25h", row + 1, col + 1);
        out.into_bytes()
    }
}

impl<W: Write + Send> Display for AnsiDisplay<W> {
    fn draw(&mut self, cells: &dyn Cells) -> io::Result<()> {
        let bytes = self.frame(cells);
        self.out.write_all(&bytes)?;
        self.out.flush()
    }

    fn describe(&self) -> String {
        let (rows, cols) = self.size;
        format!("serial terminal, {cols}x{rows} grid, ANSI sequences")
    }
}

/// Turns the bytes a terminal sends into keys.
#[derive(Debug, Default)]
pub struct KeyDecoder {
    /// An escape sequence, or a UTF-8 character, not yet whole.
    pending: Vec<u8>,
    /// The last byte was a carriage return: a line feed after it is the
    /// same Enter.
    after_cr: bool,
}

/// F1 to F12 from the key of the top row, F13 to F24 with Shift.
fn top_row(b: u8) -> Option<u8> {
    const PLAIN: &[u8] = b"1234567890-=";
    const SHIFT: &[u8] = b"!@#$%^&*()_+";
    let at = |keys: &[u8]| keys.iter().position(|&k| k == b);
    at(PLAIN)
        .map(|i| i + 1)
        .or_else(|| at(SHIFT).map(|i| i + 13))
        .and_then(|n| u8::try_from(n).ok())
}

/// The function key of a `CSI n ~` number.
fn tilde_function(n: u32) -> Option<u8> {
    let key = match n {
        11..=15 => n - 10,
        17..=21 => n - 11,
        23..=26 => n - 12,
        28..=29 => n - 13,
        31..=34 => n - 14,
        _ => return None,
    };
    u8::try_from(key).ok()
}

/// The key of a whole `CSI` or `SS3` sequence: its parameters and its
/// final byte.
fn sequence(params: &str, last: u8) -> Option<Key> {
    let mut nums = params.split(';').map(|p| p.parse::<u32>().unwrap_or(0));
    let first = nums.next().unwrap_or(0);
    // The second parameter is 1 plus the modifiers; Shift is 1.
    let shift = nums.next().is_some_and(|m| m >= 2 && (m - 1) & 1 == 1);
    let function = |n: u8| Key::Function(if shift { n + 12 } else { n });
    Some(match last {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'Z' => Key::FieldBackspace,
        b'M' => Key::Enter,
        b'P' => function(1),
        b'Q' => function(2),
        b'R' => function(3),
        b'S' => function(4),
        b'~' => match first {
            1 | 7 => Key::Home,
            2 => Key::Insert,
            3 => Key::Delete,
            5 => Key::PageUp,
            6 => Key::PageDown,
            n => function(tilde_function(n)?),
        },
        _ => return None,
    })
}

impl KeyDecoder {
    /// A decoder with nothing pending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether bytes of a sequence are held: a caller that sees none
    /// follow for a moment calls [`flush`](Self::flush).
    #[must_use]
    pub fn pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The terminal went quiet: an Esc on its own is Attention, and
    /// anything else held is dropped.
    pub fn flush(&mut self) -> Option<Key> {
        let lone = self.pending == [0x1b];
        self.pending.clear();
        lone.then_some(Key::Attention)
    }

    /// One byte from the terminal; the key it completes, if any.
    pub fn feed(&mut self, b: u8) -> Option<Key> {
        let after_cr = std::mem::replace(&mut self.after_cr, false);
        if self.pending.first() == Some(&0x1b) {
            return self.escape(b);
        }
        if !self.pending.is_empty() || b >= 0x80 {
            return self.utf8(b);
        }
        Some(match b {
            0x1b => {
                self.pending.push(b);
                return None;
            }
            b'\r' => {
                self.after_cr = true;
                Key::Enter
            }
            b'\n' if after_cr => return None,
            b'\n' => Key::Enter,
            b'\t' => Key::FieldAdvance,
            0x08 | 0x7f => Key::Backspace,
            0x01 => Key::Attention,
            0x0b => Key::FieldExit,
            0x12 => Key::Reset,
            0x14 => Key::SysReq,
            0x20..=0x7e => Key::Char(char::from(b)),
            _ => return None,
        })
    }

    /// A byte of a character of several.
    fn utf8(&mut self, b: u8) -> Option<Key> {
        self.pending.push(b);
        match std::str::from_utf8(&self.pending) {
            Ok(s) => {
                let c = s.chars().next();
                self.pending.clear();
                c.map(Key::Char)
            }
            Err(e) if e.error_len().is_none() && self.pending.len() < 4 => None,
            Err(_) => {
                self.pending.clear();
                None
            }
        }
    }

    /// A byte after Esc.
    fn escape(&mut self, b: u8) -> Option<Key> {
        if self.pending.len() == 1 {
            return match b {
                b'[' | b'O' => {
                    self.pending.push(b);
                    None
                }
                // Esc Esc: the first was Attention, the second starts over.
                0x1b => Some(Key::Attention),
                _ => {
                    self.pending.clear();
                    top_row(b).map(Key::Function)
                }
            };
        }
        if (0x40..=0x7e).contains(&b) {
            let params = String::from_utf8_lossy(&self.pending[2..]).into_owned();
            self.pending.clear();
            return sequence(&params, b);
        }
        self.pending.push(b);
        if self.pending.len() > 16 {
            self.pending.clear();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::Screen;

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn the_first_draw_clears_and_later_ones_send_what_changed() {
        let mut d = AnsiDisplay::new(Vec::new());
        let mut s = Screen::new();
        s.put(1, 1, "Title", Attr::WHITE);
        s.put(24, 1, "wrong", Attr::RED);
        s.cursor = (20, 3);
        d.draw(&s).unwrap();
        let first = text(d.get_mut());
        assert!(first.starts_with("\x1b[?25l\x1b[0m\x1b[2J"));
        assert!(first.contains("\x1b[1;1H\x1b[0;97mTitle"));
        assert!(first.contains("\x1b[24;1H\x1b[0;31mwrong"));
        assert!(first.ends_with("\x1b[0m\x1b[21;4H\x1b[?25h"));
        // Nothing changed: only the cursor is placed again.
        d.get_mut().clear();
        d.draw(&s).unwrap();
        assert_eq!(text(d.get_mut()), "\x1b[?25l\x1b[0m\x1b[21;4H\x1b[?25h");
        // One word changes: its cells and no others.
        d.get_mut().clear();
        s.put(24, 1, "wring", Attr::RED);
        d.draw(&s).unwrap();
        assert_eq!(
            text(d.get_mut()),
            "\x1b[?25l\x1b[24;3H\x1b[0;31mi\x1b[0m\x1b[21;4H\x1b[?25h"
        );
        // Told the terminal was cleared: everything again.
        d.invalidate();
        d.get_mut().clear();
        d.draw(&s).unwrap();
        assert!(text(d.get_mut()).contains("Title"));
        assert_eq!(d.describe(), "serial terminal, 80x24 grid, ANSI sequences");
    }

    #[test]
    fn fields_are_underlined_and_passwords_show_nothing() {
        let mut d = AnsiDisplay::new(Vec::new());
        let mut s = Screen::new();
        s.field(6, 53, 4, false);
        s.field(7, 53, 4, true);
        s.put(7, 53, "pw", Attr::INPUT_HIDDEN);
        s.put_double(5, 24, "C6", Attr::WHITE);
        d.draw(&s).unwrap();
        let out = text(d.get_mut());
        assert!(out.contains("\x1b[6;53H\x1b[0;32;4m    "));
        assert!(out.contains("\x1b[7;53H    "), "{out:?}");
        assert!(!out.contains("pw"));
        // Double width: each character and a space.
        assert!(out.contains("C 6 "));
        // The words of a label go out as one run, blanks and all; what
        // is far along the row is reached by moving the cursor.
        let mut d = AnsiDisplay::new(Vec::new());
        let mut s = Screen::new();
        s.put(2, 62, "System:  QSDEMO1", Attr::GREEN);
        s.put(4, 2, "left", Attr::GREEN);
        s.put(4, 40, "right", Attr::GREEN);
        d.draw(&s).unwrap();
        let out = text(d.get_mut());
        assert!(
            out.contains("\x1b[2;62H\x1b[0;32mSystem:  QSDEMO1"),
            "{out:?}"
        );
        assert!(out.contains("\x1b[4;2Hleft\x1b[4;40Hright"), "{out:?}");
    }

    fn keys(d: &mut KeyDecoder, bytes: &[u8]) -> Vec<Key> {
        bytes.iter().filter_map(|&b| d.feed(b)).collect()
    }

    #[test]
    fn keys_of_a_terminal() {
        let mut d = KeyDecoder::new();
        assert_eq!(
            keys(&mut d, b"a1\r\n\t\x7f"),
            [
                Key::Char('a'),
                Key::Char('1'),
                Key::Enter,
                Key::FieldAdvance,
                Key::Backspace
            ]
        );
        assert_eq!(keys(&mut d, b"\n"), [Key::Enter], "a line feed alone");
        assert_eq!(
            keys(&mut d, b"\x1b[A\x1b[B\x1b[C\x1b[D\x1b[Z\x1b[H"),
            [
                Key::Up,
                Key::Down,
                Key::Right,
                Key::Left,
                Key::FieldBackspace,
                Key::Home
            ]
        );
        assert_eq!(
            keys(&mut d, b"\x1b[2~\x1b[3~\x1b[5~\x1b[6~"),
            [Key::Insert, Key::Delete, Key::PageUp, Key::PageDown]
        );
        assert_eq!(
            keys(&mut d, b"\x0b\x12\x01\x14"),
            [Key::FieldExit, Key::Reset, Key::Attention, Key::SysReq]
        );
        assert_eq!(keys(&mut d, "é".as_bytes()), [Key::Char('é')]);
    }

    #[test]
    fn function_keys_every_way_they_come() {
        let mut d = KeyDecoder::new();
        let f = |n: u8| Key::Function(n);
        // xterm and VT220.
        assert_eq!(
            keys(
                &mut d,
                b"\x1bOP\x1bOS\x1b[15~\x1b[17~\x1b[21~\x1b[23~\x1b[24~"
            ),
            [f(1), f(4), f(5), f(6), f(10), f(11), f(12)]
        );
        // Shifted: F13 to F24.
        assert_eq!(
            keys(&mut d, b"\x1b[1;2P\x1b[15;2~\x1b[24;2~\x1b[25~\x1b[34~"),
            [f(13), f(17), f(24), f(13), f(20)]
        );
        // Esc and the top row, for a terminal with no function keys.
        assert_eq!(
            keys(&mut d, b"\x1b3\x1b0\x1b=\x1b!\x1b+"),
            [f(3), f(10), f(12), f(13), f(24)]
        );
        // Esc on its own is Attention once the line is quiet.
        assert!(keys(&mut d, b"\x1b").is_empty());
        assert!(d.pending());
        assert_eq!(d.flush(), Some(Key::Attention));
        assert!(!d.pending());
        // A sequence nobody knows is swallowed whole.
        assert_eq!(keys(&mut d, b"\x1b[99Xb"), [Key::Char('b')]);
    }
}
