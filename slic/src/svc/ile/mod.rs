// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ILE on the service console: the service job that runs programs,
//! the sources it reads objects through, and the commands that
//! compile, bind, call, display and debug. While a program runs the
//! console lends the job its store, its temporary ASP and the system
//! values, and takes them back when the call returns; a debug stop
//! reads its commands from the serial line inside the call.

pub mod commands;
mod provider;
mod sources;

use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::Write as _;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

use mi::Job;
use mi::debug::{Resume, Stop};
use qsname::Name;
use store::obj::ops::{self, NameContext};
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;
use sysval::SysVals;

use crate::ipl::Event;
use crate::ipl::storage::{Storage, Temp};
use crate::ipl::system::System;

pub use provider::StoreProvider;

/// What the console lends the job while a program runs.
pub struct Lent {
    /// ASP 1 and its store.
    pub storage: Storage,
    /// The temporary ASP.
    pub temp: Option<Temp>,
    /// The system values.
    pub sysvals: Option<SysVals>,
}

/// The lent resources, shared with the job's sources.
pub type Shared = Rc<RefCell<Option<Lent>>>;

/// The console's ILE state.
pub struct State {
    /// The service job, once a program has run.
    pub job: Option<Box<Job>>,
    /// The lent resources.
    pub shared: Shared,
    /// The system APIs' provider.
    pub provider: Rc<RefCell<StoreProvider>>,
}

impl Default for State {
    fn default() -> Self {
        let shared: Shared = Rc::new(RefCell::new(None));
        Self {
            job: None,
            provider: Rc::new(RefCell::new(StoreProvider::new(Rc::clone(&shared)))),
            shared,
        }
    }
}

/// The help lines.
pub const HELP: &str = "\
qir asm LIB/SRCPF MBR MODULE(LIB/M) | qir asm NAME MODULE(LIB/M)   assemble a member or an xfer buffer
qir check NAME | qir dis LIB/MODULE                                verify a buffer; print a module's IR
crtpgm LIB/PGM MODULE(LIB/M ...) [BNDDIR(LIB/B ...) ACTGRP(*NEW|*CALLER|NAME) ENTMOD(M) OPTION(*UNRSLVREF) TEXT('..')]
crtsrvpgm LIB/S MODULE(...) [EXPORT(*ALL|*SRCFILE) SRCFILE(LIB/F) SRCMBR(M) BNDDIR(...) ACTGRP(...) TEXT('..')]
updpgm LIB/PGM MODULE(LIB/M ...) | crtbnddir LIB/B | addbnddire LIB/B OBJ((LIB/N *MODULE|*SRVPGM) ...)
call LIB/PGM ['text' 123 x'C1C2' ...]      run a program; parameters as CL types them
dsppgm | dspsrvpgm | dspmod | dsppgmref | dspbnddir LIB/NAME
dspactgrp | rclactgrp NAME|*ELIGIBLE | dspcallstk | dspmsgq [*PGM|*EXT|QSYSOPR] | rpy KEY REPLY | dmp
wrkmsgd LIB/MSGF | dspmsgd ID [MSGF(LIB/F)]
strdbg LIB/PGM | addbkp N [PGM(LIB/PGM)] | rmvbkp N|*ALL | dspbkp | enddbg
   at a stop: step [over|into], go, eval VAR[:x|:c], dspvar, dspcallstk, addbkp N, rmvbkp N, dmp
";

