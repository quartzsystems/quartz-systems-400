// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the keys do: the typed changes, the line commands, the command
//! line, the function keys.

use super::{Action, BEGIN_ID, DATA_COL, END_ID, Input, Item, Mode, PromptState, Session, Target};
use crate::cmdline::{self, Command, SetOpt};
use crate::defaults::Roll;
use crate::find::{self, Direction, FindOptions, Hit};
use crate::line::{self, Kind, LineCmd, Placed};

/// The function keys the session knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Enter.
    Enter,
    /// A function key, 1 to 24.
    F(u8),
    /// Page down (roll up).
    PageDown,
    /// Page up (roll down).
    PageUp,
}

impl Session {
    /// A key with what the display sent.
    pub fn key(&mut self, key: Key, input: &Input) -> Action {
        self.message = None;
        self.touched.clear();
        if self.prompt.is_some() {
            return self.prompt_key(key, input);
        }
        let cursor_row = input.cursor.0;
        let mut row_target = self.target_at(cursor_row);
        self.made.clear();
        match key {
            Key::F(5) => {
                self.cmdline.clear();
                self.copyright = false;
                return Action::Show;
            }
            Key::F(24) => {
                self.more_keys();
                return Action::Show;
            }
            Key::F(12) if self.mode == Mode::Browse => return Action::Cancel,
            Key::F(9) => {
                self.retrieve();
                return Action::Show;
            }
            Key::F(10) => {
                self.cursor = Some((2, DATA_COL));
                return Action::Show;
            }
            _ => {}
        }
        // Everything else takes what was typed first.
        self.copyright = false;
        let command = self.take_input(input);
        if self.message.is_some() {
            self.cursor = Some(input.cursor);
            return Action::Show;
        }
        // An insert line typed into is a record now.
        if let Some(Target::Ins(ins)) = row_target
            && let Some((_, id)) = self.made.iter().find(|(i, _)| *i == ins)
        {
            row_target = Some(Target::Rec(*id));
        }
        let action = match key {
            Key::Enter => command.map_or(Action::Show, |c| self.command(c, input)),
            Key::PageDown | Key::PageUp => {
                let rel = cursor_row
                    .checked_sub(super::FIRST_ROW)
                    .filter(|r| *r < self.body_rows());
                self.roll_key(key == Key::PageDown, rel);
                Action::Show
            }
            Key::F(3) => {
                if self.mode == Mode::Browse {
                    Action::Cancel
                } else {
                    Action::Exit
                }
            }
            Key::F(12) => Action::Exit,
            Key::F(4) => match row_target {
                Some(t) if self.mode == Mode::Edit => self.start_prompt(t, None),
                Some(_) => self.refuse("Records cannot be prompted in browse mode."),
                None => self.refuse("Put the cursor on a record to prompt it."),
            },
            Key::F(11) => {
                self.toggle();
                Action::Show
            }
            Key::F(13) => Action::Defaults,
            Key::F(14) => Action::FindOptions,
            Key::F(15) => Action::BrowseOptions,
            Key::F(16) => self.repeat_find(input.cursor),
            Key::F(17) if self.mode == Mode::Edit => self.repeat_change(input.cursor),
            Key::F(19) => {
                self.shift(false);
                Action::Show
            }
            Key::F(20) => {
                self.shift(true);
                Action::Show
            }
            Key::F(21) => Action::SystemCommand,
            Key::F(23) => match row_target {
                Some(t) if self.mode == Mode::Edit => Action::SelectPrompt(t),
                _ => self.refuse("Put the cursor on a record to select its prompt."),
            },
            Key::F(_) => self.refuse("That key is not used here."),
        };
        if self.cursor.is_none() {
            self.cursor = Some((2, DATA_COL));
        }
        action
    }

    fn refuse(&mut self, text: &str) -> Action {
        self.message = Some(text.to_string());
        Action::Show
    }

