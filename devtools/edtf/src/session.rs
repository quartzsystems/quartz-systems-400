// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One EDTF or DSPF session: the file's lines, where the display shows
//! them from, the commands waiting, find and change, and what each key
//! does. The display's rows are [`Item`]s ([`Session::layout`]); the
//! screen module draws them and reads what was typed back into an
//! [`Input`], which [`Session::key`] applies.
//!
//! The header reads `Record : TOP of TOTAL by SCROLL` and `Column :
//! LEFT RCDLEN by SHIFT`: the record number shown first, the number of
//! records, how many records the page keys move, the first column
//! shown, the record length of the workspace (see
//! [`crate::text::record_length`]) and how many columns F19 and F20
//! move. The goldens give the defaults: EDTF moves 8 records and 74
//! columns, DSPF 14 and 79; each "by" and the two positions are fields
//! the operator may type over.

use crate::buffer::Buffer;
use crate::control::{self, Control};
use crate::line::{self, Placed};
use crate::msg::Msg;
use crate::text::{Eol, Text, record_length, shown};

/// Edit or browse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `EDTF`.
    Edit,
    /// `DSPF`.
    Browse,
}

/// A key that ended a read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Enter.
    Enter,
    /// A function key.
    F(u8),
    /// Page Up (Roll Down).
    PageUp,
    /// Page Down (Roll Up).
    PageDown,
}

/// What one data row had typed into it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RowInput {
    /// The data row, from 0.
    pub row: usize,
    /// The line command typed, when the `CMD` area changed.
    pub cmd: Option<String>,
    /// The text, when the data changed.
    pub data: Option<String>,
}

/// What the operator typed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Input {
    /// The changed data rows.
    pub rows: Vec<RowInput>,
    /// The control field.
    pub control: String,
    /// The record field, when changed.
    pub record: Option<String>,
    /// The record scroll amount, when changed.
    pub rec_by: Option<String>,
    /// The column field, when changed.
    pub column: Option<String>,
    /// The column shift amount, when changed.
    pub col_by: Option<String>,
}

/// What the driver does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Show the display again.
    Stay,
    /// Write the file and show it again.
    Save,
    /// Write the file and end.
    SaveExit,
    /// End without writing.
    Exit,
    /// Show the services display.
    Services,
}

/// What a data row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// `Beginning of data`.
    Begin,
    /// A line by index, and in the hex view which of its rows (0 the
    /// characters, 1 the zone digits, 2 the low digits).
    Line(usize, u8),
    /// `End of Data`.
    End,
    /// Nothing.
    Empty,
}

/// Data rows of the edit display.
pub const EDIT_ROWS: usize = 15;
/// Data rows of the browse display.
pub const BROWSE_ROWS: usize = 16;
/// Columns of data the edit display shows.
pub const EDIT_WIDTH: usize = 75;
/// Columns of data the browse display shows.
pub const BROWSE_WIDTH: usize = 79;

/// A session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Edit or browse.
    pub mode: Mode,
    /// The path shown in the title.
    pub path: String,
    /// The lines.
    pub buf: Buffer,
    /// The file's CCSID.
    pub file_ccsid: u16,
    /// The CCSID the file gets when saved, when the services changed it.
    pub new_ccsid: Option<u16>,
    /// The job's CCSID (what the display shows the text in).
    pub job_ccsid: u16,
    /// The line end a save writes.
    pub eol: Eol,
    /// Whether the last line gets one.
    pub final_eol: bool,
    /// The workspace's record length.
    pub rcdlen: usize,
    /// The record shown first (1 is the Beginning of data line's).
    pub top: usize,
    /// The first column shown, from 1.
    pub left: usize,
    /// Records the page keys move.
    pub rec_by: usize,
    /// Columns F19 and F20 move.
    pub col_by: usize,
    /// The message for the message line.
    pub message: Option<Msg>,
    /// Line commands waiting for their other end or target.
    pub pending: Vec<Placed>,
    /// The last text found.
    pub find: Option<String>,
    /// The last change.
    pub change: Option<(String, String)>,
    /// Where the last find stopped: line and column.
    pub found: Option<(usize, usize)>,
    /// The hex view (DSPF's F10).
    pub hex: bool,
    /// The line the cursor goes to, by number, and the column in it.
    pub cursor: Option<(u64, usize)>,
    /// F12 warned once about unsaved changes.
    warned: bool,
}

