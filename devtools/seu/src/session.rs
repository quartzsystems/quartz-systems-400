// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An editing session: the member, what the display shows of it (the
//! window of columns, the first line, excluded records, insert lines,
//! format lines, the prompt), and what each key does. The driver hands
//! in what the operator typed, row by row, and gets back what to do
//! next; it draws [`Session::view`].

mod apply;
mod view;

use std::collections::HashSet;

use crate::buffer::Buffer;
use crate::cmdline::SaveTo;
use crate::defaults::Defaults;
use crate::find::{FindOptions, Hit};
use crate::line::Placed;
use crate::srctype::SourceType;

pub use apply::Key;
pub use view::{Header, PromptView, Row, RowKind, View};

/// Browse (`OPTION(5)`) or edit (`OPTION(2)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Changes allowed.
    Edit,
    /// Read only.
    Browse,
}

/// The first row of records, under the format line.
pub const FIRST_ROW: usize = 4;
/// The last row of records with the key lines shown (row 21 stays
/// blank above them).
pub const LAST_ROW: usize = 20;
/// The first screen column of record data.
pub const DATA_COL: usize = 9;
/// The widest window of data.
pub const WINDOW: usize = 71;
/// How far `F19`/`F20` move the window.
pub const SHIFT: usize = 36;
/// Where `F11` moves the window.
pub const TOGGLE_COL: usize = 31;
/// The beginning-of-data line's identifier in line commands.
pub const BEGIN_ID: u64 = 0;
/// The end-of-data line's identifier.
pub const END_ID: u64 = u64::MAX;

/// A line of the display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// `Beginning of data`.
    Begin,
    /// A record.
    Rec(u64),
    /// Excluded records, shown as one line.
    Excl(Vec<u64>),
    /// An insert line, by its identifier.
    Ins(u64),
    /// A format line before a record (or at the end).
    Fmt(u64, String),
    /// `End of data`.
    End,
}

/// An insert line: where it is and what was typed in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InsLine {
    /// Its identifier (from a range of its own).
    pub id: u64,
    /// The record it follows; `BEGIN_ID` for the top.
    pub after: u64,
}

/// What the prompt is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A record.
    Rec(u64),
    /// An insert line.
    Ins(u64),
}

/// A fixed-form prompt in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptState {
    /// The line prompted.
    pub target: Target,
    /// The format.
    pub format: String,
}

/// What the driver does after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Show the session again.
    Show,
    /// Show the Exit display.
    Exit,
    /// End without saving.
    Cancel,
    /// Write the member and go on.
    Save(SaveTo),
    /// Write the member and end.
    File(SaveTo),
    /// Prompt a CL or command definition line with the command
    /// prompter: the line and its text.
    ClPrompt(Target, String),
    /// The session defaults display.
    Defaults,
    /// The find and change options display.
    FindOptions,
    /// The browse and copy options display.
    BrowseOptions,
    /// The system command window.
    SystemCommand,
    /// Select Prompt for a line.
    SelectPrompt(Target),
}

/// One row as the operator left it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowInput {
    /// The screen row.
    pub row: usize,
    /// The sequence field's text.
    pub seq: String,
    /// The data field's text.
    pub data: String,
}

/// What the display sent back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// The command line.
    pub cmdline: String,
    /// The rows of records.
    pub rows: Vec<RowInput>,
    /// The cursor.
    pub cursor: (usize, usize),
    /// The prompt's fields, when a prompt is shown.
    pub prompt: Vec<String>,
}

