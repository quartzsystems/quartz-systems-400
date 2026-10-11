// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The open data path: a file opened by a job with the record-level
//! operations RPG performs on it, their positioning rules, record locks
//! and the feedback the INFDS will show. Every operation runs in the
//! transaction the caller passes; the store makes it atomic.

pub mod feedback;
pub mod keyarg;
pub mod locks;
pub mod ovrdbf;
pub mod session;

pub use feedback::{IoFeedback, status};
pub use keyarg::KeyArg;
pub use locks::{RecordLocks, SharedLocks, new_shared};
pub use ovrdbf::{Override, Position};
pub use session::{OdpRef, Session};

use std::ops::Bound;

use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use crate::cmt::{CommitRef, LckLvl};
use crate::error::{DbError, Result};
use crate::file::{self, FileKind, Wait};
use crate::member::{Core, Member};
use crate::path::{self, PathKey};
use crate::record::Value;

/// How the file is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Usage {
    /// Reads.
    pub input: bool,
    /// Writes.
    pub output: bool,
    /// Updates.
    pub update: bool,
    /// Deletes.
    pub delete: bool,
}

impl Usage {
    /// Input only.
    pub const INPUT: Self = Self {
        input: true,
        output: false,
        update: false,
        delete: false,
    };
    /// Output only.
    pub const OUTPUT: Self = Self {
        input: false,
        output: true,
        update: false,
        delete: false,
    };
    /// Input, update and delete.
    pub const UPDATE: Self = Self {
        input: true,
        output: false,
        update: true,
        delete: true,
    };
    /// Everything.
    pub const ALL: Self = Self {
        input: true,
        output: true,
        update: true,
        delete: true,
    };

    fn changes(self) -> bool {
        self.output || self.update || self.delete
    }
}

/// What an open takes.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct OpenOpts {
    /// `MBR`.
    pub member: MemberSpec,
    /// Usage.
    pub usage: Usage,
    /// Keyed (by access path) or arrival sequence.
    pub keyed: bool,
    /// `COMMIT(*YES)`: kept for commitment control.
    pub commit: bool,
    /// Level identifiers the program was compiled with, per format.
    pub lvlchk: Option<Vec<(Name, String)>>,
    /// The one format of a multi-format file to read.
    pub record_format: Option<Name>,
    /// The job.
    pub job: JobHandle,
    /// `SHARE(*YES)`: the session shares this open data path.
    pub share: bool,
    /// `SEQONLY(*YES n)`: records per block; kept, nothing is blocked yet.
    pub seqonly: Option<u32>,
    /// `INHWRT(*YES)`: writes, updates and deletes succeed without changing anything.
    pub inhwrt: bool,
    /// `WAITRCD` in effect; the lock is not waited for yet (`MIGRATE(phase5)`).
    pub wait_rcd: Option<Wait>,
}

impl OpenOpts {
    /// Keyed input of the first member.
    #[must_use]
    pub fn input(job: JobHandle) -> Self {
        Self {
            member: MemberSpec::First,
            usage: Usage::INPUT,
            keyed: true,
            commit: false,
            lvlchk: None,
            record_format: None,
            job,
            share: false,
            seqonly: None,
            inhwrt: false,
            wait_rcd: None,
        }
    }
}

/// A record read or to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Record format name.
    pub format: Name,
    /// Format index in the file.
    pub format_index: u16,
    /// Relative record number (of the based-on member for a logical file).
    pub rrn: u64,
    /// The record bytes.
    pub data: Vec<u8>,
    /// The null map.
    pub null_map: Vec<u8>,
}

/// Where the open data path stands.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pos {
    /// Before the first record.
    Start,
    /// After the last record.
    End,
    /// Before the first entry whose key is at or above the prefix.
    Before(Vec<u8>),
    /// After the last entry whose key starts with the prefix.
    After(Vec<u8>),
    /// At this entry (keyed) or this RRN (arrival).
    At(PathKey),
}

