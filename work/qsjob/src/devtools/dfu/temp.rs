// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Working with the data in a file: `UPDDTA`'s temporary program, and a
//! DFU program run by `CHGDTA` or `DSPDTA`. Change mode finds a record
//! by key (or record number) and writes it back; entry and insert add
//! records; display mode only reads. End Data Entry closes the session.

// Throughout: `s` the session, `a` the answer, `f` the file.
#![allow(clippy::many_single_char_names)]
use std::collections::BTreeMap;

use dfu::field::{blank_record, key_bytes};
use dfu::screen::work::{Row, View};
use dfu::session::Step;
use dfu::{Mode, Program, Session};
use mi::io::KeyRef;

use super::data::{DataFile, Format, Open};
use super::ui::{Key, show};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The record on display: its bytes, record number and the format.
struct Shown {
    rec: Vec<u8>,
    rrn: u64,
}

/// One data session.
struct Run<'a> {
    file: &'a DataFile,
    open: Open,
    s: Session,
    program: Option<&'a Program>,
    member: String,
    format: usize,
    shown: Option<Shown>,
    /// What the operator typed, by field name, across pages.
    typed: BTreeMap<String, String>,
    page: usize,
    message: Option<String>,
    copyright: bool,
}

/// Work with a file's data until End Data Entry; `None` when the file
/// cannot be opened, else the message for the caller's message line.
pub fn run(
    host: &Host,
    surface: &mut Surface,
    file: &DataFile,
    member: Option<String>,
    program: Option<&Program>,
    display_only: bool,
) -> Result<(), String> {
    let open = Open::new(host, file, member, display_only)
        .map_err(|e| format!("{} the file {} could not be opened.", e.msgid, file.name))?;
    let member = open.info.member.clone();
    let mut r = Run {
        file,
        open,
        s: Session::new(display_only),
        program,
        member,
        format: 0,
        shown: None,
        typed: BTreeMap::new(),
        page: 0,
        message: None,
        copyright: true,
    };
    r.main(surface);
    Ok(())
}

