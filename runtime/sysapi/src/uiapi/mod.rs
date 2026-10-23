// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM APIs: a program opens a panel group as an application
//! (`QUIOPNDA`, `QUIOPNPA`), fills its dialog variables and lists
//! (`QUIPUTV`, `QUIADDLE`, `QUISETLA`, ...), shows panels (`QUIDSPP`)
//! or prints them (`QUIPRTP`), reads back what the operator typed
//! (`QUIGETV`, `QUIGETLE`) and closes it (`QUICLOA`); `QUHDSPH` and
//! `QUHPRTH` show and print help without one. The dialog is the
//! engine of `uim::engine`; the job's display and its panel groups
//! come from a [`UimHost`]. Errors are the `CPF6Axx` and `CPF6Exx`
//! messages through the error code parameter (`docs/UIM.md` §10).

mod buffer;
mod display;
mod lists;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use mi::Job;
use uim::PanelGroup;
use uim::engine::{Engine, IndexEntry, Input, Terminal};
use uim::render::Screen;

use crate::param::{Args, ErrCode};
use crate::provider::ApiFail;

/// What the APIs need from the job's surroundings.
pub trait UimHost {
    /// A panel group by library (`*LIBL` when blank) and name: the
    /// group and its real library.
    fn panel_group(&mut self, lib: &str, name: &str) -> Result<(Rc<PanelGroup>, String), String>;
    /// The job's display, `None` for a job without one.
    fn terminal(&mut self) -> Option<Box<dyn Terminal>>;
    /// The system name for `System:`.
    fn system_name(&mut self) -> String {
        "QS400".into()
    }
}

/// The host the installed natives share.
pub type SharedUimHost = Rc<RefCell<dyn UimHost>>;

/// An API of this module.
type UimFn = fn(&mut Job, &[u64], &SharedUimHost);

/// Every UIM API by name.
const APIS: &[(&str, UimFn)] = &[
    ("QUIOPNDA", quiopnda),
    ("QUIOPNPA", quiopnpa),
    ("QUICLOA", quicloa),
    ("QUIPUTV", quiputv),
    ("QUIGETV", quigetv),
    ("QUIADDLE", lists::quiaddle),
    ("QUIINSLE", lists::quiinsle),
    ("QUIUPDLE", lists::quiupdle),
    ("QUIGETLE", lists::quigetle),
    ("QUIRMVLE", lists::quirmvle),
    ("QUIDLTL", lists::quidltl),
    ("QUISETLA", lists::quisetla),
    ("QUIGETLA", lists::quigetla),
    ("QUIDSPP", display::quidspp),
    ("QUISETSA", display::quisetsa),
    ("QUIPRTP", display::quiprtp),
    ("QUIDSPH", display::quidsph),
    ("QUHDSPH", display::quhdsph),
    ("QUHPRTH", display::quhprth),
];

/// The names of the UIM APIs.
#[must_use]
pub fn names() -> Vec<&'static str> {
    APIS.iter().map(|(n, _)| *n).collect()
}

/// Install the UIM APIs into a job as natives of `QSYS`.
pub fn install(job: &mut Job, host: &SharedUimHost) {
    for (name, f) in APIS {
        let h = host.clone();
        let f = *f;
        job.natives.insert(
            (*name).to_string(),
            Rc::new(move |j: &mut Job, args: &[u64]| f(j, args, &h)),
        );
    }
}

/// Close every application of a job: printed output is spooled.
pub fn close_all(job: &mut Job) {
    let apps: Vec<App> = APPS.with(|m| {
        m.borrow_mut()
            .remove(&key(job))
            .map(|a| a.open.into_values().collect())
            .unwrap_or_default()
    });
    for a in apps {
        spool(job, a);
    }
}

/// What an application does.
pub(super) enum Kind {
    /// Shows panels.
    Display,
    /// Prints panels into this printer file.
    Print {
        /// The printer file.
        file: String,
        /// The lines printed so far.
        lines: Vec<String>,
    },
}

/// An open application.
pub(super) struct App {
    pub(super) engine: Engine,
    pub(super) pnlgrp: String,
    pub(super) lib: String,
    pub(super) handle: [u8; 8],
    pub(super) kind: Kind,
}

/// The applications of one job.
#[derive(Default)]
struct Apps {
    next: u32,
    open: BTreeMap<[u8; 8], App>,
}

thread_local! {
    /// Open applications by job address.
    static APPS: RefCell<HashMap<usize, Apps>> = RefCell::new(HashMap::new());
}

fn key(job: &Job) -> usize {
    std::ptr::from_ref(job) as usize
}

/// A failure with the UIM messages' data: panel group, library,
/// handle, the object (panel, list, record) and a variable.
pub(super) fn fail(id: &str, app: Option<&App>, name: &str, var: &str) -> ApiFail {
    let (g, l, h) = app.map_or((String::new(), String::new(), [0x40; 8]), |a| {
        (a.pnlgrp.clone(), a.lib.clone(), a.handle)
    });
    let mut f = ApiFail::id(id).text(&g, 10).text(&l, 10);
    f.data.extend_from_slice(&h);
    f.text(name, 10).text(var, 10)
}