/// An open data path.
#[derive(Debug)]
pub struct Odp {
    /// The file.
    pub file: ObjectId,
    member: Member,
    opts: OpenOpts,
    pos: Pos,
    /// The record read for update, with the member it is locked in.
    locked: Option<(ObjectId, PathKey)>,
    /// Key of the last record read, for `READE` without a key.
    last_key: Option<Vec<u8>>,
    /// The feedback.
    pub fb: IoFeedback,
    locks: SharedLocks,
    file_lock: LockState,
    open: bool,
    cmt: Option<CommitRef>,
}

/// Allocate an object for a job, as `ALCOBJ` would; `file` names the
/// error.
pub fn allocate(
    txn: &mut Txn<'_>,
    job: JobHandle,
    id: ObjectId,
    state: LockState,
    file: &str,
) -> Result<()> {
    txn.store()
        .locks_mut()
        .lock(job, id, state)
        .map_err(|held| DbError::FileLocked {
            file: file.to_string(),
            holder: held.job.0,
        })
}

/// Release an allocation taken with [`allocate`].
pub fn deallocate(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId, state: LockState) {
    txn.store().locks_mut().unlock(job, id, state);
}

impl Odp {
    /// Open a file: allocate it and the member, check levels, position
    /// at the start.
    pub fn open(
        txn: &mut Txn<'_>,
        locks: SharedLocks,
        file: ObjectId,
        opts: OpenOpts,
    ) -> Result<Self> {
        let desc = file::describe(txn, file)?;
        if let Some(want) = &opts.lvlchk
            && desc.attr.lvlchk
        {
            for (name, id) in want {
                let actual = desc
                    .formats
                    .iter()
                    .find(|f| f.format.name == *name)
                    .map(|f| f.level_id.clone())
                    .unwrap_or_default();
                if actual != *id {
                    return Err(DbError::LevelCheck {
                        file: desc.name().to_string(),
                        format: name.to_string(),
                        expected: id.clone(),
                        actual,
                    });
                }
            }
        }
        if let Some(rf) = opts.record_format
            && !desc.formats.iter().any(|f| f.format.name == rf)
        {
            return Err(DbError::Definition {
                what: format!("file {} has no record format {rf}", desc.name()),
            });
        }
        let entry = *desc
            .member(opts.member)
            .ok_or_else(|| DbError::MemberNotFound {
                file: desc.name().to_string(),
                member: opts.member.to_string(),
            })?;
        let file_lock = if opts.usage.changes() {
            LockState::SharedUpdate
        } else {
            LockState::SharedRead
        };
        let fname = desc.name().to_string();
        allocate(txn, opts.job, file, file_lock, &fname)?;
        let opened = Member::open(txn, &desc, &entry).and_then(|mut m| {
            allocate(txn, opts.job, m.id(), file_lock, &fname)?;
            if opts.keyed
                && m.attr().key_len > 0
                && let Err(e) = m.ensure_path(txn, &mut |_, _| {})
            {
                txn.store().locks_mut().unlock(opts.job, m.id(), file_lock);
                return Err(e);
            }
            Ok(m)
        });
        let mut member = match opened {
            Ok(m) => m,
            Err(e) => {
                txn.store().locks_mut().unlock(opts.job, file, file_lock);
                return Err(e);
            }
        };
        member.jrn.job = opts.job;
        if let Err(e) = crate::journal::member_event(txn, &member, "OP") {
            let locks = txn.store().locks_mut();
            locks.unlock(opts.job, member.id(), file_lock);
            locks.unlock(opts.job, file, file_lock);
            return Err(e);
        }
        let fb = IoFeedback::new(member.format().format.name, member.name());
        Ok(Self {
            file,
            member,
            opts,
            pos: Pos::Start,
            locked: None,
            last_key: None,
            fb,
            locks,
            file_lock,
            open: true,
            cmt: None,
        })
    }