impl Run<'_> {
    fn fmt(&self) -> &Format {
        &self.file.formats[self.format.min(self.file.formats.len() - 1)]
    }

    /// The fields the display shows: the program's choice, else all.
    fn fields(&self) -> Vec<dfu::Field> {
        let f = self.fmt();
        let names: Vec<String> = f.fields.iter().map(|x| x.name.clone()).collect();
        let pick: Vec<String> = match self.program {
            Some(p) => p.shown(&f.name, &names).into_iter().cloned().collect(),
            None => names,
        };
        f.fields
            .iter()
            .filter(|x| pick.contains(&x.name))
            .cloned()
            .collect()
    }

    fn title(&self) -> String {
        self.program
            .map(|p| p.title.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "WORK WITH DATA IN A FILE".into())
    }

    fn heading(&self) -> String {
        self.program
            .map_or_else(|| "*RECNBR".into(), |p| p.recnbr_heading.clone())
    }

    fn view(&self) -> View {
        let fields = self.fields();
        let mut keys = Vec::new();
        let mut rows = Vec::new();
        let value = |f: &dfu::Field, rec: Option<&[u8]>| {
            self.typed
                .get(&f.name)
                .cloned()
                .or_else(|| rec.map(|r| f.shown(r)))
                .unwrap_or_default()
        };
        match (&self.shown, self.s.mode) {
            (None, Mode::Change | Mode::Display) => {
                if self.file.keyed {
                    for name in &self.fmt().keys {
                        if let Some(f) = fields.iter().chain(self.fmt().fields.iter()).find(|x| x.name == *name) {
                            keys.push(Row::input(&f.name, &f.name, f.width(), &value(f, None)));
                        }
                    }
                } else {
                    let t = self.typed.get("*RECNBR").cloned().unwrap_or_default();
                    keys.push(Row::input("*RECNBR", &self.heading(), 9, &t));
                }
            }
            (Some(sh), mode) => {
                if !self.file.keyed {
                    keys.push(Row::output(&self.heading(), 9, &format!("{:>9}", sh.rrn)));
                }
                let input = mode != Mode::Display;
                for f in &fields {
                    let v = value(f, Some(&sh.rec));
                    rows.push(if input {
                        Row::input(&f.name, &f.name, f.width(), &v)
                    } else {
                        Row::output(&f.name, f.width(), &v)
                    });
                }
            }
            (None, _) => {
                for f in &fields {
                    rows.push(Row::input(&f.name, &f.name, f.width(), &value(f, None)));
                }
            }
        }
        View {
            title: self.title(),
            mode: self.s.mode,
            format: self.fmt().name.clone(),
            file: self.file.name.clone(),
            keys,
            rows,
            page: self.page,
            message: self.message.clone(),
            copyright: self.copyright,
        }
    }

    fn main(&mut self, surface: &mut Surface) {
        loop {
            let view = self.view();
            let screen = dfu::screen::work::screen(&view);
            self.message = None;
            self.copyright = false;
            let Some(a) = show(surface, &screen) else {
                return;
            };
            for (k, v) in &a.vars {
                self.typed.insert(k.clone(), v.trim_end().to_string());
            }
            let pages = dfu::screen::work::pages(&view);
            let code = match a.key {
                Key::Enter => 0,
                Key::F(n) => n,
                Key::PageDown if self.page + 1 < pages => {
                    self.page += 1;
                    continue;
                }
                Key::PageUp if self.page > 0 => {
                    self.page -= 1;
                    continue;
                }
                Key::PageDown => 100,
                Key::PageUp => 101,
                Key::Help | Key::Other => {
                    self.message = Some("Help is not available for this display.".into());
                    continue;
                }
            };
            let shown = self.shown.is_some();
            match self.s.key(code, shown) {
                Step::Enter => self.enter(),
                Step::Restart => self.restart(),
                Step::Formats => self.formats(surface),
                Step::Next | Step::Previous => {
                    let back = code == 101;
                    if self.save_shown() {
                        self.roll(back);
                    }
                }
                Step::Delete => self.delete(),
                Step::End => match self.end(surface) {
                    Some(true) | None => return,
                    Some(false) => {}
                },
                Step::Invalid => {
                    self.message = Some("That function key cannot be used here.".into());
                }
            }
        }
    }

    fn clear(&mut self) {
        if self.shown.take().is_some() {
            self.open.unlock();
        }
        self.typed.clear();
        self.page = 0;
    }

    fn restart(&mut self) {
        self.clear();
    }

    /// The typed values over a record; `Err` with the message when a
    /// value is refused.
    fn merged(&self, base: &[u8]) -> Result<Vec<u8>, String> {
        let mut rec = base.to_vec();
        for f in self.fields() {
            // Only what the operator changed: an untouched value keeps
            // its bytes.
            if let Some(v) = self
                .typed
                .get(&f.name)
                .filter(|v| v.trim() != f.shown(base).trim())
            {
                f.encode(v, &mut rec).map_err(|e| e.to_string())?;
            }
        }
        Ok(rec)
    }

    fn enter(&mut self) {
        match self.s.mode {
            Mode::Entry | Mode::Insert => self.add(),
            Mode::Change | Mode::Display if self.shown.is_none() => self.find(),
            Mode::Change => {
                if self.save_shown() {
                    self.clear();
                }
            }
            Mode::Display => self.clear(),
        }
    }

    /// Write the record shown when the operator changed it; `false`
    /// when a value was refused.
    fn save_shown(&mut self) -> bool {
        let Some(sh) = &self.shown else {
            return true;
        };
        if self.s.mode != Mode::Change {
            return true;
        }
        let rec = match self.merged(&sh.rec) {
            Ok(r) => r,
            Err(m) => {
                self.message = Some(m);
                return false;
            }
        };
        if rec == sh.rec {
            return true;
        }
        if self.program.is_some_and(|p| !p.roll_update) && self.typed.is_empty() {
            return true;
        }
        let fmt = self.fmt().name.clone();
        let r = self.open.update(&rec, &fmt);
        if r.status >= 1000 {
            self.message = Some(failed(r.msgid.as_deref(), "changed"));
            return false;
        }
        self.s.changed += 1;
        self.message = Some("The record was changed.".into());
        if let Some(sh) = &mut self.shown {
            sh.rec = rec;
        }
        self.typed.clear();
        true
    }

    fn find(&mut self) {
        let typed = self.typed.clone();
        let nolock = self.s.mode == Mode::Display;
        let r = if self.file.keyed {
            let fmt = self.fmt().clone();
            let mut rec = blank_record(&fmt.fields, fmt.len);
            for f in fmt.fields.iter().filter(|f| f.key) {
                if let Some(v) = typed.get(&f.name)
                    && let Err(e) = f.encode(v, &mut rec)
                {
                    self.message = Some(e.to_string());
                    return;
                }
            }
            let key = key_bytes(&fmt.fields, &fmt.keys, &rec);
            self.open.chain(KeyRef::Fields(&key), nolock)
        } else {
            let t = typed.get("*RECNBR").map_or("", |s| s.trim());
            let Ok(n) = t.parse::<u64>() else {
                self.message = Some("Type a record number.".into());
                return;
            };
            self.open.chain(KeyRef::Rrn(n), nolock)
        };
        self.typed.clear();
        match r.record {
            Some(rec) if r.found => {
                self.shown = Some(Shown { rec, rrn: r.rrn });
                self.page = 0;
            }
            _ if r.status >= 1000 && r.status != 1211 && r.status != 1218 => {
                self.message = Some(format!("No record with that key in member {}.", self.member));
            }
            _ if r.status == 1218 => {
                self.message = Some("The record is in use by another job.".into());
            }
            _ => {
                self.message = Some(format!("No record with that key in member {}.", self.member));
            }
        }
    }

    fn roll(&mut self, back: bool) {
        let nolock = self.s.mode == Mode::Display;
        if self.shown.is_none() {
            // Forward from the start of the member; nothing comes before it.
            if back {
                self.message = Some("There are no more records in that direction.".into());
                return;
            }
            let _ = self.open.position(KeyRef::None, false);
        } else {
            self.open.unlock();
        }
        let r = self.open.step(back, nolock);
        self.typed.clear();
        self.page = 0;
        match r.record {
            Some(rec) if !r.eof => self.shown = Some(Shown { rec, rrn: r.rrn }),
            _ => {
                self.shown = None;
                self.message = Some("There are no more records in that direction.".into());
            }
        }
    }

    fn add(&mut self) {
        if self.typed.values().all(|v| v.trim().is_empty()) {
            return;
        }
        let fmt = self.fmt().clone();
        let base = blank_record(&fmt.fields, fmt.len);
        let rec = match self.merged(&base) {
            Ok(r) => r,
            Err(m) => {
                self.message = Some(m);
                return;
            }
        };
        let r = self.open.write(&rec, &fmt.name);
        if r.status >= 1000 {
            self.message = Some(failed(r.msgid.as_deref(), "added"));
            return;
        }
        self.s.added += 1;
        self.typed.clear();
        self.page = 0;
        self.message = Some("The record was added.".into());
    }

    fn delete(&mut self) {
        let r = self.open.delete();
        if r.status >= 1000 {
            self.message = Some(failed(r.msgid.as_deref(), "deleted"));
            return;
        }
        self.s.deleted += 1;
        self.shown = None;
        self.typed.clear();
        self.message = Some("The record was deleted.".into());
    }

    fn formats(&mut self, surface: &mut Surface) {
        let list: Vec<(String, String)> = self
            .file
            .formats
            .iter()
            .map(|f| (f.name.clone(), f.text.clone()))
            .collect();
        loop {
            let screen = dfu::screen::work::formats(&list, None);
            let Some(a) = show(surface, &screen) else {
                return;
            };
            match a.key {
                Key::Enter => {
                    if let Some((i, _)) = a.options.iter().find(|(_, o)| o.as_str() == "1") {
                        self.format = *i;
                        self.restart();
                    }
                    return;
                }
                Key::F(5) => {}
                _ => return,
            }
        }
    }

    /// End Data Entry: `Some(true)` to leave, `Some(false)` to go on.
    fn end(&mut self, surface: &mut Surface) -> Option<bool> {
        let mut answer = "Y".to_string();
        let mut message = None;
        loop {
            let screen = dfu::screen::work::end(&self.s, &answer, message.as_deref());
            let a = show(surface, &screen)?;
            match a.key {
                Key::F(3) => return Some(true),
                Key::F(12) => return Some(false),
                Key::Enter => match a.up("END").as_str() {
                    "Y" => return Some(true),
                    "N" => return Some(false),
                    other => {
                        answer = other.to_string();
                        message = Some("Type Y or N.".to_string());
                    }
                },
                _ => message = Some("That function key cannot be used here.".into()),
            }
        }
    }
}

fn failed(msgid: Option<&str>, what: &str) -> String {
    format!(
        "The record was not {what} ({}).",
        msgid.unwrap_or("an error")
    )
}
