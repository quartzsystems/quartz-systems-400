// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The displays around the edit display: session defaults, find and
//! change options, browse options, Exit, the system command window and
//! Select Prompt, each written from the model's values and read back.

use dspf::rt::{Env, Runtime};
use tn5250::ds::attr;
use tn5250::screen::ScreenModel;

use super::{compiled, field_text, keys, put};
use crate::defaults::{Defaults, Page1, Page2, date_text, parse_date};
use crate::exit::ExitForm;
use crate::find::{DateCmp, Direction, FindOptions, Scope};
use crate::session::Mode;

/// The open display files of one session.
pub struct Screens {
    /// The edit and browse display.
    pub seu: Runtime,
    dft: Runtime,
    fnd: Runtime,
    brw: Runtime,
    exit: Runtime,
    sys: Runtime,
    sel: Runtime,
}

/// The browse options display's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowseForm {
    /// 1 member, 2 spooled file, 3 output queue.
    pub selection: String,
    /// Copy all records (edit mode).
    pub copy_all: String,
    /// The member.
    pub member: String,
    /// Its file.
    pub file: String,
    /// Its library.
    pub lib: String,
    /// The spooled file.
    pub splf: String,
    /// Its job.
    pub job: String,
    /// The job's user.
    pub user: String,
    /// The job's number.
    pub job_number: String,
    /// The spooled file's number.
    pub splf_number: String,
    /// The output queue.
    pub outq: String,
    /// Its library.
    pub outq_lib: String,
}

impl BrowseForm {
    /// The display for a session's member and user.
    #[must_use]
    pub fn new(member: &str, file: &str, lib: &str, user: &str) -> Self {
        Self {
            selection: "1".into(),
            copy_all: "N".into(),
            member: member.into(),
            file: file.into(),
            lib: lib.into(),
            splf: member.into(),
            job: member.into(),
            user: user.into(),
            job_number: "*LAST".into(),
            splf_number: "*LAST".into(),
            outq: "QPRINT".into(),
            outq_lib: "*LIBL".into(),
        }
    }
}