/// The session.
#[derive(Clone, Debug)]
pub struct Session {
    /// Browse or edit.
    pub mode: Mode,
    /// The records.
    pub buf: Buffer,
    /// The session defaults.
    pub defaults: Defaults,
    /// The member's source type.
    pub member_type: String,
    /// The library.
    pub lib: String,
    /// The file.
    pub file: String,
    /// The member.
    pub member: String,
    /// The member's text.
    pub text: String,
    /// Today as `yymmdd`, for changed records.
    pub today: u32,
    /// The first item shown.
    pub top: usize,
    /// The first column shown.
    pub left: usize,
    /// The function key set shown (0, 1, 2).
    pub keyset: u8,
    /// Excluded records.
    pub excluded: HashSet<u64>,
    /// Insert lines.
    pub inserts: Vec<InsLine>,
    /// Format lines: before which record, which format.
    pub formats: Vec<(u64, String)>,
    /// Line commands waiting for their partner.
    pub pending: Vec<Placed>,
    /// Find and change.
    pub find: FindOptions,
    /// The last match.
    pub hit: Option<Hit>,
    /// The message for the message line.
    pub message: Option<String>,
    /// The copyright line shows until the first key that does work.
    pub copyright: bool,
    /// The cursor to set.
    pub cursor: Option<(usize, usize)>,
    /// The command line to show (`F9` retrieves into it).
    pub cmdline: String,
    /// Commands entered, newest last.
    pub history: Vec<String>,
    retrieve: usize,
    /// The prompt in progress.
    pub prompt: Option<PromptState>,
    /// Records added or changed by the last key, for syntax checking.
    pub touched: Vec<u64>,
    /// Records the last checks found in error.
    pub errors: HashSet<u64>,
    /// What the rows showed at the last view, by screen row.
    shown: Vec<(usize, Item, String, String)>,
    next_ins: u64,
    /// Insert lines the last key made records: insert line, record.
    made: Vec<(u64, u64)>,
}

impl Session {
    /// A session over a member.
    #[must_use]
    pub fn new(mode: Mode, buf: Buffer, member_type: &str, (lib, file, member): (&str, &str, &str), today: u32) -> Self {
        let st = SourceType::new(member_type);
        let mut defaults = Defaults {
            srctype: st.name.clone(),
            ..Defaults::default()
        };
        defaults.upper = st.upper().unwrap_or(defaults.upper_unknown);
        Self {
            mode,
            buf,
            defaults,
            member_type: st.name.clone(),
            lib: lib.to_string(),
            file: file.to_string(),
            member: member.to_string(),
            text: String::new(),
            today,
            top: 0,
            left: st.first_column(),
            keyset: 0,
            excluded: HashSet::new(),
            inserts: Vec::new(),
            formats: Vec::new(),
            pending: Vec::new(),
            find: FindOptions::new(),
            hit: None,
            message: None,
            copyright: true,
            cursor: Some((2, DATA_COL)),
            cmdline: String::new(),
            history: Vec::new(),
            retrieve: 0,
            prompt: None,
            touched: Vec::new(),
            errors: HashSet::new(),
            shown: Vec::new(),
            next_ins: 1 << 62,
            made: Vec::new(),
        }
    }

    /// The source type the session prompts and checks by.
    #[must_use]
    pub fn srctype(&self) -> SourceType {
        SourceType::new(&self.defaults.srctype)
    }

    /// The session's source type changed (`F13`): upper case follows
    /// it. The message to show, when the case was forced.
    pub fn set_srctype(&mut self, name: &str) -> Option<String> {
        let st = SourceType::new(name);
        let changed = st.name != self.defaults.srctype;
        self.defaults.srctype.clone_from(&st.name);
        if !changed {
            return None;
        }
        let up = st.upper().unwrap_or(self.defaults.upper_unknown);
        let msg = (up != self.defaults.upper).then(|| {
            if up {
                "Input is now upper case only, as the source type asks.".to_string()
            } else {
                "Input now keeps lower case, as the source type allows.".to_string()
            }
        });
        self.defaults.upper = up;
        self.left = st.first_column();
        msg
    }

    /// Records shown on the display: the rows between the format line
    /// and the key lines, fewer under a prompt, more in full screen.
    #[must_use]
    pub fn body_rows(&self) -> usize {
        if let Some(p) = &self.prompt {
            return crate::srctype::layout(&p.format).top.saturating_sub(FIRST_ROW);
        }
        if self.defaults.full {
            return 24 - FIRST_ROW;
        }
        LAST_ROW - FIRST_ROW + 1
    }

