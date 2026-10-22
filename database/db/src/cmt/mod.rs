// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commitment control: a commit definition per job, and one per
//! activation group under `CMTSCOPE(*ACTGRP)` (the session keeps them
//! apart). Changes under a definition are journaled with a commit cycle
//! identifier, record locks stay until the cycle ends, and a rollback
//! undoes the cycle from the journal's before images.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use jrn::{Filter, Journal, code, dspjrn};
use qsname::Name;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{FileDesc, MemberEntry};
use crate::journal::undo::{ChangeReport, MemberMap, undo};
use crate::journal::{JrnCtx, cycle_entry};
use crate::member::Member;
use crate::odp::SharedLocks;

/// `LCKLVL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LckLvl {
    /// `*CHG`: changed records stay locked until the cycle ends.
    #[default]
    Chg,
    /// `*CS`: every record read is locked until the next read; changed
    /// ones until the cycle ends.
    Cs,
    /// `*ALL`: every record read stays locked until the cycle ends.
    All,
}

impl LckLvl {
    /// Parse `*CHG`, `*CS`, `*ALL`.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_uppercase().as_str() {
            "*CHG" => Self::Chg,
            "*CS" => Self::Cs,
            "*ALL" => Self::All,
            _ => return None,
        })
    }

    /// The `*NAME`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Chg => "*CHG",
            Self::Cs => "*CS",
            Self::All => "*ALL",
        }
    }
}

/// A job's commit definition, shared with its open data paths.
pub type CommitRef = Rc<RefCell<CommitDef>>;

/// `CMTSCOPE`: whose the commit definition is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmtScope {
    /// `*JOB`: the job's one definition, reached from every activation
    /// group without one of its own.
    Job,
    /// `*ACTGRP`: the named activation group's own definition.
    ActGrp(String),
}

/// The commit definition.
pub struct CommitDef {
    job: JobHandle,
    lcklvl: LckLvl,
    locks: SharedLocks,
    dft_jrn: Option<ObjectId>,
    /// The open cycle, 0 when none.
    cycle: u64,
    /// Journals the cycle wrote `C BC` to.
    journals: Vec<ObjectId>,
    /// Members changed in the cycle, for the rollback.
    touched: HashMap<(Name, Name, Name), (FileDesc, MemberEntry)>,
    /// Record locks held until the cycle ends.
    held: Vec<(ObjectId, u64)>,
    /// Changes made in the cycle.
    changes: u64,
}

impl std::fmt::Debug for CommitDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitDef")
            .field("job", &self.job)
            .field("lcklvl", &self.lcklvl)
            .field("cycle", &self.cycle)
            .field("changes", &self.changes)
            .field("held", &self.held.len())
            .finish_non_exhaustive()
    }
}

impl CommitDef {
    /// `STRCMTCTL`.
    #[must_use]
    pub fn new(
        job: JobHandle,
        lcklvl: LckLvl,
        locks: SharedLocks,
        dft_jrn: Option<ObjectId>,
    ) -> Self {
        Self {
            job,
            lcklvl,
            locks,
            dft_jrn,
            cycle: 0,
            journals: Vec::new(),
            touched: HashMap::new(),
            held: Vec::new(),
            changes: 0,
        }
    }

    /// `STRCMTCTL` as a shared handle, with `C SC` in the default journal.
    pub fn start(
        txn: &mut Txn<'_>,
        job: JobHandle,
        lcklvl: LckLvl,
        locks: SharedLocks,
        dft_jrn: Option<ObjectId>,
        now: u64,
    ) -> Result<CommitRef> {
        let def = Self::new(job, lcklvl, locks, dft_jrn);
        if let Some(j) = dft_jrn {
            cycle_entry(txn, j, "SC", 0, def.ctx(Some(now)))?;
        }
        Ok(Rc::new(RefCell::new(def)))
    }

    /// The lock level.
    #[must_use]
    pub fn lcklvl(&self) -> LckLvl {
        self.lcklvl
    }

    /// The open cycle, 0 when none.
    #[must_use]
    pub fn cycle(&self) -> u64 {
        self.cycle
    }