    /// Close: release the record lock, the member and the file.
    pub fn close(mut self, txn: &mut Txn<'_>) {
        self.release(txn);
    }

    /// Release every lock the path holds; it is closed afterwards.
    pub(crate) fn release(&mut self, txn: &mut Txn<'_>) {
        if !self.open {
            return;
        }
        self.unlock();
        if let Err(e) = crate::journal::member_event(txn, &self.member, "CL") {
            tracing::warn!("journal close entry for {}: {e}", self.member.name());
        }
        let locks = txn.store().locks_mut();
        locks.unlock(self.opts.job, self.member.id(), self.file_lock);
        locks.unlock(self.opts.job, self.file, self.file_lock);
        self.open = false;
    }

    /// Whether the path is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The member.
    #[must_use]
    pub fn member(&self) -> &Member {
        &self.member
    }

    /// Put the path under the job's commit definition (`COMMIT(*YES)`).
    pub fn set_commit(&mut self, cmt: CommitRef) {
        self.cmt = Some(cmt);
    }

    /// Whether the path is under commitment control.
    #[must_use]
    pub fn has_commit(&self) -> bool {
        self.cmt.is_some()
    }

    /// The lock level in force, when under commitment control.
    fn lcklvl(&self) -> Option<LckLvl> {
        self.cmt.as_ref().map(|c| c.borrow().lcklvl())
    }

    /// Before a change: under commitment control the cycle is opened and
    /// the member journals under it.
    fn begin_change(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        let Some(c) = &self.cmt else {
            return Ok(());
        };
        let r = c.borrow_mut().begin_change(txn, &self.member);
        match r {
            Ok(cycle) => {
                self.member.jrn.commit_cycle = cycle;
                Ok(())
            }
            Err(e) => {
                self.fb.fail(&e);
                Err(e)
            }
        }
    }

    /// After a change: the record lock goes to the commit definition, or
    /// is released.
    fn keep_or_unlock(&mut self) {
        let Some((member, k)) = self.locked.take() else {
            return;
        };
        match &self.cmt {
            Some(c) => c.borrow_mut().hold(member, k.rrn),
            None => {
                self.locks
                    .lock()
                    .expect("lock table")
                    .unlock(member, k.rrn, self.opts.job);
            }
        }
    }

    /// A record just written stays locked until the cycle ends.
    fn hold_new(&mut self, rrn: u64) {
        let Some(c) = &self.cmt else {
            return;
        };
        let member = self.member.id();
        let _ = self
            .locks
            .lock()
            .expect("lock table")
            .lock(member, rrn, self.opts.job);
        c.borrow_mut().hold(member, rrn);
    }

    /// After a commit or rollback: the member is reread, no record is
    /// held, the position stays.
    pub fn after_commit(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        if let Some((member, k)) = self.locked.take() {
            self.locks
                .lock()
                .expect("lock table")
                .unlock(member, k.rrn, self.opts.job);
        }
        self.member.jrn.commit_cycle = 0;
        self.member.refresh(txn)
    }

    /// The options.
    #[must_use]
    pub fn opts(&self) -> &OpenOpts {
        &self.opts
    }

    /// Whether reads go by key.
    fn keyed(&self) -> bool {
        self.opts.keyed && self.member.attr().key_len > 0
    }

    /// The format a key argument is encoded for.
    fn key_format(&self) -> usize {
        self.opts
            .record_format
            .and_then(|rf| {
                self.member
                    .file
                    .formats
                    .iter()
                    .position(|f| f.format.name == rf)
            })
            .unwrap_or(0)
    }

    fn format_index_of(&self) -> u16 {
        if self.member.is_multi_format() {
            u16::try_from(self.key_format()).expect("few")
        } else {
            self.member.attr().format
        }
    }

    /// The physical member a record of `format` lives in.
    fn lock_member(&self, format: u16) -> ObjectId {
        if self.member.is_logical() {
            self.member.bases[usize::from(format)].member
        } else {
            self.member.id()
        }
    }