    /// The typed changes and line commands; the command line's command
    /// when it has one. A refusal leaves its message.
    fn take_input(&mut self, input: &Input) -> Option<Command> {
        let cmd_text = input.cmdline.trim().to_string();
        self.cmdline.clear();
        self.cursor = None;
        let mut cmds: Vec<(usize, Placed)> = Vec::new();
        let mut typed_ins: Vec<(u64, String)> = Vec::new();
        let mut emptied: Vec<u64> = Vec::new();
        for ri in &input.rows {
            let Some((item, seq_shown, data_shown)) = self
                .shown_at(ri.row)
                .map(|(i, s, d)| (i.clone(), s.to_string(), d.to_string()))
            else {
                continue;
            };
            let id = match &item {
                Item::Rec(id) | Item::Ins(id) => *id,
                Item::Excl(ids) => ids[0],
                Item::Fmt(before, _) => *before,
                Item::Begin => BEGIN_ID,
                Item::End => END_ID,
            };
            if let Some(t) = line::typed(&seq_shown, &ri.seq) {
                match line::parse(&t) {
                    Ok(c) => cmds.push((ri.row, Placed { id, cmd: c })),
                    Err(e) => {
                        self.message = Some(format!("{e}."));
                        self.cursor = Some((ri.row, 1));
                        return None;
                    }
                }
            }
            let data_changed = ri.data.trim_end() != data_shown.trim_end();
            match &item {
                Item::Rec(id) if data_changed => {
                    if self.mode == Mode::Browse {
                        continue;
                    }
                    let i = self.buf.index_of(*id).expect("shown record");
                    let full = self.merge_window(&self.buf.records()[i].data.clone(), &ri.data);
                    let full = self.cased(&full);
                    if self.buf.set_data(i, &full, self.today) {
                        self.touched.push(*id);
                    }
                }
                Item::Ins(ins) => {
                    if ri.data.trim().is_empty() {
                        emptied.push(*ins);
                    } else {
                        let full = self.merge_window("", &ri.data);
                        typed_ins.push((*ins, self.cased(&full)));
                    }
                }
                _ => {}
            }
        }
        // Insert lines typed into become records, and insert mode goes
        // on after the last of them; untouched ones go.
        let mut last_new = None;
        let had_inserts = !self.inserts.is_empty();
        for (ins, text) in typed_ins {
            if let Some(id) = self.materialize(ins, &text) {
                self.touched.push(id);
                self.made.push((ins, id));
                last_new = Some(id);
            }
        }
        let commanded: Vec<u64> = cmds.iter().map(|(_, p)| p.id).collect();
        self.inserts
            .retain(|i| !emptied.contains(&i.id) || commanded.contains(&i.id));
        if let Some(id) = last_new
            && had_inserts
            && !self.inserts.iter().any(|i| i.after == id)
        {
            let ins = self.new_insert(id);
            self.reveal_bottom(&Item::Ins(ins));
            if let Some(r) = self.row_of(&Item::Ins(ins)) {
                self.cursor = Some((r, DATA_COL));
            }
        }
        if !cmds.is_empty() || !self.pending.is_empty() {
            self.line_commands(cmds);
        }
        if self.message.is_some() {
            return None;
        }
        if cmd_text.is_empty() {
            return None;
        }
        self.history.push(cmd_text.clone());
        match cmdline::parse(&cmd_text) {
            Ok(c) => c,
            Err(e) => {
                self.message = Some(format!("{}.", capital(&e.to_string())));
                self.cmdline = cmd_text;
                self.cursor = Some((2, DATA_COL));
                None
            }
        }
    }

    /// Keep an item on the display, as the last row when it is below.
    fn reveal_bottom(&mut self, item: &Item) {
        let rows = self.body_rows();
        if let Some(i) = self.items().iter().position(|x| x == item)
            && i >= self.top + rows
        {
            self.top = i + 1 - rows;
        }
    }