impl Session {
    /// A session over a file's text. `size` is the file's length in
    /// bytes (for the record length).
    #[must_use]
    pub fn new(mode: Mode, path: &str, text: Text, file_ccsid: u16, job_ccsid: u16, size: usize) -> Self {
        let longest = text.lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        let (rec_by, col_by) = match mode {
            Mode::Edit => (8, 74),
            Mode::Browse => (14, 79),
        };
        Self {
            mode,
            path: path.to_string(),
            buf: Buffer::new(text.lines),
            file_ccsid,
            new_ccsid: None,
            job_ccsid,
            eol: text.eol,
            final_eol: text.final_eol || text.raw.is_empty(),
            rcdlen: record_length(size, longest),
            top: 1,
            left: 1,
            rec_by,
            col_by,
            message: None,
            pending: Vec::new(),
            find: None,
            change: None,
            found: None,
            hex: false,
            cursor: None,
            warned: false,
        }
    }

    /// Data rows of the display.
    #[must_use]
    pub fn rows(&self) -> usize {
        match self.mode {
            Mode::Edit => EDIT_ROWS,
            Mode::Browse => BROWSE_ROWS,
        }
    }

    /// Columns of data shown.
    #[must_use]
    pub fn width(&self) -> usize {
        match self.mode {
            Mode::Edit => EDIT_WIDTH,
            Mode::Browse => BROWSE_WIDTH,
        }
    }

    /// What each data row shows.
    #[must_use]
    pub fn layout(&self) -> Vec<Item> {
        let n = self.buf.len();
        let mut out = Vec::with_capacity(self.rows());
        let mut v = if self.top <= 1 { 0 } else { self.top };
        while out.len() < self.rows() {
            let item = match v {
                0 => Item::Begin,
                x if x <= n => Item::Line(x - 1, 0),
                x if x == n + 1 => Item::End,
                _ => Item::Empty,
            };
            if let (Item::Line(i, _), true) = (item, self.hex) {
                for sub in 0..3 {
                    if out.len() < self.rows() {
                        out.push(Item::Line(i, sub));
                    }
                }
            } else {
                out.push(item);
            }
            v += 1;
        }
        out
    }

    /// The window of line `i`'s text the display shows, in the job's
    /// CCSID.
    #[must_use]
    pub fn window(&self, i: usize) -> String {
        shown(self.buf.text(i), self.job_ccsid)
            .chars()
            .skip(self.left - 1)
            .take(self.width())
            .collect()
    }

    /// The two hex rows of line `i`'s window: the high and the low
    /// digits of each byte in the file's CCSID.
    #[must_use]
    pub fn hex_rows(&self, i: usize) -> (String, String) {
        let bytes = ccsid::conv::from_str(self.file_ccsid, self.buf.text(i)).unwrap_or_default();
        let digit = |n: u8| char::from_digit(u32::from(n), 16).unwrap_or('0').to_ascii_uppercase();
        let w: Vec<u8> = bytes.into_iter().skip(self.left - 1).take(self.width()).collect();
        (
            w.iter().map(|b| digit(b >> 4)).collect(),
            w.iter().map(|b| digit(b & 15)).collect(),
        )
    }

    /// Line `i` with the window replaced by `typed`.
    fn merged(&self, i: usize, typed: &str) -> String {
        let old: Vec<char> = self.buf.text(i).chars().collect();
        let mut out: Vec<char> = old.iter().take(self.left - 1).copied().collect();
        if !typed.trim().is_empty() || old.len() > self.left - 1 + self.width() {
            while out.len() < self.left - 1 {
                out.push(' ');
            }
        }
        let typed: Vec<char> = typed.chars().collect();
        out.extend(&typed);
        let tail_from = self.left - 1 + self.width();
        if old.len() > tail_from {
            while out.len() < tail_from {
                out.push(' ');
            }
            out.extend(&old[tail_from..]);
        }
        let s: String = out.into_iter().collect();
        s.trim_end().to_string()
    }

    fn say(&mut self, id: &'static str, values: &[&str]) {
        self.message = Some(Msg::new(id, values));
    }

    /// Show record `r` first, within the file.
    fn show_from(&mut self, r: usize) {
        self.top = r.clamp(1, self.buf.len() + 1);
    }

    /// Make line `i` visible.
    fn reveal(&mut self, i: usize) {
        let rec = i + 1;
        let shown = self.rows() - usize::from(self.top == 1);
        if rec < self.top || rec >= self.top + shown {
            self.show_from(rec);
        }
    }