    fn begin(&mut self, txn: &mut Txn<'_>, opcode: &'static str) -> Result<()> {
        if !self.open {
            self.fb.begin(opcode);
            let e = DbError::Closed;
            self.fb.fail(&e);
            return Err(e);
        }
        self.fb.begin(opcode);
        self.member.refresh(txn)?;
        Ok(())
    }

    fn need(&mut self, ok: bool, what: &str) -> Result<()> {
        if ok {
            return Ok(());
        }
        let e = DbError::Definition {
            what: format!("file not open for {what}"),
        };
        self.fb.fail(&e);
        Err(e)
    }

    /// Encode a search argument as a key prefix.
    fn prefix(&self, arg: &KeyArg) -> Result<Vec<u8>> {
        match arg {
            KeyArg::Values(values) => {
                let fmt = &self.member.file.formats[self.key_format()];
                if values.len() > fmt.format.keys.len() {
                    return Err(DbError::Definition {
                        what: format!(
                            "{} key fields given, format {} has {}",
                            values.len(),
                            fmt.format.name,
                            fmt.format.keys.len()
                        ),
                    });
                }
                path::values_key(fmt, values)
            }
            KeyArg::Bytes(b) => Ok(b.clone()),
            other => Err(DbError::Definition {
                what: format!("{other:?} is not a key"),
            }),
        }
    }

    /// The record a path entry names, as a `Record`.
    fn fetch(&self, txn: &mut Txn<'_>, k: &PathKey) -> Result<Option<Record>> {
        let Some(slot) = self.member.read_entry(txn, k)? else {
            return Ok(None);
        };
        let fi = if self.member.is_logical() || self.member.is_multi_format() {
            k.format
        } else {
            self.member.attr().format
        };
        Ok(Some(Record {
            format: self.member.format_at(usize::from(fi)).format.name,
            format_index: fi,
            rrn: k.rrn,
            data: slot.record,
            null_map: slot.null_map,
        }))
    }

    /// The next entry in the direction asked, honouring the record
    /// format restriction and skipping records `DYNSLT` omits.
    fn step(&mut self, txn: &mut Txn<'_>, forward: bool) -> Result<Option<(PathKey, Record)>> {
        if self.keyed() {
            return self.step_keyed(txn, forward);
        }
        self.step_arrival(txn, forward)
    }

    fn step_keyed(
        &mut self,
        txn: &mut Txn<'_>,
        forward: bool,
    ) -> Result<Option<(PathKey, Record)>> {
        let want = self.opts.record_format.map(|_| self.format_index_of());
        let p = self.member.path().ok_or_else(|| DbError::Corrupt {
            what: "keyed open without an access path".into(),
        })?;
        let mut pos = self.pos.clone();
        loop {
            let batch = if forward {
                match &pos {
                    Pos::Start => p.scan(txn, Bound::Unbounded, Bound::Unbounded, 8)?,
                    Pos::End => Vec::new(),
                    Pos::Before(k) => p.scan(txn, Bound::Included(k), Bound::Unbounded, 8)?,
                    Pos::After(k) => p.scan(txn, Bound::Excluded(k), Bound::Unbounded, 8)?,
                    Pos::At(k) => p.scan_from(txn, Bound::Excluded(k), 8)?,
                }
            } else {
                match &pos {
                    Pos::Start => Vec::new(),
                    Pos::End => p.scan_back(txn, Bound::Unbounded, 8)?,
                    Pos::Before(k) => p.scan_back(txn, Bound::Excluded(k), 8)?,
                    Pos::After(k) => p.scan_back(txn, Bound::Included(k), 8)?,
                    Pos::At(k) => p.scan_back_from(txn, Bound::Excluded(k), 8)?,
                }
            };
            if batch.is_empty() {
                return Ok(None);
            }
            for k in batch {
                pos = Pos::At(k.clone());
                if want.is_some_and(|w| w != k.format) {
                    continue;
                }
                if let Some(r) = self.fetch(txn, &k)? {
                    return Ok(Some((k, r)));
                }
            }
        }
    }

