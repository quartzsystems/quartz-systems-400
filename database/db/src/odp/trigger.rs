// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The open data path's trigger state: how it makes its calls, the before
//! calls it handed out and is waiting to be resumed for, and the after
//! calls a deferring caller still has to make (`trg::Mode`).

use store::txn::Txn;

use crate::cmt::LckLvl;
use crate::error::{DbError, Result};
use crate::member::Member;
use crate::trg::fire::{self, Resume, Row};
use crate::trg::{Call, Mode, Time, buffer};

/// What a path keeps about its triggers.
#[derive(Debug, Default)]
pub struct TrgState {
    /// How calls are made.
    pub mode: Mode,
    /// Deferred before calls were handed out for this event and record.
    armed: Option<(crate::trg::Event, u64)>,
    /// The job's answer to them, for the next operation.
    resume: Option<Resume>,
    /// After calls a deferring caller has to make.
    due: Vec<Call>,
    /// Reads fire no read triggers: the path reads only to change (an
    /// SQL `UPDATE` or `DELETE`, as the primary).
    pub quiet_reads: bool,
    /// The columns the SQL `UPDATE` through the path sets (`UPDATE OF`
    /// triggers fire only for those); `None` for record writes.
    pub set_columns: Option<Vec<qsname::Name>>,
}

/// The commit lock level digit of the trigger buffer.
pub fn level_digit(l: Option<LckLvl>) -> u8 {
    match l {
        None => 0,
        Some(LckLvl::Chg) => 1,
        Some(LckLvl::Cs) => 2,
        Some(LckLvl::All) => 3,
    }
}

impl TrgState {
    /// The job's answer to the before calls handed out last.
    pub fn resume(&mut self, r: Resume) {
        self.resume = Some(r);
    }

    /// The after calls still to make, taken.
    pub fn take_due(&mut self) -> Vec<Call> {
        std::mem::take(&mut self.due)
    }

    /// Before `row` of `m` changes: its before triggers. The new image to
    /// write instead, when a trigger changed it. Refused at once, nothing
    /// changed, when the path cannot call programs and any trigger fires
    /// for the event (after ones included).
    pub fn before(&mut self, txn: &mut Txn<'_>, m: &mut Member, row: &Row<'_>, lvl: u8) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        let row = &Row { set: self.set_columns.as_deref(), ..*row };
        let Some(active) = m.trgs.clone() else {
            return Ok(None);
        };
        if !active.any(row.event) {
            return Ok(None);
        }
        match self.mode.clone() {
            Mode::Refuse => Err(DbError::TriggerNotCallable { file: m.file.name().to_string() }),
            Mode::Call(c) => {
                let r = fire::call_all(txn, &c, &active, m, Time::Before, row, lvl);
                // The program's own requests may have changed the member.
                m.refresh(txn)?;
                r
            }
            Mode::Defer => {
                let armed = self.armed.take();
                if let Some(r) = self.resume.take()
                    && armed == Some((row.event, row.rrn))
                {
                    if let Some((t, f)) = r.failed {
                        return Err(fire::failed(m.file.name(), &t, true, f));
                    }
                    let nulls = usize::from(m.format().null_map_len);
                    return Ok(r.buffer.and_then(|b| buffer::new_image(m.format(), &b, nulls)));
                }
                let calls = fire::calls(&active, m, Time::Before, row, lvl);
                if calls.is_empty() {
                    return Ok(None);
                }
                self.armed = Some((row.event, row.rrn));
                Err(DbError::TriggersDue { calls })
            }
        }
    }

    /// After `row` of `m` changed (or was read): its after triggers.
    pub fn after(&mut self, txn: &mut Txn<'_>, m: &mut Member, row: &Row<'_>, lvl: u8) -> Result<()> {
        let row = &Row { set: self.set_columns.as_deref(), ..*row };
        let Some(active) = m.trgs.clone() else {
            return Ok(());
        };
        if active.matching(Time::After, row.event).next().is_none() {
            return Ok(());
        }
        match self.mode.clone() {
            Mode::Refuse => Err(DbError::TriggerNotCallable { file: m.file.name().to_string() }),
            Mode::Call(c) => {
                let r = fire::call_all(txn, &c, &active, m, Time::After, row, lvl).map(|_| ());
                m.refresh(txn)?;
                r
            }
            Mode::Defer => {
                self.due.extend(fire::calls(&active, m, Time::After, row, lvl));
                Ok(())
            }
        }
    }

    /// Whether the path may read `m`'s records: a path that cannot call
    /// programs may not read a file with read triggers.
    pub fn may_read(&self, m: &Member) -> Result<()> {
        if self.quiet_reads {
            return Ok(());
        }
        match (&self.mode, &m.trgs) {
            (Mode::Refuse, Some(a)) if a.any(crate::trg::Event::Read) => {
                Err(DbError::TriggerNotCallable { file: m.file.name().to_string() })
            }
            _ => Ok(()),
        }
    }
}