    /// The header fields typed over.
    fn header(&mut self, input: &Input) {
        let num = |s: &str| s.trim().parse::<usize>().ok();
        if let Some(t) = &input.record {
            match num(t) {
                Some(n) => self.show_from(n),
                None if t.trim().is_empty() => self.say("CPDB603", &[]),
                None => self.say("CPDB605", &[]),
            }
        }
        if let Some(t) = &input.rec_by {
            match num(t) {
                Some(n) if n > 0 => self.rec_by = n,
                _ => self.say("CPDB605", &[]),
            }
        }
        if let Some(t) = &input.column {
            match num(t) {
                Some(n) if n > 0 => self.left = n.min(self.rcdlen.max(1)),
                _ => self.say("CPDB605", &[]),
            }
        }
        if let Some(t) = &input.col_by {
            match num(t) {
                Some(n) if n > 0 => self.col_by = n,
                _ => self.say("CPDB605", &[]),
            }
        }
    }

    /// Apply what was typed and the key.
    pub fn key(&mut self, key: Key, input: &Input) -> Action {
        self.message = None;
        if key != Key::F(12) {
            self.warned = false;
        }
        self.header(input);
        let layout = self.layout();
        let id_of = |row: usize, buf: &Buffer| -> Option<u64> {
            match layout.get(row) {
                Some(Item::Begin) => Some(0),
                Some(Item::Line(i, 0)) => buf.lines().get(*i).map(|l| l.id),
                _ => None,
            }
        };
        if self.mode == Mode::Edit {
            // Typed data first, by line number.
            let typed: Vec<(u64, String)> = input
                .rows
                .iter()
                .filter_map(|r| {
                    let d = r.data.as_ref()?;
                    let id = id_of(r.row, &self.buf).filter(|id| *id != 0)?;
                    Some((id, d.clone()))
                })
                .collect();
            let cmds: Vec<(u64, usize, String)> = input
                .rows
                .iter()
                .filter_map(|r| {
                    let c = r.cmd.as_ref().filter(|c| !c.trim().is_empty())?;
                    Some((id_of(r.row, &self.buf)?, r.row, c.clone()))
                })
                .collect();
            for (id, d) in typed {
                if let Some(i) = self.buf.index_of(id) {
                    let m = self.merged(i, &d);
                    self.buf.set(i, m);
                }
            }
            let top_index = self.top.saturating_sub(1);
            let gone = self.buf.drop_fresh(top_index);
            self.top = self.top.saturating_sub(gone).max(1);
            let mut placed = Vec::new();
            let mut bad = false;
            for (id, _, c) in &cmds {
                match line::parse(c) {
                    Some(cmd) => placed.push(Placed { id: *id, cmd }),
                    None => bad = true,
                }
            }
            if bad {
                self.say("CPDB608", &[]);
            } else if !placed.is_empty() || !self.pending.is_empty() {
                let pending = std::mem::take(&mut self.pending);
                let r = line::apply(&mut self.buf, pending, placed);
                self.pending = r.pending;
                if r.needs_target {
                    self.say("CPFB615", &[]);
                }
                if let Some(id) = r.inserted {
                    self.cursor = Some((id, 0));
                    if let Some(i) = self.buf.index_of(id) {
                        self.reveal(i);
                    }
                }
            }
        } else if input.rows.iter().any(|r| r.data.is_some()) {
            self.say("CPIB606", &[]);
        }
        match control::parse(&input.control) {
            Err(id) => self.say(id, &[]),
            Ok(None) => {}
            Ok(Some(c)) => {
                if let Some(a) = self.control(c) {
                    return a;
                }
            }
        }
        self.fkey(key)
    }

    fn control(&mut self, c: Control) -> Option<Action> {
        let edit = self.mode == Mode::Edit;
        match c {
            Control::Top => self.top = 1,
            Control::Bottom => {
                let n = self.buf.len();
                self.top = (n + 2).saturating_sub(self.rows()).max(1);
            }
            Control::Record(r) => self.show_from(r),
            Control::Scroll(d) => {
                let t = i64::try_from(self.top).unwrap_or(1) + d;
                self.show_from(usize::try_from(t.max(1)).unwrap_or(1));
            }
            Control::Find(t) => {
                self.find = Some(t);
                self.found = None;
                self.repeat_find();
            }
            Control::Change { from, to, all } if edit => {
                self.change = Some((from.clone(), to.clone()));
                if all {
                    let mut total = 0;
                    for i in 0..self.buf.len() {
                        let (l, n) = control::replace_all(self.buf.text(i), &from, &to);
                        if n > 0 {
                            self.buf.set(i, l);
                            total += n;
                        }
                    }
                    self.say("CPIB608", &[&total.to_string()]);
                } else {
                    self.found = None;
                    self.repeat_change();
                }
            }
            Control::Save if edit => return Some(Action::Save),
            Control::File if edit => return Some(Action::SaveExit),
            Control::Cancel => return Some(Action::Exit),
            Control::Change { .. } | Control::Save | Control::File => self.say("CPIB606", &[]),
        }
        None
    }