    /// Changes made in the open cycle.
    #[must_use]
    pub fn changes(&self) -> u64 {
        self.changes
    }

    /// Record locks held for the cycle.
    #[must_use]
    pub fn held(&self) -> &[(ObjectId, u64)] {
        &self.held
    }

    fn ctx(&self, now: Option<u64>) -> JrnCtx {
        JrnCtx {
            job: self.job,
            commit_cycle: self.cycle,
            now,
            rollback: false,
            applied: false,
        }
    }

    /// A change to `m` is about to happen: open the cycle if none is,
    /// write `C BC` to the member's journal on its first change, and
    /// return the cycle identifier the change is journaled under.
    pub fn begin_change(&mut self, txn: &mut Txn<'_>, m: &Member) -> Result<u64> {
        let journal = m.file.attr.journal;
        if journal.is_none() {
            return Err(DbError::CommitNeedsJournal {
                file: m.file.name().to_string(),
            });
        }
        if self.cycle == 0 {
            self.cycle = Journal::load(txn, journal)?.attr.next_seq;
        }
        if !self.journals.contains(&journal) {
            cycle_entry(txn, journal, "BC", self.cycle, self.ctx(m.jrn.now))?;
            self.journals.push(journal);
        }
        let entry = m
            .file
            .members
            .iter()
            .find(|e| e.object == m.id())
            .copied()
            .ok_or_else(|| DbError::MemberNotFound {
                file: m.file.name().to_string(),
                member: m.name().to_string(),
            })?;
        self.touched
            .entry((m.file.library_name, m.file.name(), m.name()))
            .or_insert_with(|| (m.file.clone(), entry));
        self.changes += 1;
        Ok(self.cycle)
    }

    /// Keep a record lock until the cycle ends.
    pub fn hold(&mut self, member: ObjectId, rrn: u64) {
        if !self.held.contains(&(member, rrn)) {
            self.held.push((member, rrn));
        }
    }

    fn release_locks(&mut self) {
        let mut table = self.locks.lock().expect("lock table");
        for (member, rrn) in self.held.drain(..) {
            table.unlock(member, rrn, self.job);
        }
    }

    fn close_cycle(&mut self, txn: &mut Txn<'_>, kind: &str, now: Option<u64>) -> Result<u64> {
        let cycle = self.cycle;
        let ctx = self.ctx(now);
        for j in std::mem::take(&mut self.journals) {
            cycle_entry(txn, j, kind, cycle, ctx)?;
        }
        self.release_locks();
        self.touched.clear();
        self.cycle = 0;
        self.changes = 0;
        Ok(cycle)
    }

    /// `COMMIT`: `C CM` to every journal of the cycle, locks released.
    /// Returns the cycle committed, 0 when none was open.
    pub fn commit(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<u64> {
        if self.cycle == 0 {
            self.release_locks();
            return Ok(0);
        }
        self.close_cycle(txn, "CM", now)
    }

    /// `ROLLBCK`: undo the cycle from its journal entries newest first,
    /// then `C RB`.
    pub fn rollback(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<ChangeReport> {
        if self.cycle == 0 {
            self.release_locks();
            return Ok(ChangeReport::default());
        }
        let cycle = self.cycle;
        let mut entries = Vec::new();
        for j in &self.journals {
            let f = Filter {
                commit_cycle: Some(cycle),
                codes: vec![code::RECORD],
                ..Filter::default()
            };
            entries.extend(dspjrn(txn, *j, &f)?);
        }
        entries.sort_by_key(|e| (e.time, e.micros, e.seq));
        let map: MemberMap = self.touched.clone();
        let ctx = JrnCtx {
            rollback: true,
            ..self.ctx(now)
        };
        let report = undo(txn, &entries, &map, ctx)?;
        self.close_cycle(txn, "RB", now)?;
        Ok(report)
    }

    /// `ENDCMTCTL`: an open cycle is rolled back first.
    pub fn end(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<ChangeReport> {
        let r = self.rollback(txn, now)?;
        let _ = self.dft_jrn;
        Ok(r)
    }
}
