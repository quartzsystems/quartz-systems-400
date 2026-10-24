// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keyboard, run on the station: typing, field exits, erasing,
//! cursor movement, and the attention keys that send a record.

use super::{Read, Terminal};
use crate::ds::field::{Adjust, Shift};
use crate::ds::{DUP, FIELD_MARK, NULL, aid};
use crate::record::flags;

/// A key of the 5250 keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A character.
    Char(char),
    /// Enter.
    Enter,
    /// `F1`..`F24`.
    F(u8),
    /// Page Up (Roll Down).
    PageUp,
    /// Page Down (Roll Up).
    PageDown,
    /// Help.
    Help,
    /// Clear.
    Clear,
    /// Print.
    Print,
    /// Attention.
    Attn,
    /// System Request.
    SysReq,
    /// Test Request.
    TestReq,
    /// Reset (Error Reset).
    Reset,
    /// Tab: the next field.
    Tab,
    /// Back Tab: the previous field.
    BackTab,
    /// Field Exit.
    FieldExit,
    /// Field +.
    FieldPlus,
    /// Field −.
    FieldMinus,
    /// Erase to the end of the field.
    EraseEof,
    /// Erase every input field.
    EraseInput,
    /// Home.
    Home,
    /// Insert mode toggle.
    Insert,
    /// Delete the character under the cursor.
    Delete,
    /// Backspace: the cursor left.
    Backspace,
    /// Dup.
    Dup,
    /// Field Mark.
    FieldMark,
    /// Cursor left.
    Left,
    /// Cursor right.
    Right,
    /// Cursor up.
    Up,
    /// Cursor down.
    Down,
    /// New Line.
    NewLine,
    /// Record Backspace.
    RecordBackspace,
}

/// What a key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyResult {
    /// Handled on the station.
    Done,
    /// A record went to the outbox; the keyboard is locked.
    Sent,
    /// The keyboard is locked (or inhibited by an error): ignored.
    Locked,
    /// An operator error: the keyboard is inhibited until Reset.
    Error(u16),
}

/// Operator error codes.
pub mod err {
    /// Cursor in a protected area.
    pub const PROTECTED: u16 = 5;
    /// A mandatory-entry field is empty.
    pub const MANDATORY_ENTRY: u16 = 7;
    /// Alphabetic only.
    pub const ALPHA_ONLY: u16 = 8;
    /// Numeric only.
    pub const NUMERIC_ONLY: u16 = 9;
    /// Digits only in a signed numeric field.
    pub const SIGNED_DIGITS: u16 = 10;
    /// Insert with no room.
    pub const NO_ROOM: u16 = 12;
    /// A mandatory-fill field is partly filled.
    pub const MANDATORY_FILL: u16 = 14;
    /// Dup not allowed here.
    pub const DUP_NOT_ALLOWED: u16 = 19;
    /// Field Exit required.
    pub const FIELD_EXIT_REQUIRED: u16 = 20;
    /// Field − in a field that is not signed numeric.
    pub const FIELD_MINUS: u16 = 26;
}