    /// Where the next find starts.
    fn find_from(&self) -> usize {
        match self.found {
            Some((l, _)) => l + 1,
            None => self.top.saturating_sub(1),
        }
    }

    /// F16: the last text found, found again.
    pub fn repeat_find(&mut self) {
        let Some(t) = self.find.clone() else {
            self.say("CPFB612", &[]);
            return;
        };
        let texts = self.buf.texts();
        let n = control::count(&texts, &t);
        match control::find(&texts, &t, self.find_from()) {
            Some((l, c)) => {
                self.found = Some((l, c));
                self.show_from(l + 1);
                self.cursor = Some((self.buf.lines()[l].id, c));
            }
            None => self.found = None,
        }
        self.say("CPIB607", &[&n.to_string(), &t]);
    }

    /// F17: the last change, made at the next occurrence.
    pub fn repeat_change(&mut self) {
        if self.mode == Mode::Browse {
            self.say("CPIB606", &[]);
            return;
        }
        let Some((from, to)) = self.change.clone() else {
            self.say("CPDB617", &[]);
            return;
        };
        let texts = self.buf.texts();
        match control::find(&texts, &from, self.find_from()) {
            Some((l, c)) => {
                let new = control::replace_at(self.buf.text(l), c, &from, &to);
                self.buf.set(l, new);
                self.found = Some((l, c));
                self.show_from(l + 1);
                self.cursor = Some((self.buf.lines()[l].id, c));
                self.say("CPIB608", &["1"]);
            }
            None => self.say("CPIB607", &["0", &from]),
        }
    }

    fn fkey(&mut self, key: Key) -> Action {
        let edit = self.mode == Mode::Edit;
        match key {
            Key::PageDown => {
                let t = self.top + self.rec_by;
                self.show_from(t);
                Action::Stay
            }
            Key::PageUp => {
                self.top = self.top.saturating_sub(self.rec_by).max(1);
                Action::Stay
            }
            Key::F(2) if edit => Action::Save,
            Key::F(3) if edit => Action::SaveExit,
            Key::F(3) => Action::Exit,
            Key::F(12) => {
                if edit && self.buf.changes() > 0 && !self.warned {
                    self.warned = true;
                    self.say("CPIB605", &[]);
                    Action::Stay
                } else {
                    Action::Exit
                }
            }
            Key::F(10) if !edit => {
                self.hex = !self.hex;
                Action::Stay
            }
            Key::F(15) => Action::Services,
            Key::F(16) => {
                self.repeat_find();
                Action::Stay
            }
            Key::F(17) if edit => {
                self.repeat_change();
                Action::Stay
            }
            Key::F(19) => {
                self.left = self.left.saturating_sub(self.col_by).max(1);
                Action::Stay
            }
            Key::F(20) => {
                self.left = (self.left + self.col_by).min(self.rcdlen.max(1));
                Action::Stay
            }
            Key::Enter | Key::F(_) => Action::Stay,
        }
    }

    /// The bytes a save writes: the lines in the file's CCSID (the new
    /// one when the services changed it) with the file's line end.
    /// `None` when the text cannot be encoded.
    #[must_use]
    pub fn bytes(&self) -> Option<Vec<u8>> {
        let ccsid = self.new_ccsid.unwrap_or(self.file_ccsid);
        Text::encode(&self.buf.texts(), self.eol, self.final_eol, ccsid)
    }

    /// The file was written.
    pub fn saved(&mut self) {
        self.buf.saved();
        self.warned = false;
        if let Some(n) = self.new_ccsid.take() {
            let old = self.file_ccsid;
            self.file_ccsid = n;
            if n != old {
                self.say("CPIB612", &[&old.to_string(), &n.to_string()]);
            }
        }
    }

    /// The services display's changes.
    pub fn services(&mut self, ccsid: Option<u16>, eol: Option<Eol>, shift: Option<usize>, scroll: Option<usize>) {
        if let Some(c) = ccsid.filter(|c| *c != self.file_ccsid) {
            if ccsid::conv::Codec::of(c).is_err() {
                self.say("CPDB607", &[&c.to_string()]);
                return;
            }
            self.new_ccsid = Some(c);
            self.buf.touch();
            self.say("CPIB611", &[&c.to_string()]);
        }
        if let Some(e) = eol.filter(|e| *e != self.eol) {
            self.eol = e;
            self.buf.touch();
        }
        if let Some(n) = shift.filter(|n| *n > 0) {
            self.col_by = n;
        }
        if let Some(n) = scroll.filter(|n| *n > 0) {
            self.rec_by = n;
        }
    }

