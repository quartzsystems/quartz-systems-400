// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The keys that are the system's wherever a job waits at its display
//! (`Surface::wait`, `Host::wait_input`, a running program's attention
//! hook): System Request shows the System Request menu, Attention runs
//! the attention program, Print prints the screen. The screen is put
//! aside first and put back after.
//!
//! Ending the request (System Request option 2) and signing off unwind:
//! every wait returns `None` until the request ends (its `Request`
//! guard drops) or, for a sign-off, until the job does. A program that
//! is reading its display file is cancelled to its control boundary.

use std::cell::{Cell, RefCell};

use mi::Job;
use wm::dev::key;

use super::{attn, print, sysreq};
use crate::command::{Outcome, RunMode};
use crate::console::host::Host;

/// The escape a cancelled request's caller receives.
pub const ENDED: &str = "CPF1907";

/// What the waiting code does after a system key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// Go on waiting: the screen is back.
    Resume,
    /// Return `None`: the request ended or the job is ending.
    Unwind,
}

/// How a system display was left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Back to the job, its screen as it was.
    Back,
    /// Back, with a message on the restored screen.
    Say(String),
    /// End the current request.
    EndRequest,
    /// Sign off: the job ends.
    SignOff,
    /// The device is gone or the job is to end.
    Gone,
}

thread_local! {
    /// The system displays showing: 1 System Request, 2 Attention.
    static BUSY: Cell<u8> = const { Cell::new(0) };
    /// Requests running, outermost first.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
    /// The request being ended, by its depth.
    static CANCEL: Cell<Option<u32>> = const { Cell::new(None) };
    /// The job signs off.
    static ENDING: Cell<bool> = const { Cell::new(false) };
    /// The runtime job of the System Request and Attention levels.
    static LEVEL: RefCell<Option<Box<Job>>> = const { RefCell::new(None) };
}

/// Whether a key is the system's.
#[must_use]
pub fn is_system(k: u8) -> bool {
    matches!(k, key::SYSREQ | key::ATTN | key::PRINT)
}

/// Whether waits return `None` now.
#[must_use]
pub fn unwinding() -> bool {
    ENDING.get() || CANCEL.get().is_some()
}

/// Whether a request is being ended.
#[must_use]
pub fn cancelling() -> bool {
    CANCEL.get().is_some()
}

/// The job signs off from inside a command (`ENDGRPJOB` of itself, a
/// display that went away): every wait returns `None` from now on.
pub fn sign_off() {
    ENDING.set(true);
}

/// A display a command showed found the device gone or the job told
/// to end: the job signs off, unless a request is merely being ended.
pub fn gone() {
    if !unwinding() {
        ENDING.set(true);
    }
}

/// A request running at the command line or a menu: System Request
/// option 2 ends the innermost one.
pub struct Request(u32);

impl Request {
    /// A request starts.
    #[must_use]
    pub fn enter() -> Self {
        let d = DEPTH.get() + 1;
        DEPTH.set(d);
        Self(d)
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if CANCEL.get().is_some_and(|c| c >= self.0) {
            CANCEL.set(None);
        }
        DEPTH.set(self.0 - 1);
    }
}

/// End the running program's request: its entries down to the control
/// boundary are cancelled.
pub fn cancel_running(job: &mut Job) {
    job.cancel_request(ENDED, "QCPFMSG", Vec::new());
}

/// Take a system key at a wait.
pub fn take(host: &Host, k: u8) -> After {
    match k {
        key::PRINT => {
            print::screen(host);
            After::Resume
        }
        key::SYSREQ => shown(host, 1, sysreq::run),
        key::ATTN => shown(host, 2, attn::run),
        _ => After::Resume,
    }
}

/// Run a system display with the screen put aside; a second press of
/// its key while it shows is ignored.
fn shown(host: &Host, bit: u8, f: fn(&Host) -> Act) -> After {
    if BUSY.get() & bit != 0 || host.display.borrow().is_none() {
        put_back(host, None, None);
        return After::Resume;
    }
    let saved = host.display.borrow().as_ref().map(crate::display::Display::snapshot);
    BUSY.set(BUSY.get() | bit);
    let act = f(host);
    BUSY.set(BUSY.get() & !bit);
    match act {
        Act::Back => {
            put_back(host, saved, None);
            After::Resume
        }
        Act::Say(m) => {
            put_back(host, saved, Some(&m));
            After::Resume
        }
        Act::EndRequest if DEPTH.get() > 0 => {
            CANCEL.set(Some(DEPTH.get()));
            put_back(host, saved, None);
            After::Unwind
        }
        Act::EndRequest => {
            put_back(host, saved, Some("There is no request to end."));
            After::Resume
        }
        Act::SignOff | Act::Gone => {
            ENDING.set(true);
            After::Unwind
        }
    }
}

/// The screen back as it was, with a message on its message line.
pub fn put_back(host: &Host, saved: Option<crate::display::Snapshot>, message: Option<&str>) {
    let mut d = host.display.borrow_mut();
    let Some(d) = d.as_mut() else {
        return;
    };
    if let Some(mut s) = saved {
        if let Some(m) = message {
            s.message(m);
        }
        d.restore(s);
    } else {
        let reading = d.model.is_reading();
        d.flush(if reading {
            tn5250::screen::Read::MdtFields
        } else {
            tn5250::screen::Read::None
        });
    }
}

/// Run `f` with the System Request level's runtime job, built on first
/// use beside the job's own (whose runtime may be busy in a program).
pub fn with_level<T>(host: &Host, f: impl FnOnce(&mut Job) -> T) -> T {
    let mut job = LEVEL.with(|l| l.borrow_mut().take()).unwrap_or_else(|| {
        let (mut job, _provider) = crate::console::build_job(host);
        let a = host.attrs();
        job.ident.libl = a.libl.search();
        job.ident.curlib = a.libl.cur;
        job
    });
    let r = f(&mut job);
    if !job.log_outbox.is_empty() {
        let entries = std::mem::take(&mut job.log_outbox);
        host.link
            .borrow_mut()
            .with(|l| stg::Client::joblog_put(l, entries));
    }
    LEVEL.with(|l| *l.borrow_mut() = Some(job));
    r
}

/// Run a command line at the System Request level; what it said.
pub fn command(host: &Host, text: &str, mode: RunMode) -> Outcome {
    with_level(host, |job| {
        job.log_request(text, ("QWSSYSRQ", "QSYS"));
        let setting = cmd::analyze::Setting {
            interactive: true,
            ..cmd::analyze::Setting::default()
        };
        crate::command::run_with(job, host, text, mode, setting)
    })
}

/// The message a command left: its escape, else the first line of its
/// output when that starts with a message identifier.
#[must_use]
pub fn message_of(o: &Outcome) -> Option<String> {
    let first = o.text.lines().find(|l| !l.trim().is_empty()).map(str::trim);
    if let Some(f) = &o.escape {
        if !f.text.trim().is_empty() {
            return Some(format!("{} {}", f.msgid, f.text.trim()));
        }
        return Some(first.map_or_else(|| f.msgid.clone(), str::to_string));
    }
    let line = first?;
    crate::command::jobsite::leading_msgid(line).map(|_| line.to_string())
}
