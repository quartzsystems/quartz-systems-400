// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Firing: the calls one record change or read makes, in IBM's order
//! (before triggers in the order added, the change, after triggers), and
//! the ways an open data path makes them ([`Mode`]).

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use qsname::Name;
use store::txn::Txn;

use super::buffer::{self, Header, Image};
use super::model::{Event, Time, Trigger, UpdateCondition};
use super::Active;
use crate::error::{DbError, Result};
use crate::member::Member;

/// One call of a trigger program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The trigger.
    pub trigger: String,
    /// Its program: library, name.
    pub program: (Name, Name),
    /// When.
    pub time: Time,
    /// On what.
    pub event: Event,
    /// `ALWREPCHG(*YES)`: a before trigger's change of the new record counts.
    pub alwrepchg: bool,
    /// The file.
    pub file: Name,
    /// Its library.
    pub library: Name,
    /// The trigger buffer; the program may change it.
    pub buffer: Vec<u8>,
    /// An SQL `AFTER` trigger of `MODE DB2SQL`: called when the statement
    /// has changed every row, not at once.
    pub deferred: bool,
}

/// Why a call failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Failure {
    /// The escape message the program ended with, or `CPF4160` when it
    /// could not be found.
    pub msgid: String,
    /// Its first-level text.
    pub text: String,
    /// The program was not found (SQL's `SQL0204`).
    pub not_found: bool,
}

/// Calls trigger programs in the job that changes the record.
pub trait Caller {
    /// Call the program with `call`'s buffer. `txn` is the change's
    /// transaction: the caller may commit what it holds and give the store
    /// to others while the program runs (the program's own changes are
    /// other requests); what the database read before is read again.
    ///
    /// # Errors
    ///
    /// The program ended in an exception or could not be called.
    fn call(&mut self, txn: &mut Txn<'_>, call: &mut Call) -> std::result::Result<(), Failure>;
}

/// A shared caller.
pub type CallerRef = Rc<RefCell<dyn Caller>>;

/// How an open data path makes its trigger calls.
#[derive(Clone, Default)]
pub enum Mode {
    /// It cannot: a change (or read) a trigger would fire for is refused
    /// before anything changes.
    #[default]
    Refuse,
    /// It returns them: before calls as [`DbError::TriggersDue`] with
    /// nothing changed, the operation resumed with [`Resume`]; after
    /// calls kept for the caller to take ([`crate::Odp::take_due`]) once
    /// the change is made.
    Defer,
    /// It makes them through the caller.
    Call(CallerRef),
}

impl fmt::Debug for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Refuse => "Refuse",
            Self::Defer => "Defer",
            Self::Call(_) => "Call",
        })
    }
}

/// The job's answer to before calls returned by [`Mode::Defer`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resume {
    /// The last buffer an `ALWREPCHG(*YES)` trigger left, whose new
    /// record the change writes; `None` when none changed it.
    pub buffer: Option<Vec<u8>>,
    /// The call that failed, with its trigger's name.
    pub failed: Option<(String, Failure)>,
}

/// One record change or read.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    /// What happens.
    pub event: Event,
    /// The record's number, 0 for a new record before it is written.
    pub rrn: u64,
    /// The record as it was.
    pub old: Option<Image<'a>>,
    /// The record as it will be.
    pub new: Option<Image<'a>>,
}

/// Whether `t` fires for `row`: an update trigger with `*CHANGE` only when
/// the record changes.
fn applies(t: &Trigger, row: &Row<'_>) -> bool {
    if t.event != row.event {
        return false;
    }
    if t.update_condition == UpdateCondition::Change
        && let (Some(o), Some(n)) = (row.old, row.new)
    {
        return o != n;
    }
    true
}

/// The calls `time`'s triggers of `active` make for `row` of `member`.
#[must_use]
pub fn calls(active: &Active, member: &Member, time: Time, row: &Row<'_>, commit_level: u8) -> Vec<Call> {
    let fmt = member.format();
    let h = Header {
        file: member.file.name(),
        library: member.file.library_name,
        member: member.name(),
        event: row.event,
        time,
        commit_level,
        ccsid: 37,
        rrn: u32::try_from(row.rrn).unwrap_or(u32::MAX),
    };
    active
        .matching(time, row.event)
        .filter(|t| applies(t, row))
        .map(|t| Call {
            trigger: t.name.clone(),
            program: t.program,
            time,
            event: row.event,
            alwrepchg: t.alwrepchg,
            file: h.file,
            library: h.library,
            buffer: buffer::build(&h, fmt, row.old, row.new),
            deferred: t.sql && t.db2sql && time == Time::After && !t.statement,
        })
        .collect()
}

/// The error a failed call makes the operation end with.
#[must_use]
pub fn failed(call_file: Name, trigger: &str, before: bool, f: Failure) -> DbError {
    DbError::TriggerFailed { file: call_file.to_string(), trigger: trigger.to_string(), before, failure: f }
}

/// Make `time`'s calls for `row` through `caller`, one at a time: a
/// before trigger with `ALWREPCHG(*YES)` passes its new record to the next
/// and to the change. The new image left, when one changed it.
pub fn call_all(
    txn: &mut Txn<'_>,
    caller: &CallerRef,
    active: &Active,
    member: &Member,
    time: Time,
    row: &Row<'_>,
    commit_level: u8,
) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
    let fmt = member.format();
    let nulls_len = usize::from(fmt.null_map_len);
    let mut changed: Option<(Vec<u8>, Vec<u8>)> = None;
    for mut c in calls(active, member, time, row, commit_level) {
        if let Some((r, n)) = &changed {
            buffer::replace_new(fmt, &mut c.buffer, (r, n));
        }
        caller
            .borrow_mut()
            .call(txn, &mut c)
            .map_err(|f| failed(c.file, &c.trigger, time != Time::After, f))?;
        if c.alwrepchg
            && time == Time::Before
            && let Some(img) = buffer::new_image(fmt, &c.buffer, nulls_len)
        {
            changed = Some(img);
        }
    }
    Ok(changed)
}
