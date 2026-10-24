// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kit's display: a member's display files on a screen model,
//! shown on a `tn5250` terminal as the oracle's 5250 session shows
//! them. Each read sends the screen to the terminal, keeps its text
//! rows, and answers with the member's next `keys:` step; a read with
//! no step left ends the request (`CPF1907`).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::rc::Rc;

use dspf::rt::{DateOrder, Env, Problem, Runtime};
use mi::io::IoFail;
use mi::io::display::{DisplayInput, DisplayOpen, DisplaySource};
use rpg_oracle::keys;
use rpg_oracle::outcome::{Screen as Rows, screen_rows};

/// An answer: the attention identifier and the cursor.
type Answer = (u8, (u8, u8));
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Read, ScreenModel};
use tn5250::term::Terminal;

/// The display's state.
pub struct Screen {
    steps: VecDeque<String>,
    /// The screens read, as the outcome keeps them.
    pub shown: Vec<Rows>,
    model: ScreenModel,
    term: Terminal,
    files: HashMap<u32, Runtime>,
    next: u32,
    displays: Vec<(String, Rc<dspf::DisplayFile>)>,
    env: Env,
}

/// The job's handle on the display.
#[derive(Clone)]
pub struct KitDisplay(pub Rc<RefCell<Screen>>);

impl KitDisplay {
    /// Over a member's display files, its steps and the messages its
    /// setup adds.
    #[must_use]
    pub fn new(displays: Vec<(String, Rc<dspf::DisplayFile>)>, steps: &[String], messages: BTreeMap<String, String>) -> Self {
        Self(Rc::new(RefCell::new(Screen {
            steps: steps.iter().cloned().collect(),
            shown: Vec::new(),
            model: ScreenModel::new(false),
            term: Terminal::new(Capabilities {
                model: Model::M3179,
                gui: false,
                color: true,
            }),
            files: HashMap::new(),
            next: 1,
            displays,
            env: Env {
                date: (2026, 10, 23),
                sys_date: (2026, 10, 23),
                time: (12, 0, 0),
                datfmt: DateOrder::Mdy,
                datsep: '/',
                user: "QSECOFR".into(),
                sysname: "QS400".into(),
                messages,
            },
        })))
    }
}

fn not_open(format: &str) -> IoFail {
    IoFail::new(1299, "CPF4101", &[format])
}

fn bad(format: &str) -> IoFail {
    IoFail::new(1299, "CPF5123", &[format, "QGPL"])
}

/// A read's result for the program.
fn input_of(format: &str, i: dspf::rt::Input) -> DisplayInput {
    DisplayInput {
        format: format.to_string(),
        record: i.record,
        indicators: i.indicators,
        aid: i.aid,
        cursor: i.cursor,
        sfl_rrn: i.sfl_rrn,
        sfl_top: i.sfl_top,
        sfl_count: i.sfl_count,
    }
}

impl Screen {
    /// Show the model on the terminal and keep its rows.
    fn show(&mut self) -> Result<(), IoFail> {
        let out = self.model.render(Read::MdtFields);
        self.term
            .apply(&Record::new(out.opcode, out.data))
            .map_err(|_| IoFail::new(1299, "CPF5140", &[]))?;
        self.shown.push(screen_rows(&self.term.text_rows()));
        Ok(())
    }

    /// The operator's next answer: the attention identifier and the
    /// cursor, or `None` when the steps ran out.
    fn answer(&mut self) -> Result<Option<Answer>, IoFail> {
        let Some(step) = self.steps.pop_front() else {
            return Ok(None);
        };
        let fail = |e: String| {
            let mut f = IoFail::new(1299, "CPF5140", &[]);
            f.data = e.into_bytes();
            f
        };
        let actions = keys::parse(&step).map_err(fail)?;
        if keys::play(&mut self.term, &actions).is_err() {
            // The step does not fit the screen: the oracle ends the
            // request the same way.
            let mut f = IoFail::new(IoFail::REQUEST_ENDED, "DISPLAY", &[]);
            f.data.clear();
            return Err(f);
        }
        let Some(rec) = self.term.take_outbox().into_iter().next() else {
            return Err(fail("no record".into()));
        };
        let inb = self.model.accept(&rec.data).ok_or_else(|| fail("not an input".into()))?;
        Ok(Some((inb.aid, (inb.row, inb.col))))
    }
}