    /// A record's data with the window's columns replaced.
    fn merge_window(&self, data: &str, shown: &str) -> String {
        let w = self.window();
        let mut chars: Vec<char> = data.chars().collect();
        let need = self.left - 1 + w;
        if chars.len() < need {
            chars.resize(need, ' ');
        }
        let new: Vec<char> = shown.chars().collect();
        for k in 0..w {
            chars[self.left - 1 + k] = new.get(k).copied().unwrap_or(' ');
        }
        let s: String = chars.into_iter().collect();
        s.trim_end().to_string()
    }

    /// Run the line commands typed this time with those pending.
    fn line_commands(&mut self, typed: Vec<(usize, Placed)>) {
        // Display order: by the records' positions, the beginning
        // first and the end last.
        let pos_of = |s: &Self, id: u64| -> usize {
            if id == BEGIN_ID {
                0
            } else if id == END_ID {
                usize::MAX
            } else {
                s.buf.index_of(id).map_or_else(
                    || {
                        s.inserts
                            .iter()
                            .find(|i| i.id == id)
                            .and_then(|i| s.buf.index_of(i.after))
                            .map_or(0, |p| p + 1)
                    },
                    |p| p + 1,
                )
            }
        };
        let mut all: Vec<Placed> = std::mem::take(&mut self.pending);
        for (_, p) in typed {
            all.retain(|q| q.id != p.id);
            all.push(p);
        }
        all.sort_by_key(|p| pos_of(self, p.id));
        if self.mode == Mode::Browse
            && let Some(p) = all.iter().find(|p| modifies(p.cmd.kind))
        {
            self.message = Some(format!("Line command {} is not allowed in browse mode.", p.cmd.text));
            return;
        }
        let plan = match line::plan(&all) {
            Ok(p) => p,
            Err(e) => {
                self.message = Some(format!("{}.", capital(&e.to_string())));
                self.pending = all;
                return;
            }
        };
        self.pending.clone_from(&plan.pending);
        if !self.pending.is_empty() {
            self.message = Some("A line command waits for its other end.".into());
        }
        if let Some(t) = &plan.transfer {
            self.transfer(t);
        }
        for (kind, from, to, cmd) in &plan.blocks {
            self.block(*kind, *from, *to, cmd);
        }
        for p in &plan.single {
            self.single(p);
        }
    }

    /// The records from `from` to `to` (positions), or `n` from `from`.
    fn span(&self, from: u64, to: Option<u64>, n: Option<u32>) -> Option<(usize, usize)> {
        let a = self.buf.index_of(from)?;
        let b = match (to, n) {
            (Some(t), _) => self.buf.index_of(t)?,
            (None, Some(n)) => (a + n.max(1) as usize - 1).min(self.buf.len().saturating_sub(1)),
            (None, None) => a,
        };
        Some((a.min(b), a.max(b)))
    }

