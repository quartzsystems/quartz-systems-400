// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands that can run long take turns with other jobs. The service
//! runs each on a fiber of its own (`stgsvc::turns`): what it reads from
//! the job and the system is read first ([`prepare`]), its work then
//! runs on the store lent to it, committing what it did so far and
//! giving the store to the others at the database's pauses, and what
//! it writes back to the job (an `xfer` buffer, a stream file) is
//! written at the end. `docs/STORAGE_SERVICE.md` "Long statements".

use std::cell::RefCell;
use std::time::{Duration, Instant};

use cmd::block::Parms;
use sql::SqlContext;
use sql::embed::StoreLend;
use stg::proto::CmdReply;
use store::obj::JobHandle;
use store::txn::Txn;
use sys_cpps::Site;

use crate::ipl::system::System;
use crate::svc::db::{Params, files};

/// The commands that take turns.
pub const NAMES: &[&str] = &["CPYF", "RGZPFM", "CPYTOIMPF", "CPYFRMIMPF", "CRTLF", "RUNSQL", "RUNSQLSTM"];

/// A command's end, run with the system as one of the job's commands.
pub type Finish = Box<dyn FnOnce(&mut System) -> CmdReply>;

/// A command's work on the store lent to it.
pub type Long = Box<dyn FnOnce(&Work<'_>) -> Finish>;

/// A long command started: its work and its parameter block.
pub type Started = (Long, Vec<Option<Vec<u8>>>);

/// What a command's work runs with.
pub struct Work<'a> {
    lender: &'a dyn StoreLend,
    yielder: sql::exec::Yielder,
    turns: RefCell<Turns>,
}

/// When the work last gave a turn.
struct Turns {
    every: Duration,
    last: Option<Instant>,
    yields: u64,
}

impl<'a> Work<'a> {
    /// Work on `lender`'s store, `yielder` giving the others their turns.
    pub fn new(lender: &'a dyn StoreLend, yielder: sql::exec::Yielder) -> Self {
        Self {
            lender,
            yielder,
            turns: RefCell::new(Turns {
                every: sql::exec::YIELD_EVERY,
                last: None,
                yields: 0,
            }),
        }
    }

    /// Turns at most every `every` (5 ms by default).
    #[cfg(test)]
    pub fn every(self, every: Duration) -> Self {
        self.turns.borrow_mut().every = every;
        self
    }

    /// The store, lent per transaction.
    pub fn lender(&self) -> &'a dyn StoreLend {
        self.lender
    }

    /// The yielder, for the SQL engine's own turns.
    pub fn yielder(&self) -> sql::exec::Yielder {
        std::rc::Rc::clone(&self.yielder)
    }

    /// The database's pause ([`db::Pause`]): at most every 5 ms, what
    /// was done so far committed, the store given back and a turn for
    /// the others; the yielder may end the work ([`db::DbError::Ended`]).
    fn pause(&self, txn: &mut Txn<'_>) -> db::Result<bool> {
        let mut t = self.turns.borrow_mut();
        if t.last.is_some_and(|l| l.elapsed() < t.every) {
            return Ok(false);
        }
        txn.commit_part()?;
        if !txn.suspend() {
            return Ok(false);
        }
        let stop = (self.yielder)();
        txn.resume()?;
        t.yields += 1;
        t.last = Some(Instant::now());
        if stop {
            return Err(db::DbError::Ended);
        }
        Ok(true)
    }

    /// Run `f` in a transaction on the lent store, its pauses taking
    /// turns. It commits when `f` ends; when `f` fails it is dropped,
    /// unless parts were committed at pauses: then the rest is
    /// committed too (what a failure undid with them included), as a
    /// change ended part way keeps what it did.
    pub fn txn<T>(&self, f: impl FnOnce(&mut Txn<'_>, db::Pause<'_>) -> Result<T, String>) -> Result<T, String> {
        let mut txn = Txn::lent(Box::new(|| self.lender.store())).map_err(|e| e.to_string())?;
        let before = self.turns.borrow().yields;
        let r = f(&mut txn, &mut |t: &mut Txn<'_>| self.pause(t));
        let parted = self.turns.borrow().yields > before;
        match r {
            Ok(v) => {
                txn.commit().map_err(|e| e.to_string())?;
                Ok(v)
            }
            Err(e) => {
                if parted {
                    txn.commit().map_err(|c| format!("{e}; {c}"))?;
                } else {
                    txn.abort();
                }
                Err(e)
            }
        }
    }
}