impl DisplaySource for KitDisplay {
    fn open(&mut self, file: &str, _lib: &str, _actgrp: &str) -> Result<DisplayOpen, IoFail> {
        let mut s = self.0.borrow_mut();
        let df = s
            .displays
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(file))
            .map(|(_, d)| (**d).clone())
            .ok_or_else(|| IoFail::new(1299, "CPF9801", &[file, "*LIBL"]))?;
        let handle = s.next;
        s.next += 1;
        s.files.insert(handle, Runtime::new(df, false));
        Ok(DisplayOpen {
            handle,
            lib: "QGPL".into(),
        })
    }

    fn write(&mut self, handle: u32, format: &str, record: &[u8], indicators: &[u8]) -> Result<(), IoFail> {
        let s = &mut *self.0.borrow_mut();
        let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(format))?;
        rt.write(&mut s.model, &s.env, format, record, indicators).map_err(|_| bad(format))
    }

    fn read(&mut self, handle: u32, format: &str) -> Result<DisplayInput, IoFail> {
        let s = &mut *self.0.borrow_mut();
        let format = {
            let rt = s.files.get(&handle).ok_or_else(|| not_open(format))?;
            if rt.file().record(format).is_some() {
                format.to_string()
            } else {
                rt.last_format().unwrap_or_default()
            }
        };
        loop {
            s.show()?;
            let Some((aid, cursor)) = s.answer()? else {
                let mut f = IoFail::new(IoFail::REQUEST_ENDED, "CPF1907", &[]);
                f.data.clear();
                return Err(f);
            };
            let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(&format))?;
            match rt.input(&mut s.model, &s.env, &format, aid, cursor) {
                Ok(i) => return Ok(input_of(&format, i)),
                Err(Problem::NotShown(_)) => return Err(bad(&format)),
                Err(p) => rt.show_problem(&mut s.model, &p),
            }
        }
    }

    fn sfl_write(&mut self, handle: u32, format: &str, rrn: u32, record: &[u8], indicators: &[u8]) -> Result<(), IoFail> {
        let s = &mut *self.0.borrow_mut();
        let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(format))?;
        rt.sfl_write(format, rrn as usize, record, indicators).map_err(|_| bad(format))
    }

    fn sfl_chain(&mut self, handle: u32, format: &str, rrn: u32) -> Result<Option<DisplayInput>, IoFail> {
        let s = &mut *self.0.borrow_mut();
        let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(format))?;
        Ok(rt.sfl_chain(format, rrn as usize).map_err(|_| bad(format))?.map(|i| input_of(format, i)))
    }

    fn sfl_update(&mut self, handle: u32, format: &str, record: &[u8], indicators: &[u8]) -> Result<(), IoFail> {
        let s = &mut *self.0.borrow_mut();
        let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(format))?;
        match rt.sfl_update(format, record, indicators) {
            Ok(true) => Ok(()),
            Ok(false) => Err(IoFail::new(1221, "CPF5034", &[format, "QGPL"])),
            Err(_) => Err(bad(format)),
        }
    }

    fn sfl_readc(&mut self, handle: u32, format: &str) -> Result<Option<DisplayInput>, IoFail> {
        let s = &mut *self.0.borrow_mut();
        let rt = s.files.get_mut(&handle).ok_or_else(|| not_open(format))?;
        Ok(rt.sfl_readc(format).map_err(|_| bad(format))?.map(|i| input_of(format, i)))
    }

    fn close(&mut self, handle: u32) -> Result<(), IoFail> {
        self.0.borrow_mut().files.remove(&handle);
        Ok(())
    }
}