    fn transfer(&mut self, t: &line::Transfer) {
        let to = (t.to != t.from).then_some(t.to);
        let Some((a, b)) = self.span(t.from, to, t.count) else {
            return;
        };
        let lines: Vec<String> = self.buf.records()[a..=b].iter().map(|r| r.data.clone()).collect();
        let src_ids: Vec<u64> = self.buf.records()[a..=b].iter().map(|r| r.id).collect();
        match t.target_kind {
            Kind::Overlay => {
                let Some(start) = self.buf.index_of(t.target) else {
                    return;
                };
                let n = match (t.target_to, t.target_count) {
                    (Some(e), _) => self.buf.index_of(e).map_or(1, |e| e.saturating_sub(start) + 1),
                    (None, Some(c)) => c as usize,
                    (None, None) => lines.len(),
                };
                for k in 0..n.min(self.buf.len() - start) {
                    let src = &lines[k % lines.len()];
                    let mut tgt: Vec<char> = self.buf.records()[start + k].data.chars().collect();
                    for (j, c) in src.chars().enumerate() {
                        if j >= tgt.len() {
                            tgt.resize(j + 1, ' ');
                        }
                        if tgt[j] == ' ' {
                            tgt[j] = c;
                        }
                    }
                    let s: String = tgt.into_iter().collect();
                    let id = self.buf.records()[start + k].id;
                    if self.buf.set_data(start + k, &s, self.today) {
                        self.touched.push(id);
                    }
                }
            }
            kind => {
                let after = if t.target == BEGIN_ID {
                    None
                } else if t.target == END_ID {
                    self.buf.len().checked_sub(1)
                } else {
                    let Some(i) = self.buf.index_of(t.target) else {
                        return;
                    };
                    if kind == Kind::Before { i.checked_sub(1) } else { Some(i) }
                };
                if t.kind == Kind::Move
                    && let Some(at) = after
                    && at >= a
                    && at < b
                {
                    self.message = Some("The target is inside the lines moved.".into());
                    return;
                }
                let inc = self.defaults.increment;
                match self.buf.insert(after, &lines, inc, self.today) {
                    Ok(ids) => self.touched.extend(ids),
                    Err(e) => {
                        self.message = Some(format!("{e}: resequence the member first."));
                        return;
                    }
                }
            }
        }
        match t.kind {
            Kind::Move => {
                for id in src_ids {
                    if let Some(i) = self.buf.index_of(id) {
                        self.buf.remove(i, i);
                    }
                }
            }
            Kind::CopyRepeat => {
                self.pending.push(Placed {
                    id: t.from,
                    cmd: t.source_cmd.clone(),
                });
                if t.to != t.from {
                    self.pending.push(Placed {
                        id: t.to,
                        cmd: t.source_cmd.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    fn block(&mut self, kind: Kind, from: u64, to: u64, cmd: &LineCmd) {
        let Some((a, b)) = self.span(from, Some(to), None) else {
            // A block over excluded lines or insert lines: only the
            // records count.
            return;
        };
        let ids: Vec<u64> = self.buf.records()[a..=b].iter().map(|r| r.id).collect();
        match kind {
            Kind::Delete => {
                self.buf.remove(a, b);
            }
            Kind::Exclude => self.excluded.extend(ids),
            Kind::Show => {
                for id in ids {
                    self.excluded.remove(&id);
                }
            }
            Kind::Repeat => {
                let lines: Vec<String> = self.buf.records()[a..=b].iter().map(|r| r.data.clone()).collect();
                for _ in 0..cmd.count.unwrap_or(1) {
                    if let Ok(new) = self.buf.insert(Some(b), &lines, self.defaults.increment, self.today) {
                        self.touched.extend(new);
                    }
                }
            }
            Kind::ShiftLeft | Kind::ShiftRight => {
                for i in a..=b {
                    self.shift_record(i, kind == Kind::ShiftRight, cmd.count.unwrap_or(1) as usize);
                }
            }
            Kind::Prompt => {
                self.prompt = Some(PromptState {
                    target: Target::Rec(from),
                    format: self.srctype().format_of(&self.target_data(Target::Rec(from))).into(),
                });
            }
            _ => {}
        }
    }

    /// Shift a record's data, never losing a character that is not
    /// blank.
    fn shift_record(&mut self, i: usize, right: bool, n: usize) {
        let data = self.buf.records()[i].data.clone();
        let id = self.buf.records()[i].id;
        let mut chars: Vec<char> = data.chars().collect();
        if right {
            let room = self.buf.width.saturating_sub(chars.len());
            let k = n.min(room);
            if k < n {
                self.message = Some("A shift stopped where data would be lost.".into());
            }
            let mut out = vec![' '; k];
            out.append(&mut chars);
            chars = out;
        } else {
            let lead = chars.iter().take_while(|c| **c == ' ').count();
            let k = n.min(lead);
            if k < n {
                self.message = Some("A shift stopped where data would be lost.".into());
            }
            chars.drain(..k);
        }
        let s: String = chars.into_iter().collect();
        if self.buf.set_data(i, &s, self.today) {
            self.touched.push(id);
        }
    }

    fn single(&mut self, p: &Placed) {
        let c = &p.cmd;
        let n = c.count;
        let is_ins = self.inserts.iter().any(|i| i.id == p.id);
        match c.kind {
            Kind::Delete => {
                if is_ins {
                    self.inserts.retain(|i| i.id != p.id);
                } else if let Some(pos) = self.formats.iter().position(|f| f.0 == p.id)
                    && self.buf.index_of(p.id).is_none()
                {
                    self.formats.remove(pos);
                } else if let Some((a, b)) = self.span(p.id, None, n) {
                    self.buf.remove(a, b);
                }
            }
            Kind::Insert | Kind::InsertPrompt | Kind::InsertFormat => {
                let k = if c.kind == Kind::Insert { n.unwrap_or(1) } else { 1 };
                let after = if p.id == END_ID {
                    self.buf.records().last().map_or(BEGIN_ID, |r| r.id)
                } else {
                    p.id
                };
                let mut first = None;
                for _ in 0..k.max(1) {
                    let id = self.new_insert(after);
                    first.get_or_insert(id);
                }
                let Some(ins) = first else {
                    return;
                };
                if c.kind == Kind::InsertFormat {
                    let f = c.format.clone().unwrap_or_else(|| "**".into());
                    self.formats.push((ins, f));
                }
                if c.kind == Kind::InsertPrompt {
                    let fmt = c.format.clone();
                    let _ = self.start_prompt(Target::Ins(ins), fmt.as_deref());
                } else {
                    self.reveal_bottom(&Item::Ins(ins));
                    if let Some(r) = self.row_of(&Item::Ins(ins)) {
                        self.cursor = Some((r, DATA_COL));
                    }
                }
            }
            Kind::Repeat => {
                if let Some(i) = self.buf.index_of(p.id) {
                    let line = self.buf.records()[i].data.clone();
                    let lines = vec![line; n.unwrap_or(1).max(1) as usize];
                    if let Ok(ids) = self.buf.insert(Some(i), &lines, self.defaults.increment, self.today) {
                        self.touched.extend(ids);
                    }
                }
            }
            Kind::ShiftLeft | Kind::ShiftRight => {
                if let Some(i) = self.buf.index_of(p.id) {
                    self.shift_record(i, c.kind == Kind::ShiftRight, n.unwrap_or(1) as usize);
                }
            }
            Kind::Exclude => {
                if let Some((a, b)) = self.span(p.id, None, n) {
                    let ids: Vec<u64> = self.buf.records()[a..=b].iter().map(|r| r.id).collect();
                    self.excluded.extend(ids);
                }
            }
            Kind::Show | Kind::ShowFirst | Kind::ShowLast => self.show(p.id, c),
            Kind::Format => {
                let f = c.format.clone().unwrap_or_else(|| {
                    self.srctype().format_of(&self.target_data(Target::Rec(p.id))).to_string()
                });
                self.formats.retain(|x| x.0 != p.id);
                self.formats.push((p.id, f));
            }
            Kind::Prompt => {
                let t = if is_ins { Target::Ins(p.id) } else { Target::Rec(p.id) };
                let _ = self.start_prompt(t, c.format.as_deref());
            }
            Kind::Window => {
                let col = n.map_or_else(|| self.srctype().first_column(), |v| v as usize);
                self.left = col.clamp(1, self.max_left());
            }
            Kind::Top => {
                if let Some(i) = self.items().iter().position(|it| item_id(it) == Some(p.id)) {
                    self.top = i;
                }
            }
            Kind::Forward | Kind::Back => {
                let k = n.map_or_else(|| self.defaults.roll.lines(self.body_rows(), None), |v| v as usize);
                let k = i64::try_from(k).unwrap_or(1);
                self.roll(if c.kind == Kind::Forward { k } else { -k });
            }
            Kind::Position => self.position(n.unwrap_or(0)),
            _ => {}
        }
    }

    /// `S`, `SF`, `SL` on an excluded line.
    fn show(&mut self, first: u64, c: &LineCmd) {
        let group = self.items().into_iter().find_map(|i| match i {
            Item::Excl(ids) if ids.first() == Some(&first) => Some(ids),
            _ => None,
        });
        let Some(ids) = group else {
            self.message = Some("Show commands go on an excluded line.".into());
            return;
        };
        let k = c.count.unwrap_or(1).max(1) as usize;
        let pick: Vec<u64> = if c.kind == Kind::ShowLast {
            ids.iter().rev().take(k).copied().collect()
        } else {
            ids.iter().take(k).copied().collect()
        };
        for id in pick {
            self.excluded.remove(&id);
        }
    }

    /// The record at or after a sequence number to the top.
    fn position(&mut self, seq: u32) {
        let i = self.buf.index_at_seq(seq);
        match self.buf.get(i).map(|r| r.id) {
            Some(id) => {
                if let Some(at) = self.items().iter().position(|it| *it == Item::Rec(id)) {
                    self.top = at;
                }
            }
            None => self.top = self.items().len().saturating_sub(1),
        }
    }

    /// A command line command.
    fn command(&mut self, c: Command, input: &Input) -> Action {
        match c {
            Command::Top => self.top = 0,
            Command::Bottom => {
                let end = self.items().len().saturating_sub(1);
                self.top = end.saturating_sub(self.body_rows().saturating_sub(1));
            }
            Command::Position(s) => self.position(s),
            Command::Roll(n) => self.roll(n),
            Command::Cancel => return Action::Cancel,
            Command::Exit => return Action::Exit,
            Command::Save(t) => {
                if self.mode == Mode::Browse {
                    return self.refuse("A member cannot be saved in browse mode.");
                }
                return Action::Save(t);
            }
            Command::File(t) => {
                if self.mode == Mode::Browse {
                    return self.refuse("A member cannot be saved in browse mode.");
                }
                return Action::File(t);
            }
            Command::Set(o) => self.set(o),
            Command::Find(s) => {
                self.search_opts(&s);
                return self.do_find(input.cursor, false);
            }
            Command::Change(s, new) => {
                if self.mode == Mode::Browse {
                    return self.refuse("Records cannot be changed in browse mode.");
                }
                self.search_opts(&s);
                self.find.change = self.cased(&new);
                return self.do_find(input.cursor, true);
            }
        }
        Action::Show
    }

    fn set(&mut self, o: SetOpt) {
        match o {
            SetOpt::Caps(b) => self.defaults.upper = b,
            SetOpt::Tabs(b) => self.defaults.tabs = b,
            SetOpt::Match(b) => self.find.same_case = b,
            SetOpt::Full(b) => self.defaults.full = b,
            SetOpt::Roll(r) => {
                if let Some(r) = Roll::parse(&r) {
                    self.defaults.roll = r;
                }
            }
        }
    }

    fn search_opts(&mut self, s: &cmdline::Search) {
        if s.what != "*" {
            self.find.find.clone_from(&s.what);
        }
        self.find.dir = s.dir;
        self.find.scope = s.scope;
        self.find.from = s.from.unwrap_or(1);
        self.find.to = s.to;
    }

    /// Where a search starts: the cursor's record and column, else
    /// the last match, else the top.
    fn search_from(&self, cursor: (usize, usize)) -> Option<Hit> {
        if let Some((Item::Rec(id), _, _)) = self.shown_at(cursor.0)
            && cursor.1 >= DATA_COL
            && let Some(rec) = self.buf.index_of(*id)
        {
            return Some(Hit {
                rec,
                col: self.left + cursor.1 - DATA_COL,
            });
        }
        self.hit
    }

    fn do_find(&mut self, cursor: (usize, usize), change: bool) -> Action {
        if self.find.find.is_empty() {
            return self.refuse("Type the string to find first (F14 or FIND).");
        }
        let what = self.find.find.clone();
        if change && self.find.dir == Direction::All {
            let n = find::change_all(&mut self.buf, &self.find, &self.excluded, self.today);
            self.message = Some(if n == 0 {
                format!("No record holds {what}.")
            } else {
                format!("{what} changed in {n} records.")
            });
            return Action::Show;
        }
        let from = self.search_from(cursor);
        let Some(h) = find::find(&self.buf, &self.find, &self.excluded, from) else {
            self.message = Some(format!("No more of {} in the member.", what.to_ascii_uppercase()));
            return Action::Show;
        };
        if change {
            let id = self.buf.records()[h.rec].id;
            if find::change_at(&mut self.buf, &self.find, h, self.today) {
                self.touched.push(id);
            }
        }
        self.hit = Some(h);
        let id = self.buf.records()[h.rec].id;
        self.excluded.remove(&id);
        self.reveal(&Item::Rec(id));
        if h.col < self.left || h.col >= self.left + self.window() {
            self.left = h.col.clamp(1, self.max_left());
        }
        if let Some(r) = self.row_of(&Item::Rec(id)) {
            self.cursor = Some((r, DATA_COL + h.col - self.left));
        }
        self.message = Some(if change {
            format!("{} changed.", what.to_ascii_uppercase())
        } else {
            format!("Found {}.", what.to_ascii_uppercase())
        });
        Action::Show
    }

    /// `F16`.
    fn repeat_find(&mut self, cursor: (usize, usize)) -> Action {
        if self.find.dir != Direction::Previous {
            self.find.dir = Direction::Next;
        }
        self.do_find(cursor, false)
    }

    /// `F17`.
    fn repeat_change(&mut self, cursor: (usize, usize)) -> Action {
        if self.find.dir == Direction::All {
            return self.do_find(cursor, true);
        }
        self.find.dir = Direction::Next;
        self.do_find(cursor, true)
    }

    /// Take the find and change options display's values.
    pub fn set_find(&mut self, o: FindOptions) {
        self.find = o;
    }

    /// A key on a prompt.
    fn prompt_key(&mut self, key: Key, input: &Input) -> Action {
        let Some(p) = self.prompt.clone() else {
            return Action::Show;
        };
        match key {
            Key::F(12) => {
                self.prompt_cancel();
                Action::Show
            }
            Key::F(3) => {
                self.prompt_cancel();
                Action::Exit
            }
            Key::F(5) => Action::Show,
            Key::F(11) => {
                self.prompt_previous();
                Action::Show
            }
            Key::F(24) => {
                self.keyset = (self.keyset + 1) % 2;
                Action::Show
            }
            Key::F(23) => Action::SelectPrompt(p.target),
            Key::F(13) => Action::Defaults,
            Key::F(14) => Action::FindOptions,
            Key::F(15) => Action::BrowseOptions,
            Key::Enter | Key::F(4) => {
                let pr = crate::prompt::Prompt::new(&p.format, &self.target_data(p.target), self.buf.width);
                let original = self.target_data(p.target);
                let text = pr.record(&original, &input.prompt);
                let blank = text.trim().is_empty();
                if matches!(p.target, Target::Ins(_)) && blank {
                    self.prompt_cancel();
                    return Action::Show;
                }
                let made = self.put_line(p.target, &text);
                self.prompt_done(made);
                Action::Show
            }
            _ => self.refuse("That key is not used here."),
        }
    }
}

/// Line commands that change the member.
fn modifies(k: Kind) -> bool {
    !matches!(
        k,
        Kind::Exclude
            | Kind::Show
            | Kind::ShowFirst
            | Kind::ShowLast
            | Kind::Format
            | Kind::Window
            | Kind::Top
            | Kind::Forward
            | Kind::Back
            | Kind::Position
    )
}

fn item_id(i: &Item) -> Option<u64> {
    match i {
        Item::Rec(id) | Item::Ins(id) | Item::Fmt(id, _) => Some(*id),
        Item::Excl(ids) => ids.first().copied(),
        Item::Begin => Some(BEGIN_ID),
        Item::End => Some(END_ID),
    }
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
