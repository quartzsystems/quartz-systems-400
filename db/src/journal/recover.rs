// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database recovery after an abnormal end: commit cycles left open in
//! a journal are rolled back. The IPL reports the count as `C1001082`.

use jrn::{Filter, Journal, code, dspjrn};
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::undo::{resolve_members, undo};
use super::{JrnCtx, cycle_entry};
use crate::error::Result;

/// What recovery did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// Journals looked at.
    pub journals: u32,
    /// Cycles rolled back.
    pub cycles_rolled_back: u32,
    /// Record changes undone.
    pub records_undone: u64,
}

/// Roll back every cycle of `journal` that began without ending.
pub fn recover_journal(txn: &mut Txn<'_>, journal: ObjectId, now: u64) -> Result<RecoveryReport> {
    let j = Journal::load(txn, journal)?;
    let entries = dspjrn(txn, journal, &Filter::default())?;
    let mut open: Vec<u64> = Vec::new();
    for e in &entries {
        if e.is(code::COMMIT, "BC") {
            if !open.contains(&e.commit_cycle) {
                open.push(e.commit_cycle);
            }
        } else if e.is(code::COMMIT, "CM") || e.is(code::COMMIT, "RB") {
            open.retain(|c| *c != e.commit_cycle);
        }
    }
    let mut report = RecoveryReport {
        journals: 1,
        ..RecoveryReport::default()
    };
    if open.is_empty() {
        return Ok(report);
    }
    let map = resolve_members(txn, &j, None)?;
    for cycle in open {
        let mine: Vec<_> = entries
            .iter()
            .filter(|e| e.commit_cycle == cycle && e.code == code::RECORD)
            .cloned()
            .collect();
        let ctx = JrnCtx {
            job: JobHandle(0),
            commit_cycle: cycle,
            now: Some(now),
            rollback: true,
            applied: false,
        };
        let r = undo(txn, &mine, &map, ctx)?;
        cycle_entry(txn, journal, "RB", cycle, ctx)?;
        report.cycles_rolled_back += 1;
        report.records_undone += r.applied;
    }
    Ok(report)
}

/// Recover every journal in every library.
pub fn recover_all(txn: &mut Txn<'_>, qsys: ObjectId, now: u64) -> Result<RecoveryReport> {
    let mut libraries: Vec<ObjectId> = ops::list(txn, qsys, Some(types::LIB))?
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    if !libraries.contains(&qsys) {
        libraries.insert(0, qsys);
    }
    let mut total = RecoveryReport::default();
    for lib in libraries {
        for (_, id) in ops::list(txn, lib, Some(jrn::JRN))? {
            let r = recover_journal(txn, id, now)?;
            total.journals += r.journals;
            total.cycles_rolled_back += r.cycles_rolled_back;
            total.records_undone += r.records_undone;
        }
    }
    Ok(total)
}