    /// The window's width over the records.
    #[must_use]
    pub fn window(&self) -> usize {
        let w = self.buf.width.max(1);
        WINDOW.min(w.saturating_sub(self.left - 1)).max(1)
    }

    /// The furthest right the window goes.
    #[must_use]
    pub fn max_left(&self) -> usize {
        self.buf.width.saturating_sub(59).max(1)
    }

    /// Every line of the display in order.
    #[must_use]
    pub fn items(&self) -> Vec<Item> {
        let mut out = vec![Item::Begin];
        let ins_after = |out: &mut Vec<Item>, id: u64| {
            for i in self.inserts.iter().filter(|i| i.after == id) {
                for (before, f) in &self.formats {
                    if *before == i.id {
                        out.push(Item::Fmt(i.id, f.clone()));
                    }
                }
                out.push(Item::Ins(i.id));
            }
        };
        ins_after(&mut out, BEGIN_ID);
        let mut group: Vec<u64> = Vec::new();
        for r in self.buf.records() {
            if self.excluded.contains(&r.id) {
                group.push(r.id);
                continue;
            }
            if !group.is_empty() {
                out.push(Item::Excl(std::mem::take(&mut group)));
            }
            for (before, f) in &self.formats {
                if *before == r.id {
                    out.push(Item::Fmt(r.id, f.clone()));
                }
            }
            out.push(Item::Rec(r.id));
            ins_after(&mut out, r.id);
        }
        if !group.is_empty() {
            let last = *group.last().unwrap_or(&BEGIN_ID);
            out.push(Item::Excl(group));
            ins_after(&mut out, last);
        }
        for (before, f) in &self.formats {
            if *before == END_ID {
                out.push(Item::Fmt(END_ID, f.clone()));
            }
        }
        out.push(Item::End);
        out
    }

    /// The position of an item.
    fn item_index(&self, want: &Item) -> Option<usize> {
        self.items().iter().position(|i| i == want)
    }

    /// Make an item visible: the first line when it is above the
    /// display or below it.
    pub fn reveal(&mut self, item: &Item) {
        if let Some(i) = self.item_index(item)
            && (i < self.top || i >= self.top + self.body_rows())
        {
            self.top = i;
        }
    }

    /// A new insert line after a record.
    fn new_insert(&mut self, after: u64) -> u64 {
        self.next_ins += 1;
        let id = self.next_ins;
        // After the insert lines already there for that record.
        let pos = self
            .inserts
            .iter()
            .rposition(|i| i.after == after)
            .map_or(self.inserts.len(), |p| p + 1);
        self.inserts.insert(pos, InsLine { id, after });
        id
    }

    /// Roll by a number of lines, forward when positive.
    pub fn roll(&mut self, lines: i64) {
        let last = self.items().len().saturating_sub(1);
        let top = i64::try_from(self.top).unwrap_or(0) + lines;
        if top < 0 && self.top == 0 || top > i64::try_from(last).unwrap_or(0) && self.top == last {
            self.message = Some("The display is already at the end of the member.".into());
            return;
        }
        self.top = usize::try_from(top.max(0)).unwrap_or(0).min(last);
    }

    /// The roll keys: by the amount to roll.
    pub fn roll_key(&mut self, forward: bool, cursor_row: Option<usize>) {
        self.copyright = false;
        let rows = self.body_rows();
        let n = self.defaults.roll.lines(rows, cursor_row);
        let n = i64::try_from(n).unwrap_or(1);
        self.roll(if forward { n } else { -n });
    }

    /// `F19`/`F20`.
    pub fn shift(&mut self, right: bool) {
        self.copyright = false;
        if right {
            self.left = (self.left + SHIFT).min(self.max_left()).max(self.left.min(self.max_left()));
        } else {
            self.left = self.left.saturating_sub(SHIFT).max(1);
        }
    }

    /// `F11`: to column 31 from left of it, else back to the type's
    /// first column.
    pub fn toggle(&mut self) {
        self.copyright = false;
        let first = self.srctype().first_column();
        if self.left < TOGGLE_COL && self.buf.width > TOGGLE_COL {
            self.left = TOGGLE_COL;
        } else {
            self.left = first;
        }
    }