    /// Slots in the member read in arrival sequence (format 0 of a
    /// logical member).
    fn arrival_slots(&self, txn: &mut Txn<'_>) -> Result<u64> {
        if self.member.is_logical() {
            Ok(Core::load(txn, self.member.bases[0].member)?.attr.slots)
        } else {
            Ok(self.member.attr().slots)
        }
    }

    fn step_arrival(
        &mut self,
        txn: &mut Txn<'_>,
        forward: bool,
    ) -> Result<Option<(PathKey, Record)>> {
        let slots = self.arrival_slots(txn)?;
        let mut rrn = match (&self.pos, forward) {
            (Pos::Start, true) => 1,
            (Pos::Start, false) | (Pos::End, true) => return Ok(None),
            (Pos::End, false) => slots,
            (Pos::Before(_) | Pos::After(_), _) => {
                return Err(DbError::Definition {
                    what: "key positioning on a file opened in arrival sequence".into(),
                });
            }
            (Pos::At(k), true) => k.rrn + 1,
            (Pos::At(k), false) => k.rrn.saturating_sub(1),
        };
        while rrn >= 1 && rrn <= slots {
            let k = PathKey {
                key: Vec::new(),
                format: 0,
                rrn,
            };
            if let Some(r) = self.fetch(txn, &k)? {
                return Ok(Some((k, r)));
            }
            if forward {
                rrn += 1;
            } else {
                rrn -= 1;
            }
        }
        Ok(None)
    }

    /// Take the record lock when the open allows changes.
    fn take_lock(&mut self, k: &PathKey, lock: bool) -> Result<()> {
        self.unlock();
        let lock = lock || matches!(self.lcklvl(), Some(LckLvl::Cs | LckLvl::All));
        if !(lock && (self.opts.usage.update || self.opts.usage.delete)) {
            return Ok(());
        }
        let member = self.lock_member(k.format);
        let mut table = self.locks.lock().expect("lock table");
        if let Err(holder) = table.lock(member, k.rrn, self.opts.job) {
            return Err(DbError::RecordLocked {
                file: self.member.file.name().to_string(),
                member: self.member.name().to_string(),
                rrn: k.rrn,
                holder: holder.0,
            });
        }
        self.locked = Some((member, k.clone()));
        Ok(())
    }

    /// Release the record lock held, if any; under `LCKLVL(*ALL)` the
    /// commit definition keeps it instead.
    pub fn unlock(&mut self) {
        if self.lcklvl() == Some(LckLvl::All) {
            self.keep_or_unlock();
            return;
        }
        if let Some((member, k)) = self.locked.take() {
            self.locks
                .lock()
                .expect("lock table")
                .unlock(member, k.rrn, self.opts.job);
        }
    }

    fn deliver(
        &mut self,
        txn: &mut Txn<'_>,
        found: Option<(PathKey, Record)>,
        lock: bool,
    ) -> Result<Option<Record>> {
        let _ = txn;
        match found {
            None => {
                self.fb.eof = true;
                self.fb.status = status::EOF;
                Ok(None)
            }
            Some((k, r)) => {
                if let Err(e) = self.take_lock(&k, lock) {
                    self.fb.fail(&e);
                    return Err(e);
                }
                self.fb.rrn = k.rrn;
                self.fb.key.clone_from(&k.key);
                self.fb.format = r.format;
                self.fb.found = true;
                self.last_key = Some(k.key.clone());
                self.pos = Pos::At(k);
                Ok(Some(r))
            }
        }
    }

