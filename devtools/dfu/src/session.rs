// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A data entry session: its mode, what `F9`, `F10`, `F11` and the roll
//! keys do in it, and the counts End Data Entry shows.

/// The mode on the title line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Records found by key (or record number) and changed.
    Change,
    /// New records added at the end.
    Entry,
    /// New records added among the others (a keyed file).
    Insert,
    /// Records shown, nothing changed (`DSPDTA`).
    Display,
}

impl Mode {
    /// The word the title line shows.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Change => "CHANGE",
            Self::Entry => "ENTRY",
            Self::Insert => "INSERT",
            Self::Display => "DISPLAY",
        }
    }

    /// Whether the display asks for a new record's fields.
    #[must_use]
    pub fn adds(self) -> bool {
        matches!(self, Self::Entry | Self::Insert)
    }
}

/// What a key asks of the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Show the mode's first display.
    Restart,
    /// The End Data Entry display.
    End,
    /// The format list.
    Formats,
    /// Read the next record (Page Down).
    Next,
    /// Read the one before (Page Up).
    Previous,
    /// Enter: look up the key, or write the record shown.
    Enter,
    /// Delete the record shown.
    Delete,
    /// The key is not used here.
    Invalid,
}

/// A session over one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The mode.
    pub mode: Mode,
    /// Records added.
    pub added: u32,
    /// Records changed.
    pub changed: u32,
    /// Records deleted.
    pub deleted: u32,
    /// Only display: `DSPDTA`.
    pub display_only: bool,
}

impl Session {
    /// A session: change mode, or display mode for `DSPDTA`.
    #[must_use]
    pub fn new(display_only: bool) -> Self {
        Self {
            mode: if display_only {
                Mode::Display
            } else {
                Mode::Change
            },
            added: 0,
            changed: 0,
            deleted: 0,
            display_only,
        }
    }

    /// A function key (or 0 for Enter, 100 and 101 for Page Down and
    /// Up) with a record shown or not.
    pub fn key(&mut self, f: u8, shown: bool) -> Step {
        match f {
            0 => Step::Enter,
            3 => Step::End,
            5 => Step::Restart,
            6 => Step::Formats,
            9 if !self.display_only => {
                self.mode = Mode::Insert;
                Step::Restart
            }
            10 if !self.display_only => {
                self.mode = Mode::Entry;
                Step::Restart
            }
            11 if !self.display_only => {
                self.mode = Mode::Change;
                Step::Restart
            }
            23 if shown && self.mode == Mode::Change => Step::Delete,
            100 if !self.mode.adds() => Step::Next,
            101 if !self.mode.adds() => Step::Previous,
            _ => Step::Invalid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        let mut s = Session::new(false);
        assert_eq!(s.mode, Mode::Change);
        assert_eq!(s.key(10, false), Step::Restart);
        assert_eq!(s.mode.word(), "ENTRY");
        assert_eq!(s.key(100, false), Step::Invalid);
        assert_eq!(s.key(9, false), Step::Restart);
        assert_eq!(s.mode, Mode::Insert);
        assert_eq!(s.key(11, false), Step::Restart);
        assert_eq!(s.key(23, true), Step::Delete);
        assert_eq!(s.key(24, true), Step::Invalid);
        let mut d = Session::new(true);
        assert_eq!(d.key(10, false), Step::Invalid);
        assert_eq!(d.key(23, true), Step::Invalid);
        assert_eq!(d.key(100, true), Step::Next);
    }
}
