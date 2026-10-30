// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSH terminal session's display, QSH Command Entry, as pure
//! logic: the scrolling output area, the command line and the function
//! keys. The driver (`work/qsjob/src/devtools/qsh.rs`) feeds the shell's
//! output in with [`Term::output`], shows [`Term::screen`] and hands the
//! key back to [`Term::key`], which says what to do.
//!
//! The layout is IBM's (`tools/conformance/golden/phase10/misc/qsh-*`):
//! the title on row 1, the output from row 3 at column 4, fourteen rows
//! of it, the `===>` line on row 18 with a four-row input field, and two
//! rows of keys. The keys shown are F3, F6, F9, F12, F13, F17, F18 and
//! F21; F5 (refresh), F7 and F8 (page), F11 (line wrap) work too, as on
//! IBM's display.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target};

/// The title.
pub const TITLE: &str = "QSH Command Entry";
/// The first row of keys.
pub const KEYS1: &str = "F3=Exit   F6=Print F9=Retrieve F12=Disconnect";
/// The second row of keys.
pub const KEYS2: &str = "F13=Clear F17=Top  F18=Bottom  F21=CL command entry";
/// The output area's first row.
pub const FIRST_ROW: usize = 3;
/// Rows of output shown.
pub const ROWS: usize = 14;
/// The output's column.
pub const COL: usize = 4;
/// Characters of output a row shows.
pub const WIDTH: usize = 76;
/// The command line's row.
pub const CMD_ROW: usize = 18;
/// The command field's column.
pub const CMD_COL: usize = 7;
/// The command field's length: the rest of row 18 and rows 19 to 21,
/// each up to column 79.
pub const CMD_LEN: usize = 73 + 3 * 79;
/// The command field's name.
pub const CMD_VAR: &str = "QSHCMD";
/// Commands kept for F9.
const HISTORY: usize = 50;
/// Lines of output kept.
const KEEP: usize = 10_000;

/// A key the display sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Enter.
    Enter,
    /// A function key.
    F(u8),
    /// Page Up (Roll Down).
    PageUp,
    /// Page Down (Roll Up).
    PageDown,
    /// Anything else.
    Other,
}

/// What the driver does next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Give the line to the shell.
    Run(String),
    /// F3: end the shell and leave.
    Exit,
    /// F12: leave; the shell stays for the next `STRQSH`.
    Disconnect,
    /// F6: print the output.
    Print,
    /// F21: a CL command line.
    ClEntry,
    /// Show the display again.
    Show,
}

/// The session's display state.
#[derive(Clone, Debug)]
pub struct Term {
    lines: Vec<String>,
    partial: String,
    /// The first output row shown; `None` follows the end.
    top: Option<usize>,
    wrap: bool,
    history: Vec<String>,
    recall: usize,
    input: String,
    message: Option<String>,
}

impl Default for Term {
    fn default() -> Self {
        Self::new()
    }
}

fn expand_tabs(s: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        if c == '\t' {
            let to = (n / 8 + 1) * 8;
            while n < to {
                out.push(' ');
                n += 1;
            }
        } else if !c.is_control() {
            out.push(c);
            n += 1;
        }
    }
    out
}