    /// `F24`.
    pub fn more_keys(&mut self) {
        self.keyset = (self.keyset + 1) % 3;
    }

    /// `F9`: the previous command into the command line.
    pub fn retrieve(&mut self) {
        if self.history.is_empty() {
            self.message = Some("No command to retrieve.".into());
            return;
        }
        self.retrieve = if self.retrieve == 0 {
            self.history.len()
        } else {
            self.retrieve
        };
        self.retrieve -= 1;
        self.cmdline = self.history[self.retrieve].clone();
        if self.retrieve == 0 {
            self.retrieve = self.history.len();
        }
        self.cursor = Some((2, DATA_COL));
    }

    /// The record a screen row shows, as a line command target.
    #[must_use]
    pub fn target_at(&self, row: usize) -> Option<Target> {
        self.shown.iter().find(|s| s.0 == row).and_then(|s| match &s.1 {
            Item::Rec(id) => Some(Target::Rec(*id)),
            Item::Ins(id) => Some(Target::Ins(*id)),
            _ => None,
        })
    }

    /// The data of a target.
    #[must_use]
    pub fn target_data(&self, t: Target) -> String {
        match t {
            Target::Rec(id) => self
                .buf
                .index_of(id)
                .and_then(|i| self.buf.get(i))
                .map(|r| r.data.clone())
                .unwrap_or_default(),
            Target::Ins(_) => String::new(),
        }
    }

    /// The sequence number text of a target (`'''''''` for an insert
    /// line).
    #[must_use]
    pub fn target_seq(&self, t: Target) -> String {
        match t {
            Target::Rec(id) => self
                .buf
                .index_of(id)
                .and_then(|i| self.buf.get(i))
                .map(|r| crate::seq_text(r.seq))
                .unwrap_or_default(),
            Target::Ins(_) => "'''''''".into(),
        }
    }

    /// Put a line's new text (from a prompt): a record changes, an
    /// insert line becomes a record. The record's identifier.
    pub fn put_line(&mut self, t: Target, text: &str) -> Option<u64> {
        let text = self.cased(text);
        match t {
            Target::Rec(id) => {
                let i = self.buf.index_of(id)?;
                if self.buf.set_data(i, &text, self.today) {
                    self.touched.push(id);
                }
                Some(id)
            }
            Target::Ins(ins) => {
                let id = self.materialize(ins, &text)?;
                self.touched.push(id);
                Some(id)
            }
        }
    }

    /// Text as the session's case rule has it.
    #[must_use]
    pub fn cased(&self, text: &str) -> String {
        if self.defaults.upper {
            text.to_ascii_uppercase()
        } else {
            text.to_string()
        }
    }

    /// An insert line typed into becomes a record; the insert line
    /// goes. The record's identifier.
    fn materialize(&mut self, ins: u64, text: &str) -> Option<u64> {
        let pos = self.inserts.iter().position(|i| i.id == ins)?;
        let line = self.inserts.remove(pos);
        // After the record it follows and the insert lines before it
        // that already became records.
        let after_idx = if line.after == BEGIN_ID {
            None
        } else {
            self.buf.index_of(line.after)
        };
        let at_end = after_idx.map_or(self.buf.is_empty(), |i| i + 1 == self.buf.len());
        let inc = if at_end {
            self.defaults.increment.max(100)
        } else {
            self.defaults.increment
        };
        let ids = self
            .buf
            .insert(after_idx, &[text.to_string()], inc, self.today)
            .map_err(|e| self.message = Some(format!("{e}: resequence the member to insert.")))
            .ok()?;
        let id = ids[0];
        // Insert lines still waiting after the same record now follow
        // the new one.
        for i in &mut self.inserts {
            if i.after == line.after {
                i.after = id;
            }
        }
        Some(id)
    }