    /// `SETLL`: position before the first record whose key is at or
    /// above the argument. `%FOUND` when one exists, `%EQUAL` when its
    /// key begins with the argument.
    pub fn setll(&mut self, txn: &mut Txn<'_>, arg: &KeyArg) -> Result<()> {
        self.begin(txn, "SETLL")?;
        self.need(self.opts.usage.input, "input")?;
        match arg {
            KeyArg::Start | KeyArg::LoVal => {
                self.pos = Pos::Start;
                self.fb.found = self.peek(txn, true)?.is_some();
            }
            KeyArg::End | KeyArg::HiVal => {
                self.pos = Pos::End;
                self.fb.found = self.peek(txn, false)?.is_some();
            }
            KeyArg::Rrn(n) if self.keyed() => self.setll_rrn_keyed(txn, *n)?,
            KeyArg::Rrn(n) => {
                self.pos = Pos::At(PathKey {
                    key: Vec::new(),
                    format: 0,
                    rrn: n.saturating_sub(1),
                });
                let next = self.peek(txn, true)?;
                self.fb.found = next.is_some();
                self.fb.equal = next.is_some_and(|(k, _)| k.rrn == *n);
            }
            _ => {
                self.need(self.keyed(), "keyed access")?;
                let prefix = self.prefix(arg)?;
                self.pos = Pos::Before(prefix.clone());
                let next = self.peek(txn, true)?;
                self.fb.found = next.is_some();
                self.fb.equal = next.is_some_and(|(k, _)| k.key.starts_with(&prefix));
            }
        }
        Ok(())
    }

    /// `SETLL` by RRN on a keyed open: before the record's own entry, so
    /// the next read returns it and reads go on in key order.
    fn setll_rrn_keyed(&mut self, txn: &mut Txn<'_>, rrn: u64) -> Result<()> {
        if self.member.is_logical() {
            let e = DbError::Definition {
                what: "RRN positioning on a keyed logical file".into(),
            };
            self.fb.fail(&e);
            return Err(e);
        }
        let Some(slot) = self.member.read(txn, rrn)?.filter(|s| !s.deleted) else {
            self.fb.found = false;
            self.fb.equal = false;
            return Ok(());
        };
        let entry = PathKey {
            key: self.member.key_of(&slot.record, &slot.null_map)?,
            format: 0,
            rrn,
        };
        self.pos = Pos::At(entry);
        self.pos = match self.step(txn, false)? {
            Some((k, _)) => Pos::At(k),
            None => Pos::Start,
        };
        self.fb.found = true;
        self.fb.equal = true;
        Ok(())
    }

    /// `SETGT`: position after the last record whose key begins with the
    /// argument. `%FOUND` when a record follows.
    pub fn setgt(&mut self, txn: &mut Txn<'_>, arg: &KeyArg) -> Result<()> {
        self.begin(txn, "SETGT")?;
        self.need(self.opts.usage.input, "input")?;
        match arg {
            KeyArg::Start | KeyArg::LoVal => self.pos = Pos::Start,
            KeyArg::End | KeyArg::HiVal => self.pos = Pos::End,
            KeyArg::Rrn(n) => {
                self.need(!self.keyed(), "arrival sequence")?;
                self.pos = Pos::At(PathKey {
                    key: Vec::new(),
                    format: 0,
                    rrn: *n,
                });
            }
            _ => {
                self.need(self.keyed(), "keyed access")?;
                self.pos = Pos::After(self.prefix(arg)?);
            }
        }
        self.fb.found = self.peek(txn, true)?.is_some();
        Ok(())
    }

    /// The next record without moving.
    fn peek(&mut self, txn: &mut Txn<'_>, forward: bool) -> Result<Option<(PathKey, Record)>> {
        let saved = self.pos.clone();
        let found = self.step(txn, forward);
        self.pos = saved;
        found
    }

    /// `READ`: the next record. `lock` is false for the `(N)` extender.
    pub fn read(&mut self, txn: &mut Txn<'_>, lock: bool) -> Result<Option<Record>> {
        self.begin(txn, "READ")?;
        self.need(self.opts.usage.input, "input")?;
        self.unlock();
        let found = self.step(txn, true)?;
        if found.is_none() {
            self.pos = Pos::End;
        }
        self.deliver(txn, found, lock)
    }