/// A context of the job's own for the work: its library list and
/// naming as the console's has them, its overrides, its job's handle
/// (so its locks are its job's), the service's record lock table.
pub fn context(system: &mut System, job: JobHandle) -> Result<SqlContext, String> {
    let locks = system.locks.clone();
    let (_, console) = crate::svc::db::db(system)?;
    let mut ctx = SqlContext::with_locks(console.qsys, job, console.names.clone(), locks);
    ctx.naming = console.naming;
    ctx.current_schema = console.current_schema;
    ctx.session.set_overrides(console.session.overrides().to_vec());
    Ok(ctx)
}

/// Prepare command `name` to take turns: its work, or the reply when it
/// ended at once. One that cannot take turns now (the job's console is
/// capturing lines) runs to its end here, as any command.
pub fn prepare(system: &mut System, name: &str, parms: &Parms, typed: &str, job: JobHandle) -> Result<Long, Box<CmdReply>> {
    if !crate::svc::db::idle(system) {
        return Err(Box::new(super::run(system, name, parms, typed)));
    }
    match name {
        "RUNSQL" | "RUNSQLSTM" => super::dbcmd::runsql_long(system, name, parms, job),
        _ => bridged(system, name, parms, typed, job),
    }
}

/// A command the bridge hands to the database's console word, its work
/// that word's on a lent transaction.
fn bridged(system: &mut System, name: &str, parms: &Parms, typed: &str, job: JobHandle) -> Result<Long, Box<CmdReply>> {
    let Some(e) = sys_cpps::entry(name).filter(|e| matches!(e.site, Site::Service)) else {
        return Err(Box::new(super::run(system, name, parms, typed)));
    };
    let line = if typed.is_empty() {
        sys_cpps::bridge::specified_line(&e.word, parms)
    } else {
        typed.to_string()
    };
    let word = line.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
    let p = Params::parse(&line);
    let ctx = context(system, job).map_err(|m| Box::new(super::check(name, &m)))?;
    let owned = parms.clone();
    let reply = move |r: Result<String, String>| -> Finish { Box::new(move |_: &mut System| bridged_reply(e, &owned, r)) };
    let parms_at_end = parms.clone();
    Ok(match word.as_str() {
        "cpyf" => Box::new(move |w: &Work<'_>| reply(w.txn(|t, pause| files::cpyf_in(t, &ctx, job, &p, pause)))),
        "rgzpfm" => Box::new(move |w: &Work<'_>| reply(w.txn(|t, pause| files::rgzpfm_in(t, &ctx, job, &p, pause)))),
        "crtlf" => Box::new(move |w: &Work<'_>| reply(w.txn(|t, pause| files::crtlf_in(t, &ctx, job, &p, pause)))),
        "cpyfrmimpf" => {
            let text = match files::cpyfrmimpf_text(system, &p) {
                Ok(t) => t,
                Err(m) => return Err(Box::new(bridged_reply(e, parms, Err(m)))),
            };
            Box::new(move |w: &Work<'_>| reply(w.txn(|t, pause| files::cpyfrmimpf_in(t, &ctx, job, &p, &text, pause))))
        }
        "cpytoimpf" => Box::new(move |w: &Work<'_>| {
            let text = w.txn(|t, pause| files::cpytoimpf_in(t, &ctx, job, &p, pause));
            // The text goes where it goes with the job's state in place.
            Box::new(move |system: &mut System| {
                let r = text.and_then(|t| files::cpytoimpf_out(system, &p, t));
                bridged_reply(e, &parms_at_end, r)
            })
        }),
        _ => return Err(Box::new(super::run(system, name, parms, typed))),
    })
}

/// The reply the bridge gives a console word's output.
fn bridged_reply(e: &'static sys_cpps::Entry, parms: &Parms, r: Result<String, String>) -> CmdReply {
    let text = match r {
        Ok(s) if s.ends_with('\n') || s.is_empty() => s,
        Ok(s) => format!("{s}\n"),
        Err(m) => format!("{m}\n"),
    };
    if text.trim_start().starts_with("CPF9899") {
        return super::check(&e.name, &text);
    }
    super::classify(text, e, parms)
}