    /// The prompt's target ends: back to the display, or on to the
    /// next insert line when inserting.
    pub fn prompt_done(&mut self, made: Option<u64>) {
        let Some(p) = self.prompt.take() else {
            return;
        };
        if let (Target::Ins(_), Some(id)) = (p.target, made) {
            let ins = self.new_insert(id);
            self.prompt = Some(PromptState {
                target: Target::Ins(ins),
                format: p.format,
            });
            self.reveal(&Item::Ins(ins));
        }
    }

    /// `F12` on a prompt: the prompt goes, and an insert line with it.
    pub fn prompt_cancel(&mut self) {
        if let Some(p) = self.prompt.take()
            && let Target::Ins(ins) = p.target
        {
            self.inserts.retain(|i| i.id != ins);
        }
    }

    /// `F11` on a prompt: the record before.
    pub fn prompt_previous(&mut self) {
        let Some(p) = self.prompt.clone() else {
            return;
        };
        let items = self.items();
        let here = match p.target {
            Target::Rec(id) => items.iter().position(|i| *i == Item::Rec(id)),
            Target::Ins(id) => items.iter().position(|i| *i == Item::Ins(id)),
        };
        let prev = here.and_then(|h| {
            items[..h].iter().rev().find_map(|i| match i {
                Item::Rec(id) => Some(*id),
                _ => None,
            })
        });
        match prev {
            Some(id) => {
                self.prompt_cancel();
                let st = self.srctype();
                let format = st.format_of(&self.target_data(Target::Rec(id))).to_string();
                self.prompt = Some(PromptState {
                    target: Target::Rec(id),
                    format,
                });
                self.reveal(&Item::Rec(id));
            }
            None => self.message = Some("There is no record before this one.".into()),
        }
    }

    /// Prompt a line in a format (the record's own when `None`). CL
    /// and command definitions go to the command prompter.
    pub fn start_prompt(&mut self, t: Target, format: Option<&str>) -> Action {
        let st = self.srctype();
        if matches!(st.family, crate::srctype::Family::Cl | crate::srctype::Family::Cmd)
            && format.is_none_or(|f| f != "**")
        {
            return Action::ClPrompt(t, self.target_data(t));
        }
        let data = self.target_data(t);
        let format = format.map_or_else(|| st.format_of(&data).to_string(), str::to_uppercase);
        self.prompt = Some(PromptState { target: t, format });
        match t {
            Target::Rec(id) => self.reveal_in_prompt(&Item::Rec(id)),
            Target::Ins(id) => self.reveal_in_prompt(&Item::Ins(id)),
        }
        Action::Show
    }

    /// Keep a prompted line on the rows above the prompt.
    fn reveal_in_prompt(&mut self, item: &Item) {
        let rows = self.body_rows();
        if let Some(i) = self.item_index(item)
            && (i < self.top || i >= self.top + rows)
        {
            self.top = (i + 1).saturating_sub(rows);
        }
    }

    /// The text of the session's syntax error on a record: the
    /// message, the cursor on the line.
    pub fn syntax_error(&mut self, id: u64, text: &str) {
        self.errors.insert(id);
        self.message = Some(text.to_string());
        self.reveal(&Item::Rec(id));
        if let Some(row) = self.row_of(&Item::Rec(id)) {
            self.cursor = Some((row, DATA_COL));
        }
    }

    /// Whether records in error remain (the checks of the records
    /// touched since forget theirs; deleted ones go).
    pub fn has_errors(&mut self) -> bool {
        let buf = &self.buf;
        self.errors.retain(|id| buf.index_of(*id).is_some());
        !self.errors.is_empty()
    }

    /// The screen row an item would be on.
    #[must_use]
    pub fn row_of(&self, item: &Item) -> Option<usize> {
        let i = self.item_index(item)?;
        (i >= self.top && i < self.top + self.body_rows()).then(|| FIRST_ROW + i - self.top)
    }

    /// The records written back.
    #[must_use]
    pub fn records(&self) -> Vec<(u32, u32, String)> {
        self.buf.to_records()
    }
}

#[cfg(test)]
mod tests;