fn kv(k: &(keys::Legend, keys::Legend)) -> [(&'static str, String); 2] {
    [("KEYS1", k.0.text(2)), ("KEYS2", k.1.text(2))]
}

/// Write a record with its values and its keys.
fn put_with_keys(
    rt: &mut Runtime,
    m: &mut ScreenModel,
    env: &Env,
    format: &str,
    values: &[(&str, String)],
    k: &(keys::Legend, keys::Legend),
) {
    let mut all: Vec<(&str, String)> = values.to_vec();
    all.extend(kv(k));
    let refs: Vec<(&str, &str)> = all.iter().map(|(n, v)| (*n, v.as_str())).collect();
    put(rt, m, env, format, &refs, &[]);
}

fn yn(b: bool) -> String {
    if b { "Y" } else { "N" }.into()
}

impl Screens {
    /// The display files, for a 24x80 or 27x132 display.
    #[must_use]
    pub fn new(wide: bool) -> Self {
        let rt = |t: &str| Runtime::new(compiled(t), wide);
        Self {
            seu: rt(dspf::sys::QSUSEU),
            dft: rt(dspf::sys::QSUDFT),
            fnd: rt(dspf::sys::QSUFND),
            brw: rt(dspf::sys::QSUBRW),
            exit: rt(dspf::sys::QSUEXIT),
            sys: rt(dspf::sys::QSUSYS),
            sel: rt(dspf::sys::QSUSEL),
        }
    }

    /// The session defaults: page 0 or 1 in edit mode, the one page in
    /// browse mode.
    pub fn show_defaults(&mut self, m: &mut ScreenModel, env: &Env, mode: Mode, page: usize, d: &Defaults) {
        let k = keys::defaults(mode);
        let p1 = d.page1();
        let p2 = d.page2();
        match (mode, page) {
            (Mode::Browse, _) => put_with_keys(
                &mut self.dft,
                m,
                env,
                "DFTB",
                &[
                    ("ROLL", p1.roll),
                    ("FULL", p1.full),
                    ("EXITPGM", p2.exit_pgm),
                    ("FILTER", p2.filter),
                ],
                &k,
            ),
            (Mode::Edit, 0) => put_with_keys(
                &mut self.dft,
                m,
                env,
                "DFT1",
                &[
                    ("ROLL", p1.roll),
                    ("UPPER", p1.upper),
                    ("TABS", p1.tabs),
                    ("INCR", p1.increment),
                    ("FULL", p1.full),
                    ("SRCTYPE", p1.srctype),
                    ("SYNTAX", p1.syntax),
                    ("FROMSEQ", p1.from),
                    ("TOSEQ", p1.to),
                    ("SETDATE", p1.date),
                ],
                &k,
            ),
            (Mode::Edit, _) => put_with_keys(
                &mut self.dft,
                m,
                env,
                "DFT2",
                &[
                    ("RESEQ", p2.reseq),
                    ("UPUNK", p2.upper_unknown),
                    ("EXITPGM", p2.exit_pgm),
                    ("FILTER", p2.filter),
                ],
                &k,
            ),
        }
    }

    /// The first page's values as typed (browse mode's page fills
    /// what it shows, the rest from `d`).
    #[must_use]
    pub fn read_page1(m: &ScreenModel, mode: Mode, d: &Defaults) -> Page1 {
        let f = |r, c| field_text(m, r, c);
        let mut p = d.page1();
        if mode == Mode::Browse {
            p.roll = f(5, 43);
            p.full = f(8, 43);
            return p;
        }
        Page1 {
            roll: f(5, 43),
            upper: f(8, 43),
            tabs: f(9, 43),
            increment: f(10, 43),
            full: f(11, 43),
            srctype: f(13, 43),
            syntax: f(15, 43),
            from: f(16, 43),
            to: f(17, 43),
            date: f(20, 43),
        }
    }

    /// The second page's values (browse mode: the exit program fields).
    #[must_use]
    pub fn read_page2(m: &ScreenModel, mode: Mode, d: &Defaults) -> Page2 {
        let f = |r, c| field_text(m, r, c);
        let mut p = d.page2();
        if mode == Mode::Browse {
            p.exit_pgm = f(10, 43);
            p.filter = f(11, 43);
            return p;
        }
        Page2 {
            reseq: f(5, 43),
            upper_unknown: f(8, 43),
            exit_pgm: f(10, 43),
            filter: f(11, 43),
        }
    }

    /// Find (and change) options.
    pub fn show_find(&mut self, m: &mut ScreenModel, env: &Env, mode: Mode, o: &FindOptions, width: usize) {
        let occ = match o.dir {
            Direction::Next => "1",
            Direction::All => "2",
            Direction::Previous => "3",
            Direction::First => "4",
            Direction::Last => "5",
        };
        let recs = match o.scope {
            Scope::All => "1",
            Scope::Excluded => "2",
            Scope::NotExcluded => "3",
        };
        let (date, cmp) = match o.date {
            Some((d, c)) => (
                date_text(Some(d)),
                match c {
                    DateCmp::Less => "1",
                    DateCmp::Equal => "2",
                    DateCmp::Greater => "3",
                }
                .to_string(),
            ),
            None => ("00/00/00".into(), String::new()),
        };
        let mut v = vec![
            ("FIND", o.find.clone()),
            ("FROMCOL", o.from.to_string()),
            ("TOCOL", o.to.unwrap_or(width).to_string()),
            ("OCC", occ.into()),
            ("RECS", recs.into()),
            ("MATCH", if o.same_case { "1" } else { "2" }.into()),
            ("SDATE", date),
            ("CMP", cmp),
        ];
        let format = if mode == Mode::Edit {
            v.push(("CHANGE", o.change.clone()));
            v.push(("SHIFT", yn(o.shift)));
            "FNDCHG"
        } else {
            "FND"
        };
        put_with_keys(&mut self.fnd, m, env, format, &v, &keys::find(mode));
    }

    /// The find options as typed; the message of a refused value.
    pub fn read_find(m: &ScreenModel, mode: Mode, old: &FindOptions) -> Result<FindOptions, String> {
        let f = |r, c| field_text(m, r, c);
        let rows: [usize; 7] = if mode == Mode::Edit {
            [7, 8, 9, 11, 13, 17, 18]
        } else {
            [6, 7, 8, 10, 12, 15, 16]
        };
        let mut o = old.clone();
        o.find = f(5, 38);
        if mode == Mode::Edit {
            o.change = f(6, 38);
            o.shift = match f(15, 38).to_ascii_uppercase().as_str() {
                "Y" => true,
                "N" => false,
                _ => return Err("Type Y or N for the data shift.".into()),
            };
        }
        let num = |s: String, what: &str| -> Result<usize, String> {
            s.trim().parse::<usize>().map_err(|_| format!("Type a number for the {what}."))
        };
        o.from = num(f(rows[0], 38), "first column")?.max(1);
        let to = f(rows[1], 38);
        o.to = if to.trim().is_empty() { None } else { Some(num(to, "last column")?) };
        o.dir = match f(rows[2], 38).trim() {
            "1" => Direction::Next,
            "2" => Direction::All,
            "3" => Direction::Previous,
            "4" => Direction::First,
            "5" => Direction::Last,
            _ => return Err("Type 1, 2, 3, 4 or 5 for the occurrences.".into()),
        };
        o.scope = match f(rows[3], 38).trim() {
            "1" => Scope::All,
            "2" => Scope::Excluded,
            "3" => Scope::NotExcluded,
            _ => return Err("Type 1, 2 or 3 for the records to search.".into()),
        };
        o.same_case = match f(rows[4], 38).trim() {
            "1" => true,
            "2" => false,
            _ => return Err("Type 1 or 2 for the kind of match.".into()),
        };
        let date = f(rows[5], 38);
        let cmp = f(rows[6], 38);
        o.date = match (parse_date(&date), cmp.trim()) {
            (Some(Some(0) | None), _) | (_, "") => None,
            (Some(Some(d)), "1") => Some((d, DateCmp::Less)),
            (Some(Some(d)), "2") => Some((d, DateCmp::Equal)),
            (Some(Some(d)), "3") => Some((d, DateCmp::Greater)),
            (None, _) => return Err("Type the date as YY/MM/DD or YYMMDD.".into()),
            _ => return Err("Type 1, 2 or 3 for the comparison.".into()),
        };
        Ok(o)
    }

    /// Browse (and copy) options.
    pub fn show_browse(&mut self, m: &mut ScreenModel, env: &Env, mode: Mode, b: &BrowseForm) {
        let mut v = vec![
            ("SEL", b.selection.clone()),
            ("BMBR", b.member.clone()),
            ("BFILE", b.file.clone()),
            ("BLIB", b.lib.clone()),
            ("SPLF", b.splf.clone()),
            ("JOB", b.job.clone()),
            ("USER", b.user.clone()),
            ("JOBNBR", b.job_number.clone()),
            ("SPLNBR", b.splf_number.clone()),
            ("OUTQ", b.outq.clone()),
            ("OUTQLIB", b.outq_lib.clone()),
        ];
        let format = if mode == Mode::Edit {
            v.push(("COPYALL", b.copy_all.clone()));
            "BRWCPY"
        } else {
            "BRW"
        };
        put_with_keys(&mut self.brw, m, env, format, &v, &keys::browse(mode));
    }

    /// The browse options as typed.
    #[must_use]
    pub fn read_browse(m: &ScreenModel, mode: Mode) -> BrowseForm {
        let f = |r, c| field_text(m, r, c);
        BrowseForm {
            selection: f(5, 38),
            copy_all: if mode == Mode::Edit { f(8, 38) } else { "N".into() },
            member: f(9, 38),
            file: f(10, 40),
            lib: f(11, 42),
            splf: f(13, 38),
            job: f(14, 40),
            user: f(15, 42),
            job_number: f(16, 42),
            splf_number: f(17, 40),
            outq: f(19, 38),
            outq_lib: f(20, 40),
        }
    }

    /// The Exit display.
    pub fn show_exit(&mut self, m: &mut ScreenModel, env: &Env, e: &ExitForm) {
        put_with_keys(
            &mut self.exit,
            m,
            env,
            "EXIT",
            &[
                ("CHGMBR", e.change.clone()),
                ("MBR", e.member.clone()),
                ("FILE", e.file.clone()),
                ("LIB", e.lib.clone()),
                ("TEXT", e.text.clone()),
                ("RESEQ", e.reseq.clone()),
                ("START", e.start.clone()),
                ("INCR", e.increment.clone()),
                ("PRINT", e.print.clone()),
                ("RETURN", e.ret.clone()),
                ("MBRLIST", e.list.clone()),
            ],
            &keys::exit(),
        );
        m.set_cursor(5, 42);
    }

    /// The Exit display as typed.
    #[must_use]
    pub fn read_exit(m: &ScreenModel) -> ExitForm {
        let f = |r, c| field_text(m, r, c);
        ExitForm {
            change: f(5, 42),
            member: f(6, 42),
            file: f(7, 42),
            lib: f(8, 44),
            text: f(9, 42),
            reseq: f(11, 42),
            start: f(12, 42),
            increment: f(13, 42),
            print: f(15, 42),
            ret: f(17, 42),
            list: f(19, 42),
        }
    }

    /// The system command window over the edit display, with a
    /// command and a message (the copyright when there is none).
    pub fn show_window(&mut self, m: &mut ScreenModel, env: &Env, cmd: &str, message: Option<&str>) {
        let msg = message.unwrap_or("(C) COPYRIGHT QUARTZ SYSTEMS 2026.");
        let wkeys = keys::window().text(5);
        put(&mut self.sys, m, env, "SYSCMD", &[("CMD", cmd), ("WKEYS", &wkeys), ("WMSG", msg)], &[]);
        m.set_cursor(21, 10);
    }

    /// The command typed in the window.
    #[must_use]
    pub fn read_window(m: &ScreenModel) -> String {
        field_text(m, 21, 10)
    }

    /// Select Prompt, with the formats of the session's type listed.
    pub fn show_select(&mut self, m: &mut ScreenModel, env: &Env, list: &[String], message: Option<&str>) {
        let names: Vec<String> = (7..=20).map(|r| format!("LIST{r:02}")).collect();
        let mut v: Vec<(&str, String)> = vec![("PTYPE", String::new())];
        for (i, n) in names.iter().enumerate() {
            v.push((n.as_str(), list.get(i).cloned().unwrap_or_default()));
        }
        put_with_keys(&mut self.sel, m, env, "SELECT", &v, &keys::select());
        if let Some(msg) = message {
            m.error_line(msg, attr::WHITE);
        }
        m.set_cursor(5, 40);
    }

    /// The prompt type chosen.
    #[must_use]
    pub fn read_select(m: &ScreenModel) -> String {
        field_text(m, 5, 40).trim().to_ascii_uppercase()
    }
}