impl Term {
    /// An empty session display, wrapping long lines.
    #[must_use]
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            partial: String::new(),
            top: None,
            wrap: true,
            history: Vec::new(),
            recall: 0,
            input: String::new(),
            message: None,
        }
    }

    /// Output from the shell.
    pub fn output(&mut self, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for (i, piece) in text.split('\n').enumerate() {
            if i > 0 {
                let line = std::mem::take(&mut self.partial);
                self.lines.push(expand_tabs(line.trim_end_matches('\r')));
            }
            self.partial.push_str(piece);
        }
        if self.lines.len() > KEEP {
            let cut = self.lines.len() - KEEP;
            self.lines.drain(..cut);
            self.top = self.top.map(|t| t.saturating_sub(cut));
        }
    }

    /// The output lines, the last one even without its newline.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut v = self.lines.clone();
        if !self.partial.is_empty() {
            v.push(expand_tabs(&self.partial));
        }
        v
    }

    /// The display rows the output makes: long lines wrapped, or cut
    /// when wrapping is off (F11).
    #[must_use]
    pub fn rows(&self) -> Vec<String> {
        let mut out = Vec::new();
        for l in self.lines() {
            let chars: Vec<char> = l.chars().collect();
            if chars.is_empty() {
                out.push(String::new());
            } else if self.wrap {
                for c in chars.chunks(WIDTH) {
                    out.push(c.iter().collect());
                }
            } else {
                out.push(chars.iter().take(WIDTH).collect());
            }
        }
        out
    }

    /// Whether long lines wrap.
    #[must_use]
    pub fn wraps(&self) -> bool {
        self.wrap
    }

    /// The command line's text, kept between displays.
    pub fn set_input(&mut self, s: &str) {
        self.input = s.to_string();
    }

    /// A message for row 24 at the next display.
    pub fn set_message(&mut self, m: &str) {
        self.message = Some(m.to_string());
    }

    fn first_shown(&self, total: usize) -> usize {
        let last = total.saturating_sub(ROWS);
        self.top.map_or(last, |t| t.min(last))
    }

    /// The display.
    #[must_use]
    pub fn screen(&self) -> Screen {
        let mut s = Screen::new(24, 80);
        s.put(1, 32, Attr::HighIntensity, TITLE);
        let rows = self.rows();
        let first = self.first_shown(rows.len());
        for (i, r) in rows.iter().skip(first).take(ROWS).enumerate() {
            s.put(FIRST_ROW + i, COL, Attr::Normal, r);
        }
        s.put(CMD_ROW, 2, Attr::Normal, "===>");
        s.field(Field {
            row: CMD_ROW,
            col: CMD_COL,
            len: CMD_LEN,
            attr: Attr::Underline,
            value: self.input.clone(),
            target: Target::Var(CMD_VAR.into()),
            usage: Usage::Inout,
            shift: Shift::None,
            required: false,
            autoskip: false,
        });
        s.put(22, 2, Attr::Blue, KEYS1);
        s.put(23, 2, Attr::Blue, KEYS2);
        if let Some(m) = &self.message {
            s.put(24, 2, Attr::HighIntensity, m);
        }
        s.cursor = Some((CMD_ROW, CMD_COL));
        s
    }

    /// A key with the command line as typed: what to do. The message
    /// shown goes with any key.
    pub fn key(&mut self, key: Key, typed: &str) -> Action {
        self.message = None;
        let typed = typed.trim_end();
        let rows = self.rows().len();
        match key {
            Key::Enter => {
                self.input.clear();
                self.top = None;
                self.recall = 0;
                if !typed.trim().is_empty() {
                    if self.history.last().map(String::as_str) != Some(typed) {
                        self.history.push(typed.to_string());
                    }
                    if self.history.len() > HISTORY {
                        self.history.remove(0);
                    }
                }
                return Action::Run(typed.to_string());
            }
            Key::F(3) => return Action::Exit,
            Key::F(12) => return Action::Disconnect,
            Key::F(6) => {
                self.input = typed.to_string();
                return Action::Print;
            }
            Key::F(21) => {
                self.input = typed.to_string();
                return Action::ClEntry;
            }
            Key::F(9) => {
                if self.history.is_empty() {
                    self.input = typed.to_string();
                } else {
                    self.recall = self.recall % self.history.len() + 1;
                    let i = self.history.len() - self.recall;
                    self.input.clone_from(&self.history[i]);
                }
                return Action::Show;
            }
            Key::F(7) | Key::PageUp => {
                let first = self.first_shown(rows);
                self.top = Some(first.saturating_sub(ROWS));
            }
            Key::F(8) | Key::PageDown => {
                let first = self.first_shown(rows) + ROWS;
                self.top = if first + ROWS >= rows { None } else { Some(first) };
            }
            Key::F(11) => self.wrap = !self.wrap,
            Key::F(13) => {
                self.lines.clear();
                self.partial.clear();
                self.top = None;
            }
            Key::F(17) => self.top = Some(0),
            Key::F(18) => self.top = None,
            _ => {}
        }
        self.input = typed.to_string();
        Action::Show
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term_with(n: usize) -> Term {
        let mut t = Term::new();
        for i in 0..n {
            t.output(format!("line {i}\n").as_bytes());
        }
        t
    }

    fn shown(t: &Term) -> Vec<String> {
        t.screen().text_rows()[FIRST_ROW - 1..FIRST_ROW - 1 + ROWS]
            .iter()
            .map(|r| r.trim().to_string())
            .collect()
    }

    #[test]
    fn output_follows_the_end_and_pages() {
        let mut t = term_with(40);
        assert_eq!(shown(&t)[ROWS - 1], "line 39");
        t.key(Key::F(7), "");
        assert_eq!(shown(&t)[ROWS - 1], "line 25");
        t.key(Key::F(17), "");
        assert_eq!(shown(&t)[0], "line 0");
        t.key(Key::F(8), "");
        assert_eq!(shown(&t)[0], "line 14");
        t.key(Key::F(18), "");
        assert_eq!(shown(&t)[ROWS - 1], "line 39");
        t.key(Key::F(13), "");
        assert!(shown(&t).iter().all(String::is_empty));
    }

    #[test]
    fn enter_runs_and_f9_retrieves() {
        let mut t = Term::new();
        assert_eq!(t.key(Key::Enter, "ls /   "), Action::Run("ls /".into()));
        assert_eq!(t.key(Key::Enter, "pwd"), Action::Run("pwd".into()));
        assert_eq!(t.key(Key::F(9), ""), Action::Show);
        assert!(t.screen().text_rows()[CMD_ROW - 1].contains("pwd"));
        t.key(Key::F(9), "");
        assert!(t.screen().text_rows()[CMD_ROW - 1].contains("ls /"));
        assert_eq!(t.key(Key::F(3), ""), Action::Exit);
        assert_eq!(t.key(Key::F(12), ""), Action::Disconnect);
        assert_eq!(t.key(Key::F(21), "x"), Action::ClEntry);
        assert_eq!(t.key(Key::F(6), ""), Action::Print);
    }

    #[test]
    fn long_lines_wrap_or_cut() {
        let mut t = Term::new();
        t.output(format!("{}\n", "x".repeat(100)).as_bytes());
        assert_eq!(t.rows().len(), 2);
        t.key(Key::F(11), "");
        assert_eq!(t.rows().len(), 1);
        assert!(!t.wraps());
    }
}