/// Run `f` in a transaction on the lent main store.
pub fn with_main<T>(
    shared: &Shared,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let mut b = shared.borrow_mut();
    let lent = b
        .as_mut()
        .ok_or("the object store is not available to the job")?;
    let qsys = lent.storage.qsys.ok_or("object store not initialized")?;
    let mut txn = lent.storage.store.begin().map_err(|e| e.to_string())?;
    match f(&mut txn, qsys) {
        Ok(v) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

/// Run `f` in a transaction on the lent temporary store.
pub fn with_temp<T>(
    shared: &Shared,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let mut b = shared.borrow_mut();
    let lent = b
        .as_mut()
        .ok_or("the object store is not available to the job")?;
    let temp = lent.temp.as_mut().ok_or("QTEMP is not available")?;
    let qtemp = temp.qtemp;
    let mut txn = temp.store.begin().map_err(|e| e.to_string())?;
    match f(&mut txn, qtemp) {
        Ok(v) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

/// The service job's library list: `QSYS` then `QGPL`, the current
/// library `QGPL`.
pub fn name_context(txn: &mut Txn<'_>, qsys: ObjectId) -> NameContext {
    let qgpl = ops::find_library(txn, qsys, Name::parse("QGPL").expect("valid"))
        .map(|f| f.id)
        .ok();
    NameContext {
        curlib: qgpl,
        libl: std::iter::once(qsys).chain(qgpl).collect(),
    }
}

/// Lend the store, the temporary ASP and the system values to the job.
fn lend(system: &mut System) -> Result<(), String> {
    let storage = system.storage.take().ok_or("object store not open")?;
    *system.svc.ile.shared.borrow_mut() = Some(Lent {
        storage,
        temp: system.temp.take(),
        sysvals: system.sysvals.take(),
    });
    Ok(())
}

/// Take them back.
fn restore(system: &mut System) {
    if let Some(l) = system.svc.ile.shared.borrow_mut().take() {
        system.storage = Some(l.storage);
        system.temp = l.temp;
        system.sysvals = l.sysvals;
    }
}

/// The job, created on first use with its sources.
fn job(system: &mut System) -> &mut Job {
    if system.svc.ile.job.is_none() {
        let shared = Rc::clone(&system.svc.ile.shared);
        let mut job = Job::boxed(Box::new(sources::StoreObjects::new(Rc::clone(&shared))));
        job.usrspcs
            .set_source(Box::new(sources::StoreUserSpaces::new(Rc::clone(&shared))));
        job.msgs = Box::new(sources::StoreMsgs::new(Rc::clone(&shared)));
        job.data = Some(Box::new(sources::StoreData::new(Rc::clone(&shared))));
        let resolver_shared = Rc::clone(&shared);
        let resolve = move |fr: &dds::FileRef| -> db::Result<ObjectId> {
            with_main(&resolver_shared, |txn, qsys| {
                let names = name_context(txn, qsys);
                ops::resolve(txn, qsys, &names, fr.library, fr.file, types::FILE)
                    .map(|f| f.id)
                    .map_err(|e| e.to_string())
            })
            .map_err(|what| db::DbError::Definition { what })
        };
        job.io = Some(Box::new(mi::io::store::StoreIo::new(
            Box::new(sources::MainStore::new(Rc::clone(&shared))),
            JobHandle(1),
            Box::new(resolve),
        )));
        let provider: Rc<RefCell<dyn sysapi::Provider>> = system.svc.ile.provider.clone();
        sysapi::install(&mut job, provider);
        if let Some(events) = system.events.clone() {
            let serial = system.out.serial_clone();
            job.set_debug_hook(Box::new(move |job: &mut Job, stop: &Stop| {
                debug_session(job, stop, &events, serial.as_ref())
            }));
        }
        system.svc.ile.job = Some(job);
    }
    system.svc.ile.job.as_mut().expect("created")
}

/// Run `f` on the job with the store lent to it; the commands
/// `QCMDEXC` asked for run afterwards and their output follows.
pub fn run_in_job<T>(
    system: &mut System,
    f: impl FnOnce(&mut Job) -> T,
) -> Result<(T, String), String> {
    job(system);
    lend(system)?;
    let mut j = system.svc.ile.job.take().expect("created");
    let r = f(&mut j);
    system.svc.ile.job = Some(j);
    restore(system);
    let pending: Vec<String> = std::mem::take(&mut system.svc.ile.provider.borrow_mut().pending);
    let mut extra = String::new();
    for line in pending {
        let _ = writeln!(extra, "QCMDEXC: {line}");
        extra.push_str(&super::reply(system, &line));
    }
    Ok((r, extra))
}

/// The debug session at a stop: commands from the serial line until
/// one resumes.
fn debug_session(
    job: &mut Job,
    stop: &Stop,
    events: &Receiver<Event>,
    serial: Option<&std::fs::File>,
) -> Resume {
    let out = |s: &str| {
        if let Some(mut f) = serial {
            let _ = f.write_all(s.as_bytes());
        }
    };
    out(&format!(
        "Stopped at {} {} statement {}\n",
        stop.program, stop.procedure, stop.stmt
    ));
    loop {
        out("dbg> ");
        let line = loop {
            match events.recv() {
                Ok(Event::Line(l)) => break l,
                Ok(Event::Signal(_)) => {
                    crate::platform::sys::reap();
                }
                Ok(_) => {}
                Err(_) => return Resume::Go,
            }
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            [] => {}
            ["step"] | ["step", "over"] => return Resume::StepOver,
            ["step", "into"] => return Resume::StepInto,
            ["go" | "resume" | "g"] => return Resume::Go,
            ["eval", expr] => match job.eval(expr) {
                Ok(v) => out(&format!("{expr} = {v}\n")),
                Err(e) => out(&format!("{e}\n")),
            },
            ["dspvar"] => {
                for (n, t) in job.dspvar() {
                    out(&format!("  {n:<12} {t}\n"));
                }
            }
            ["dspcallstk"] => out(&job.call_stack_text()),
            ["addbkp", n] => match n.parse::<u32>() {
                Ok(n) => {
                    job.addbkp(&stop.program, n);
                    out(&format!("Breakpoint at {} statement {n}.\n", stop.program));
                }
                Err(_) => out("addbkp N\n"),
            },
            ["rmvbkp", n] => {
                let gone = match n.parse::<u32>() {
                    Ok(n) => job.rmvbkp(&stop.program, Some(n)),
                    Err(_) => job.rmvbkp(&stop.program, None),
                };
                out(&format!("{gone} breakpoint(s) removed.\n"));
            }
            ["dmp"] => out(&job.dump(true)),
            ["help"] => out(
                "step [over|into], go, eval VAR[:x|:c], dspvar, dspcallstk, addbkp N, rmvbkp N|*ALL, dmp\n",
            ),
            _ => out("Not a debug command. Type help.\n"),
        }
    }
}

/// A console command of this module, or none.
pub fn command(system: &mut System, cmd: &str, args: &[&str], line: &str) -> Option<String> {
    commands::command(system, cmd, args, line)
}