impl Terminal {
    /// Press a key.
    #[allow(clippy::too_many_lines)] // one arm per key
    pub fn key(&mut self, k: Key) -> KeyResult {
        if k == Key::Reset {
            self.error = None;
            self.insert = false;
            return KeyResult::Done;
        }
        if self.error.is_some() {
            return KeyResult::Locked;
        }
        match k {
            Key::Attn => {
                self.send_flags(flags::ATN);
                return KeyResult::Sent;
            }
            Key::SysReq => {
                self.send_flags(flags::SRQ);
                return KeyResult::Sent;
            }
            Key::TestReq => {
                self.send_flags(flags::TRQ);
                return KeyResult::Sent;
            }
            _ => {}
        }
        if self.locked {
            return KeyResult::Locked;
        }
        match k {
            Key::Char(c) => self.type_char(c),
            Key::Enter => self.aid_key(aid::ENTER, true),
            Key::F(n) => {
                // A command attention key brings no field data.
                let data = !(1..=24).contains(&n)
                    || self.key_switches[usize::from(2 - (n - 1) / 8)] & (1 << ((n - 1) % 8)) == 0;
                self.aid_key(aid::function(n), data)
            }
            Key::PageUp => self.aid_key(aid::ROLL_DOWN, true),
            Key::PageDown => self.aid_key(aid::ROLL_UP, true),
            Key::Help => self.aid_key(aid::HELP, false),
            Key::Clear => self.aid_key(aid::CLEAR, false),
            Key::Print => self.aid_key(aid::PRINT, false),
            Key::RecordBackspace => self.aid_key(aid::RECORD_BACKSPACE, false),
            Key::Tab => {
                self.next_field();
                KeyResult::Done
            }
            Key::BackTab => {
                self.previous_field();
                KeyResult::Done
            }
            Key::FieldExit => self.field_exit(None),
            Key::FieldPlus => self.field_exit(Some(false)),
            Key::FieldMinus => self.field_exit(Some(true)),
            Key::EraseEof => self.erase_eof(),
            Key::EraseInput => {
                for i in 0..self.fields.len() {
                    if self.fields[i].ffw.bypass {
                        continue;
                    }
                    let (s, e) = (self.fields[i].start, self.fields[i].end());
                    for a in s..e {
                        self.cells[a] = NULL;
                    }
                    self.fields[i].ffw.mdt = false;
                }
                self.go_home();
                KeyResult::Done
            }
            Key::Home => {
                self.go_home();
                KeyResult::Done
            }
            Key::Insert => {
                self.insert = !self.insert;
                KeyResult::Done
            }
            Key::Delete => self.delete(),
            Key::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                KeyResult::Done
            }
            Key::Dup => self.dup(),
            Key::FieldMark => self.type_byte(FIELD_MARK),
            Key::Reset | Key::Attn | Key::SysReq | Key::TestReq => KeyResult::Done,
            Key::Left => {
                self.cursor = (self.cursor + self.cells.len() - 1) % self.cells.len();
                KeyResult::Done
            }
            Key::Right => {
                self.cursor = (self.cursor + 1) % self.cells.len();
                KeyResult::Done
            }
            Key::Up => {
                self.cursor = (self.cursor + self.cells.len() - self.cols) % self.cells.len();
                KeyResult::Done
            }
            Key::Down => {
                self.cursor = (self.cursor + self.cols) % self.cells.len();
                KeyResult::Done
            }
            Key::NewLine => {
                let row = self.cursor / self.cols;
                let next = ((row + 1) % self.rows) * self.cols;
                self.cursor = self
                    .fields
                    .iter()
                    .find(|f| !f.ffw.bypass && f.start >= next)
                    .map_or(next, |f| f.start);
                KeyResult::Done
            }
        }
    }

    fn fail(&mut self, code: u16) -> KeyResult {
        self.error = Some(code);
        KeyResult::Error(code)
    }

    fn aid_key(&mut self, a: u8, with_fields: bool) -> KeyResult {
        if a == aid::ENTER
            && let Some(code) = self.check_fields()
        {
            return self.fail(code);
        }
        self.send_aid(a, with_fields);
        KeyResult::Sent
    }

    /// Mandatory entry and mandatory fill, checked before Enter.
    fn check_fields(&mut self) -> Option<u16> {
        self.read?;
        for f in &self.fields {
            if f.ffw.bypass {
                continue;
            }
            let cells = &self.cells[f.start..f.end()];
            let empty = cells.iter().all(|&b| b == NULL || b == 0x40);
            if f.ffw.mandatory_entry && empty {
                self.cursor = f.start;
                return Some(err::MANDATORY_ENTRY);
            }
            if f.ffw.adjust == Adjust::MandatoryFill && !empty && cells.contains(&NULL) {
                self.cursor = f.start;
                return Some(err::MANDATORY_FILL);
            }
        }
        None
    }

    fn go_home(&mut self) {
        self.cursor = self
            .home
            .or_else(|| self.fields.iter().find(|f| !f.ffw.bypass).map(|f| f.start))
            .unwrap_or(0);
    }

    /// The next input field after the cursor, wrapping.
    fn next_field(&mut self) {
        let cur = self.cursor;
        let next = self
            .fields
            .iter()
            .find(|f| !f.ffw.bypass && f.start > cur && !f.contains(cur))
            .or_else(|| self.fields.iter().find(|f| !f.ffw.bypass));
        if let Some(f) = next {
            self.cursor = f.start;
        }
    }

    fn previous_field(&mut self) {
        let cur = self.cursor;
        if let Some(i) = self.field_at(cur)
            && self.fields[i].start < cur
        {
            self.cursor = self.fields[i].start;
            return;
        }
        let prev = self
            .fields
            .iter()
            .rev()
            .find(|f| !f.ffw.bypass && f.start < cur)
            .or_else(|| self.fields.iter().rev().find(|f| !f.ffw.bypass));
        if let Some(f) = prev {
            self.cursor = f.start;
        }
    }

    fn type_char(&mut self, c: char) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        let f = self.fields[i].clone();
        if f.ffw.bypass {
            return self.fail(err::PROTECTED);
        }
        let c = if f.ffw.monocase {
            c.to_ascii_uppercase()
        } else {
            c
        };
        let ok = match f.ffw.shift {
            Shift::AlphaOnly => c.is_ascii_alphabetic() || matches!(c, ' ' | ',' | '.' | '-'),
            Shift::NumericOnly => c.is_ascii_digit() || matches!(c, ' ' | '+' | '-' | '.' | ','),
            Shift::DigitsOnly | Shift::SignedNumeric => c.is_ascii_digit(),
            _ => true,
        };
        if !ok {
            return self.fail(match f.ffw.shift {
                Shift::AlphaOnly => err::ALPHA_ONLY,
                Shift::SignedNumeric => err::SIGNED_DIGITS,
                _ => err::NUMERIC_ONLY,
            });
        }
        let Some(b) = ccsid::byte_of(c) else {
            return KeyResult::Done;
        };
        self.type_byte(b)
    }

    fn type_byte(&mut self, b: u8) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        let f = self.fields[i].clone();
        // A signed numeric field keeps its last position for the sign.
        let last = if f.ffw.shift == Shift::SignedNumeric {
            f.end() - 1
        } else {
            f.end()
        };
        if self.cursor >= last {
            return self.fail(err::FIELD_EXIT_REQUIRED);
        }
        if self.insert {
            if self.cells[last - 1] != NULL && self.cells[last - 1] != 0x40 {
                return self.fail(err::NO_ROOM);
            }
            for a in (self.cursor + 1..last).rev() {
                self.cells[a] = self.cells[a - 1];
            }
        }
        self.cells[self.cursor] = b;
        self.fields[i].ffw.mdt = true;
        self.cursor += 1;
        if self.cursor >= last {
            if f.ffw.auto_enter {
                self.cursor = f.start;
                return self.aid_key(aid::AUTO_ENTER, true);
            }
            if f.ffw.field_exit_required {
                self.cursor = last - 1;
            } else {
                self.next_field();
            }
        }
        KeyResult::Done
    }

    #[allow(clippy::many_single_char_names)] // field, start, end
    fn field_exit(&mut self, sign: Option<bool>) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        let f = self.fields[i].clone();
        if f.ffw.bypass {
            return self.fail(err::PROTECTED);
        }
        if sign == Some(true) && !matches!(f.ffw.shift, Shift::SignedNumeric | Shift::NumericOnly) {
            return self.fail(err::FIELD_MINUS);
        }
        let (s, e) = (f.start, f.end());
        for a in self.cursor..e {
            self.cells[a] = NULL;
        }
        let typed: Vec<u8> = self.cells[s..e]
            .iter()
            .copied()
            .filter(|&b| b != NULL)
            .collect();
        match (f.ffw.adjust, f.ffw.shift) {
            (Adjust::MandatoryFill, _) => {
                if !typed.is_empty() && typed.len() < f.len {
                    return self.fail(err::MANDATORY_FILL);
                }
            }
            (Adjust::RightZero | Adjust::RightBlank, _) => {
                let fill = if f.ffw.adjust == Adjust::RightZero {
                    0xF0
                } else {
                    0x40
                };
                let keep = if f.ffw.shift == Shift::SignedNumeric {
                    f.len - 1
                } else {
                    f.len
                };
                let mut out = vec![fill; keep];
                let n = typed.len().min(keep);
                out[keep - n..].copy_from_slice(&typed[typed.len() - n..]);
                self.cells[s..s + keep].copy_from_slice(&out);
            }
            (Adjust::None, Shift::SignedNumeric) => {
                let keep = f.len - 1;
                let mut out = vec![0x40; keep];
                let n = typed.len().min(keep);
                out[keep - n..].copy_from_slice(&typed[typed.len() - n..]);
                self.cells[s..s + keep].copy_from_slice(&out);
            }
            _ => {}
        }
        if f.ffw.shift == Shift::SignedNumeric {
            self.cells[e - 1] = if sign == Some(true) { 0x60 } else { 0x40 };
        } else if sign == Some(true)
            && let Some(last) = self.cells[s..e]
                .iter()
                .rposition(|&b| b != NULL && b != 0x40)
        {
            // Field minus on a numeric-only field: the last digit zoned
            // negative.
            self.cells[s + last] = (self.cells[s + last] & 0x0F) | 0xD0;
        }
        self.fields[i].ffw.mdt = true;
        if f.ffw.auto_enter {
            return self.aid_key(aid::AUTO_ENTER, true);
        }
        self.cursor = e - 1;
        self.next_field();
        KeyResult::Done
    }

    fn erase_eof(&mut self) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        if self.fields[i].ffw.bypass {
            return self.fail(err::PROTECTED);
        }
        let e = self.fields[i].end();
        for a in self.cursor..e {
            self.cells[a] = NULL;
        }
        self.fields[i].ffw.mdt = true;
        KeyResult::Done
    }

    fn delete(&mut self) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        if self.fields[i].ffw.bypass {
            return self.fail(err::PROTECTED);
        }
        let e = self.fields[i].end();
        for a in self.cursor..e - 1 {
            self.cells[a] = self.cells[a + 1];
        }
        self.cells[e - 1] = NULL;
        self.fields[i].ffw.mdt = true;
        KeyResult::Done
    }

    fn dup(&mut self) -> KeyResult {
        let Some(i) = self.field_at(self.cursor) else {
            return self.fail(err::PROTECTED);
        };
        let f = self.fields[i].clone();
        if f.ffw.bypass {
            return self.fail(err::PROTECTED);
        }
        if !f.ffw.dup {
            return self.fail(err::DUP_NOT_ALLOWED);
        }
        for a in self.cursor..f.end() {
            self.cells[a] = DUP;
        }
        self.fields[i].ffw.mdt = true;
        self.cursor = f.end() - 1;
        self.next_field();
        KeyResult::Done
    }

    /// Type a string, character by character; stops at the first key
    /// that is not accepted.
    pub fn type_str(&mut self, s: &str) -> KeyResult {
        for c in s.chars() {
            match self.key(Key::Char(c)) {
                KeyResult::Done => {}
                other => return other,
            }
        }
        KeyResult::Done
    }

    /// Put the cursor at a 1-based row and column.
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        if row >= 1 && row <= self.rows && col >= 1 && col <= self.cols {
            self.cursor = (row - 1) * self.cols + (col - 1);
        }
    }

    /// The read outstanding, for tests and traces.
    pub fn read_outstanding(&self) -> Option<Read> {
        self.read
    }
}