    /// `READP`: the previous record.
    pub fn readp(&mut self, txn: &mut Txn<'_>, lock: bool) -> Result<Option<Record>> {
        self.begin(txn, "READP")?;
        self.need(self.opts.usage.input, "input")?;
        self.unlock();
        let found = self.step(txn, false)?;
        if found.is_none() {
            self.pos = Pos::Start;
        }
        self.deliver(txn, found, lock)
    }

    /// `READE`: the next record whose key begins with the argument, or
    /// with the last record's key when no argument is given.
    pub fn reade(
        &mut self,
        txn: &mut Txn<'_>,
        arg: Option<&KeyArg>,
        lock: bool,
    ) -> Result<Option<Record>> {
        self.begin(txn, "READE")?;
        self.read_equal(txn, arg, lock, true)
    }

    /// `READPE`: like `READE`, backward.
    pub fn readpe(
        &mut self,
        txn: &mut Txn<'_>,
        arg: Option<&KeyArg>,
        lock: bool,
    ) -> Result<Option<Record>> {
        self.begin(txn, "READPE")?;
        self.read_equal(txn, arg, lock, false)
    }

    fn read_equal(
        &mut self,
        txn: &mut Txn<'_>,
        arg: Option<&KeyArg>,
        lock: bool,
        forward: bool,
    ) -> Result<Option<Record>> {
        self.need(self.opts.usage.input, "input")?;
        self.need(self.keyed(), "keyed access")?;
        let prefix = match arg {
            Some(a) => self.prefix(a)?,
            None => self.last_key.clone().ok_or_else(|| DbError::Definition {
                what: "READE without a key needs a record read first".into(),
            })?,
        };
        self.unlock();
        let found = self.peek(txn, forward)?;
        let found = found.filter(|(k, _)| k.key.starts_with(&prefix));
        if found.is_none() {
            self.fb.eof = true;
            self.fb.status = status::EOF;
            return Ok(None);
        }
        self.deliver(txn, found, lock)
    }

    /// `CHAIN`: the record with the key or RRN. Not found leaves the
    /// file positioned where the record would be.
    pub fn chain(&mut self, txn: &mut Txn<'_>, arg: &KeyArg, lock: bool) -> Result<Option<Record>> {
        self.begin(txn, "CHAIN")?;
        self.need(self.opts.usage.input, "input")?;
        // Any read gives up the record held so far.
        self.unlock();
        let found = if let KeyArg::Rrn(n) = arg {
            let k = PathKey {
                key: Vec::new(),
                format: 0,
                rrn: *n,
            };
            let r = if self.member.is_logical() && self.keyed() {
                None
            } else {
                self.fetch(txn, &k)?
            };
            if let Some(r) = r {
                Some((k, r))
            } else {
                self.pos = Pos::At(PathKey {
                    key: Vec::new(),
                    format: 0,
                    rrn: n.saturating_sub(1),
                });
                None
            }
        } else {
            self.need(self.keyed(), "keyed access")?;
            let prefix = self.prefix(arg)?;
            self.pos = Pos::Before(prefix.clone());
            let next = self.step(txn, true)?;
            match next {
                Some((k, r)) if k.key.starts_with(&prefix) => Some((k, r)),
                _ => {
                    self.pos = Pos::Before(prefix);
                    None
                }
            }
        };
        if found.is_none() {
            self.fb.status = status::NOT_FOUND;
            self.fb.found = false;
            return Ok(None);
        }
        self.deliver(txn, found, lock)
    }