/// Take an application out of the table while an API works on it (an
/// action may run a program that calls the APIs again).
pub(super) fn take(job: &Job, handle: &[u8]) -> Result<App, ApiFail> {
    let h: [u8; 8] = handle
        .get(..8)
        .and_then(|b| b.try_into().ok())
        .unwrap_or([0; 8]);
    APPS.with(|m| {
        m.borrow_mut()
            .get_mut(&key(job))
            .and_then(|a| a.open.remove(&h))
    })
    .ok_or_else(|| {
        let mut f = ApiFail::id("CPF6A0B").text("", 10).text("", 10);
        f.data.extend_from_slice(&h);
        f.text("", 10).text("", 10)
    })
}

/// Put an application back.
pub(super) fn put(job: &Job, app: App) {
    APPS.with(|m| {
        m.borrow_mut()
            .entry(key(job))
            .or_default()
            .open
            .insert(app.handle, app);
    });
}

/// Work on an application: take it, run `f`, put it back.
pub(super) fn with_app<R>(
    job: &mut Job,
    handle: &[u8],
    f: impl FnOnce(&mut Job, &mut App) -> Result<R, ApiFail>,
) -> Result<R, ApiFail> {
    let mut app = take(job, handle)?;
    let r = f(job, &mut app);
    put(job, app);
    r
}

/// A new application's handle: `UI` and six digits, in EBCDIC.
fn new_handle(job: &Job) -> [u8; 8] {
    let n = APPS.with(|m| {
        let mut m = m.borrow_mut();
        let a = m.entry(key(job)).or_default();
        a.next += 1;
        a.next
    });
    let b = ccsid::to_ebcdic(&format!("UI{:06}", n % 1_000_000)).unwrap_or_default();
    b.try_into().unwrap_or([0x40; 8])
}

/// Spool a print application's lines.
fn spool(job: &mut Job, app: App) {
    if let Kind::Print { file, lines } = app.kind
        && !lines.is_empty()
    {
        let _ = job.spool_text(&file, &lines.join("\n"));
    }
}

/// The terminal the APIs hand the engine: the host's display, with
/// other panel groups opened through the host and printing spooled to
/// `QSYSPRT` in the job.
pub(super) struct ApiTerm<'a> {
    pub(super) inner: &'a mut dyn Terminal,
    pub(super) host: SharedUimHost,
    pub(super) job: &'a mut Job,
}

impl Terminal for ApiTerm<'_> {
    fn wide(&mut self) -> bool {
        self.inner.wide()
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        self.inner.exchange(screen)
    }

    fn group(&mut self, lib: &str, name: &str) -> Option<Rc<PanelGroup>> {
        self.host
            .borrow_mut()
            .panel_group(lib, name)
            .ok()
            .map(|(g, _)| g)
    }

    fn search_index(&mut self, lib: &str, name: &str) -> Option<Vec<IndexEntry>> {
        self.inner.search_index(lib, name)
    }

    fn print(&mut self, _title: &str, rows: &[String]) -> Result<String, String> {
        self.job
            .spool_text("QSYSPRT", &rows.join("\n"))
            .map(|_| "Display printed to QSYSPRT.".to_string())
            .map_err(|e| format!("{} ended the print.", e.msgid))
    }
}

/// `(lib, name)` of a qualified name parameter, `*LIBL` for a blank
/// library.
fn qualified(a: &Args<'_>, i: usize) -> (String, String) {
    let (n, l) = a.qual(i);
    (if l.is_empty() { "*LIBL".into() } else { l }, n)
}

/// Open an application over a panel group.
fn open(job: &mut Job, host: &SharedUimHost, a: &Args<'_>, kind: Kind) -> Result<[u8; 8], ApiFail> {
    let (lib, name) = qualified(a, 1);
    let scope = a.bin4(2);
    if !matches!(scope, -1 | -2) {
        return Err(fail("CPF6A2A", None, &name, ""));
    }
    if !(0..=2).contains(&a.bin4(3)) {
        return Err(fail("CPF6A2E", None, &name, ""));
    }
    let (group, real) = host
        .borrow_mut()
        .panel_group(&lib, &name)
        .map_err(|_| ApiFail::id("CPF6A12").text(&name, 10).text(&lib, 10))?;
    let system = host.borrow_mut().system_name();
    let mut engine = Engine::new(group, &system);
    engine.incomplete_returns = true;
    let handle = new_handle(job);
    put(
        job,
        App {
            engine,
            pnlgrp: name,
            lib: real,
            handle,
            kind,
        },
    );
    Ok(handle)
}