    /// The display row (from 0) of the line numbered `id`, if shown.
    #[must_use]
    pub fn row_of(&self, id: u64) -> Option<usize> {
        let i = self.buf.index_of(id)?;
        self.layout()
            .iter()
            .position(|it| matches!(it, Item::Line(x, 0) if *x == i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(mode: Mode) -> Session {
        let bytes = b"Quartz IFS probe\r\nline two\r\n\tline three after a tab\r\n";
        Session::new(mode, "/p", Text::decode(bytes, 819).unwrap(), 819, 37, bytes.len())
    }

    #[test]
    fn layout_and_header_values() {
        let s = probe(Mode::Edit);
        let l = s.layout();
        assert_eq!(l[0], Item::Begin);
        assert_eq!(l[1], Item::Line(0, 0));
        assert_eq!(l[4], Item::End);
        assert_eq!(l.len(), EDIT_ROWS);
        assert_eq!((s.rcdlen, s.rec_by, s.col_by), (59, 8, 74));
        assert_eq!(s.window(2), " line three after a tab");
    }

    #[test]
    fn typing_and_saving() {
        let mut s = probe(Mode::Edit);
        let input = Input {
            rows: vec![RowInput {
                row: 2,
                data: Some("line 2".into()),
                cmd: None,
            }],
            ..Input::default()
        };
        assert_eq!(s.key(Key::F(2), &input), Action::Save);
        assert_eq!(
            s.bytes().unwrap(),
            b"Quartz IFS probe\r\nline 2\r\n\tline three after a tab\r\n"
        );
        s.saved();
        assert_eq!(s.buf.changes(), 0);
    }

    #[test]
    fn f12_warns_once() {
        let mut s = probe(Mode::Edit);
        let typed = Input {
            rows: vec![RowInput { row: 1, data: Some("x".into()), cmd: None }],
            ..Input::default()
        };
        assert_eq!(s.key(Key::Enter, &typed), Action::Stay);
        assert_eq!(s.key(Key::F(12), &Input::default()), Action::Stay);
        assert_eq!(s.message.as_ref().unwrap().id, "CPIB605");
        assert_eq!(s.key(Key::F(12), &Input::default()), Action::Exit);
    }

    #[test]
    fn insert_from_the_top_and_unused_lines_go() {
        let mut s = probe(Mode::Edit);
        let i = Input {
            rows: vec![RowInput { row: 0, cmd: Some("i2".into()), data: None }],
            ..Input::default()
        };
        s.key(Key::Enter, &i);
        assert_eq!(s.buf.len(), 5);
        let id = s.cursor.unwrap().0;
        assert_eq!(s.row_of(id), Some(1));
        let typed = Input {
            rows: vec![RowInput { row: 1, data: Some("new".into()), cmd: None }],
            ..Input::default()
        };
        s.key(Key::Enter, &typed);
        assert_eq!(s.buf.texts()[0], "new");
        assert_eq!(s.buf.len(), 4);
    }

    #[test]
    fn find_change_scroll() {
        let mut s = probe(Mode::Edit);
        s.key(Key::Enter, &Input { control: "f three".into(), ..Input::default() });
        assert_eq!(s.found, Some((2, 6)));
        assert_eq!(s.top, 3);
        s.key(Key::Enter, &Input { control: "c two 2 all".into(), ..Input::default() });
        assert_eq!(s.buf.text(1), "line 2");
        s.key(Key::Enter, &Input { control: "t".into(), ..Input::default() });
        assert_eq!(s.top, 1);
        s.key(Key::F(20), &Input::default());
        assert_eq!(s.left, 59);
        s.key(Key::F(19), &Input::default());
        assert_eq!(s.left, 1);
    }

    #[test]
    fn browse_does_not_change() {
        let mut s = probe(Mode::Browse);
        s.key(Key::Enter, &Input { control: "c a b".into(), ..Input::default() });
        assert_eq!(s.message.as_ref().unwrap().id, "CPIB606");
        assert_eq!(s.key(Key::F(10), &Input::default()), Action::Stay);
        assert!(s.hex);
        assert_eq!(s.layout()[1], Item::Line(0, 0));
        assert_eq!(s.layout()[2], Item::Line(0, 1));
        let (hi, lo) = s.hex_rows(1);
        assert_eq!((&hi[..2], &lo[..2]), ("66", "C9"));
        assert_eq!(s.key(Key::F(3), &Input::default()), Action::Exit);
    }
}