    /// `WRITE`: add a record. The position does not move.
    pub fn write(&mut self, txn: &mut Txn<'_>, data: &[u8], null_map: &[u8]) -> Result<u64> {
        self.begin(txn, "WRITE")?;
        self.need(self.opts.usage.output, "output")?;
        if self.opts.inhwrt {
            return Ok(0);
        }
        if self.member.is_logical() {
            let e = DbError::Definition {
                what: "writing through a logical file is not supported yet".into(),
            };
            self.fb.fail(&e);
            return Err(e);
        }
        self.begin_change(txn)?;
        match self.member.write_new(txn, data, null_map) {
            Ok(rrn) => {
                self.fb.rrn = rrn;
                self.hold_new(rrn);
                Ok(rrn)
            }
            Err(e) => {
                self.fb.fail(&e);
                Err(e)
            }
        }
    }

    /// The record read for update, or the error RPG reports as 01221.
    fn current(&mut self) -> Result<(ObjectId, PathKey)> {
        if let Some(l) = &self.locked {
            return Ok(l.clone());
        }
        let e = DbError::NoLockedRecord {
            file: self.member.file.name().to_string(),
        };
        self.fb.fail(&e);
        Err(e)
    }

    /// `UPDATE`: replace the record last read for update.
    pub fn update(&mut self, txn: &mut Txn<'_>, data: &[u8], null_map: &[u8]) -> Result<()> {
        self.begin(txn, "UPDATE")?;
        self.need(self.opts.usage.update, "update")?;
        let (_, k) = self.current()?;
        if self.opts.inhwrt {
            self.unlock();
            self.fb.rrn = k.rrn;
            return Ok(());
        }
        if self.member.is_logical() {
            let e = DbError::Definition {
                what: "updating through a logical file is not supported yet".into(),
            };
            self.fb.fail(&e);
            return Err(e);
        }
        self.begin_change(txn)?;
        let result = self.member.update(txn, k.rrn, data, null_map);
        self.keep_or_unlock();
        self.fb.rrn = k.rrn;
        result.inspect_err(|e| self.fb.fail(e))
    }

    /// `DELETE`: delete the record last read for update.
    pub fn delete(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.begin(txn, "DELETE")?;
        self.need(self.opts.usage.delete, "delete")?;
        let (_, k) = self.current()?;
        if self.opts.inhwrt {
            self.unlock();
            self.fb.rrn = k.rrn;
            return Ok(());
        }
        if self.member.is_logical() {
            let e = DbError::Definition {
                what: "deleting through a logical file is not supported yet".into(),
            };
            self.fb.fail(&e);
            return Err(e);
        }
        self.begin_change(txn)?;
        let result = self.member.delete(txn, k.rrn);
        self.keep_or_unlock();
        self.fb.rrn = k.rrn;
        result.inspect_err(|e| self.fb.fail(e))
    }

    /// `DELETE` with a key: `CHAIN` then delete.
    pub fn delete_key(&mut self, txn: &mut Txn<'_>, arg: &KeyArg) -> Result<bool> {
        if self.chain(txn, arg, true)?.is_none() {
            return Ok(false);
        }
        self.delete(txn)?;
        Ok(true)
    }

    /// `UNLOCK`: release the record read for update.
    pub fn unlock_op(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.begin(txn, "UNLOCK")?;
        self.unlock();
        Ok(())
    }

    /// `FEOD`: force end of data. Nothing is buffered, so nothing to do.
    pub fn feod(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.begin(txn, "FEOD")
    }

    /// The value of a field of a record of this file.
    pub fn field(&self, record: &Record, name: &str) -> Result<Value> {
        let fmt = self.member.format_at(usize::from(record.format_index));
        let n = Name::parse(name).map_err(|e| DbError::Definition {
            what: e.to_string(),
        })?;
        let (i, f) = fmt.format.field(&n).ok_or_else(|| DbError::Definition {
            what: format!("format {} has no field {name}", fmt.format.name),
        })?;
        crate::record::read_field(f, &record.data, &record.null_map, fmt.null_index(i))
    }
}

impl FileKind {
    /// Whether records are written here.
    #[must_use]
    pub fn is_physical(self) -> bool {
        matches!(self, Self::Physical | Self::Source)
    }
}