/// `QUIOPNDA(handle out, panel group, scope, exit parameter interface,
/// full-screen help, errc)`: open a display application. The scope is
/// -1 (the caller's call level) or -2 (the job).
fn quiopnda(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let fsh = a.char(4, 1);
    if !matches!(fsh.as_str(), "Y" | "N" | "") {
        errc.fail(j, &fail("CPF6A2F", None, "", ""));
        return;
    }
    match open(j, host, &a, Kind::Display) {
        Ok(h) => {
            if fsh == "Y"
                && let Ok(mut app) = take(j, &h)
            {
                app.engine.full_screen_help = true;
                put(j, app);
            }
            a.put_bytes(0, &h);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIOPNPA(handle out, panel group, scope, exit parameter interface,
/// printer file, alternative file name, share open data path, user
/// data, errc)`: open a print application; what `QUIPRTP` prints is
/// spooled into the printer file (`QSYSPRT` by default) at close.
fn quiopnpa(j: &mut Job, args: &[u64], host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 8);
    let (_, prtf) = qualified(&a, 4);
    let alt = a.char(5, 10);
    let file = match (alt.as_str(), prtf.as_str()) {
        ("" | "*NONE" | "*PRTF", "") => "QSYSPRT".to_string(),
        ("" | "*NONE" | "*PRTF", f) => f.to_string(),
        (alt, _) => alt.to_string(),
    };
    match open(
        j,
        host,
        &a,
        Kind::Print {
            file,
            lines: Vec::new(),
        },
    ) {
        Ok(h) => {
            a.put_bytes(0, &h);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUICLOA(handle, close option, errc)`: `M` closes normally, `A`
/// abnormally (a print application's output is spooled either way).
fn quicloa(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 2);
    let opt = a.char(1, 1);
    if !matches!(opt.as_str(), "M" | "A" | "") {
        errc.fail(j, &fail("CPF6A4C", None, "", ""));
        return;
    }
    match take(j, &a.bytes(0, 8)) {
        Ok(app) => {
            spool(j, app);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// The variables of a record and their storage, in order.
fn record(app: &App, name: &str) -> Result<Vec<(String, buffer::Store, bool, bool)>, ApiFail> {
    let g = &app.engine.group;
    let rec = g
        .var_records
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| fail("CPF6A38", Some(app), name, ""))?;
    rec.vars
        .iter()
        .map(|v| {
            let st = buffer::store_of(g, v).ok_or_else(|| fail("CPF6A35", Some(app), name, v))?;
            Ok((v.clone(), st, rec.noget.contains(v), rec.noput.contains(v)))
        })
        .collect()
}

/// The record's variables read from a buffer.
fn read_record(app: &App, rec: &str, buf: &[u8]) -> Result<Vec<(String, String)>, ApiFail> {
    let vars = record(app, rec)?;
    let need: usize = vars.iter().map(|(_, s, ..)| s.len()).sum();
    if buf.len() < need {
        return Err(fail("CPF6A39", Some(app), rec, ""));
    }
    let mut out = Vec::new();
    let mut off = 0;
    for (v, st, _, noput) in vars {
        let b = &buf[off..off + st.len()];
        off += st.len();
        if noput {
            continue;
        }
        let val = buffer::decode(st, b).ok_or_else(|| fail("CPF6A36", Some(app), rec, &v))?;
        out.push((v, val));
    }
    Ok(out)
}

/// The record's variables written into a buffer from `value`.
fn write_record(
    app: &App,
    rec: &str,
    len: usize,
    value: &dyn Fn(&str) -> String,
) -> Result<Vec<u8>, ApiFail> {
    let vars = record(app, rec)?;
    let need: usize = vars.iter().map(|(_, s, ..)| s.len()).sum();
    if len < need {
        return Err(fail("CPF6A39", Some(app), rec, ""));
    }
    let mut out = Vec::with_capacity(need);
    for (v, st, noget, _) in vars {
        let text = if noget { String::new() } else { value(&v) };
        let b = buffer::encode(st, &text)
            .or_else(|| buffer::encode(st, ""))
            .ok_or_else(|| fail("CPF6A37", Some(app), rec, &v))?;
        out.extend(b);
    }
    Ok(out)
}

/// `QUIPUTV(handle, buffer, buffer length, variable record, errc)`:
/// set the record's variables from the buffer.
fn quiputv(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let rec = a.char(3, 10);
    let buf = a.bytes(1, len);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        for (v, val) in read_record(app, &rec, &buf)? {
            app.engine.set(&v, &val);
        }
        Ok(())
    });
    match r {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// `QUIGETV(handle, buffer out, buffer length, variable record, errc)`:
/// the record's variables into the buffer.
fn quigetv(j: &mut Job, args: &[u64], _host: &SharedUimHost) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let len = usize::try_from(a.bin4(2)).unwrap_or(0);
    let rec = a.char(3, 10);
    let r = with_app(j, &a.bytes(0, 8), |_, app| {
        let e = &app.engine;
        write_record(app, &rec, len, &|v| e.get(v))
    });
    match r {
        Ok(b) => {
            a.put_bytes(1, &b);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}
