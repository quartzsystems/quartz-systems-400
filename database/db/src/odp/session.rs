// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's view of the database: its overrides and the open data paths
//! it shares. An override reaches the opens of the activation group
//! that issued it, or every open for `OVRSCOPE(*JOB)` and for one
//! issued at command level. Within its reach the overrides of the file
//! merge from the deepest call level outwards, a `SECURE(*YES)` one
//! ending the merge, and a call level's overrides go when it returns.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use dds::FileRef;
use qsname::{LibSpec, Name};
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use super::ovrdbf::{Override, Position};
use super::{KeyArg, Odp, OpenOpts, SharedLocks, Usage, status};
use crate::cmt::{CommitDef, CommitRef, LckLvl};
use crate::error::{DbError, Result};
use crate::file::{self, Resolve};
use crate::journal::ChangeReport;
use crate::record::Value;

/// An open data path handed out by a session; shared ones are the same
/// path under several handles.
pub type OdpRef = Rc<RefCell<Odp>>;

/// What the session holds for a shared path.
struct Shared {
    odp: OdpRef,
    file: Name,
    uses: usize,
    usage: Usage,
}

/// A job's overrides and shared open data paths.
pub struct Session {
    job: JobHandle,
    locks: SharedLocks,
    overrides: Vec<Override>,
    /// The activation group opening files now.
    opener: String,
    shared: HashMap<(ObjectId, Name), Shared>,
    cmt: Option<CommitRef>,
    odps: Vec<Weak<RefCell<Odp>>>,
}

impl Session {
    /// A session for `job`.
    #[must_use]
    pub fn new(job: JobHandle, locks: SharedLocks) -> Self {
        Self {
            job,
            locks,
            overrides: Vec::new(),
            opener: super::ovrdbf::DFTACTGRP.into(),
            shared: HashMap::new(),
            cmt: None,
            odps: Vec::new(),
        }
    }

    /// The job.
    #[must_use]
    pub fn job(&self) -> JobHandle {
        self.job
    }

    /// `OVRDBF`: a second override of the same file at the same scope
    /// and call level replaces the first.
    pub fn ovrdbf(&mut self, ovr: Override) {
        self.overrides
            .retain(|o| o.file != ovr.file || o.scope != ovr.scope || o.level != ovr.level);
        self.overrides.push(ovr);
    }

    /// `DLTOVR FILE(name) LVL(*JOB)`, or `FILE(*ALL)` with `None`: every
    /// call level's; how many went.
    pub fn dltovr(&mut self, file: Option<Name>) -> usize {
        let before = self.overrides.len();
        match file {
            Some(f) => self.overrides.retain(|o| o.file != f),
            None => self.overrides.clear(),
        }
        before - self.overrides.len()
    }

    /// `DLTOVR` at call level `level`: the overrides that level issued,
    /// of `file` or all of them; how many went.
    pub fn dltovr_at(&mut self, file: Option<Name>, level: u32) -> usize {
        let before = self.overrides.len();
        self.overrides
            .retain(|o| o.level != level || file.is_some_and(|f| o.file != f));
        before - self.overrides.len()
    }

    /// Call level `level` returned: its overrides and any deeper level's
    /// go; how many went.
    pub fn end_level(&mut self, level: u32) -> usize {
        let before = self.overrides.len();
        self.overrides.retain(|o| o.level < level);
        before - self.overrides.len()
    }

    /// The overrides in effect, in the order they were issued.
    #[must_use]
    pub fn overrides(&self) -> &[Override] {
        &self.overrides
    }

    /// The override in effect for a file name, if any: the deepest call
    /// level's, filled in from the levels below it down to a
    /// `SECURE(*YES)` one.
    #[must_use]
    pub fn override_for(&self, file: Name) -> Option<Override> {
        let mut mine: Vec<&Override> = self
            .overrides
            .iter()
            .rev()
            .filter(|o| o.file == file && super::ovrdbf::reaches(&o.scope, &self.opener))
            .collect();
        mine.sort_by_key(|o| std::cmp::Reverse(o.level));
        let mut it = mine.into_iter();
        let mut merged = it.next()?.clone();
        if !merged.secure {
            for o in it {
                merged.merge_under(o);
                if o.secure {
                    break;
                }
            }
        }
        Some(merged)
    }

    /// The activation group the next opens come from.
    pub fn set_opener(&mut self, actgrp: &str) {
        actgrp.clone_into(&mut self.opener);
    }

    /// Replace the overrides: the job's one table, as its command level
    /// last changed it.
    pub fn set_overrides(&mut self, overrides: Vec<Override>) {
        self.overrides = overrides;
    }

    /// An activation group went: its overrides go with it.
    pub fn reclaim(&mut self, actgrp: &str) {
        self.overrides.retain(|o| o.scope != actgrp);
    }

    /// Open the file a program names, through the overrides: `TOFILE`
    /// redirects, `MBR`, `SHARE`, `LVLCHK`, `INHWRT`, `WAITRCD` and
    /// `SEQONLY` change the open, `POSITION` moves the new path.
    pub fn open(
        &mut self,
        txn: &mut Txn<'_>,
        file: &FileRef,
        resolve: Resolve<'_>,
        mut opts: OpenOpts,
    ) -> Result<OdpRef> {
        let ovr = self.override_for(file.file);
        let target = ovr
            .as_ref()
            .and_then(|o| o.tofile.clone())
            .unwrap_or_else(|| file.clone());
        let id = resolve(&target)?;
        // A shared open is checked too: each opener needs the authority.
        crate::auth::check_open(txn, id, opts.usage)?;
        let desc = file::describe(txn, id)?;
        opts.job = self.job;
        opts.share |= desc.attr.share;
        if let Some(o) = &ovr {
            apply(o, &mut opts);
        }
        if opts.wait_rcd.is_none() {
            opts.wait_rcd = Some(desc.attr.wait_rcd);
        }
        let member_name =
            desc.member(opts.member)
                .map(|m| m.name)
                .ok_or_else(|| DbError::MemberNotFound {
                    file: desc.name().to_string(),
                    member: opts.member.to_string(),
                })?;
        if opts.share
            && let Some(s) = self.shared.get_mut(&(id, member_name))
        {
            if !covers(s.usage, opts.usage) {
                return Err(DbError::ShareMismatch {
                    file: desc.name().to_string(),
                    member: member_name.to_string(),
                });
            }
            s.uses += 1;
            return Ok(Rc::clone(&s.odp));
        }
        if opts.commit && self.cmt.is_none() {
            return Err(DbError::CommitOpen);
        }
        let share = opts.share;
        let usage = opts.usage;
        let position = ovr.and_then(|o| o.position);
        let mut odp = Odp::open(txn, Arc::clone(&self.locks), id, opts)?;
        if let Some(p) = position
            && let Err(e) = position_odp(&mut odp, txn, &p)
        {
            odp.close(txn);
            return Err(e);
        }
        if let Some(c) = &self.cmt
            && odp.opts().commit
        {
            odp.set_commit(Rc::clone(c));
        }
        let odp = Rc::new(RefCell::new(odp));
        self.odps.push(Rc::downgrade(&odp));
        if share {
            self.shared.insert(
                (id, member_name),
                Shared {
                    odp: Rc::clone(&odp),
                    file: desc.name(),
                    uses: 1,
                    usage,
                },
            );
        }
        Ok(odp)
    }

    /// Close a path. A shared one stays open until its last user closes.
    pub fn close(&mut self, txn: &mut Txn<'_>, odp: &OdpRef) {
        let key = self
            .shared
            .iter()
            .find(|(_, s)| Rc::ptr_eq(&s.odp, odp))
            .map(|(k, _)| *k);
        if let Some(k) = key {
            let s = self.shared.get_mut(&k).expect("found");
            s.uses -= 1;
            if s.uses > 0 {
                return;
            }
            self.shared.remove(&k);
        }
        odp.borrow_mut().release(txn);
    }

    /// `CLOF`: close the shared paths over `file`, whoever still uses
    /// them; how many closed.
    pub fn clof(&mut self, txn: &mut Txn<'_>, file: Name) -> usize {
        let keys: Vec<_> = self
            .shared
            .iter()
            .filter(|(_, s)| s.file == file)
            .map(|(k, _)| *k)
            .collect();
        for k in &keys {
            let s = self.shared.remove(k).expect("listed");
            s.odp.borrow_mut().release(txn);
        }
        keys.len()
    }

    /// Shared paths open now.
    #[must_use]
    pub fn shared_count(&self) -> usize {
        self.shared.len()
    }

    /// Users of the shared path over a file and member.
    #[must_use]
    pub fn shared_uses(&self, file: ObjectId, member: Name) -> usize {
        self.shared.get(&(file, member)).map_or(0, |s| s.uses)
    }

    /// `STRCMTCTL`.
    pub fn strcmtctl(
        &mut self,
        txn: &mut Txn<'_>,
        lcklvl: LckLvl,
        dft_jrn: Option<ObjectId>,
        now: u64,
    ) -> Result<()> {
        if self.cmt.is_some() {
            return Err(DbError::CommitActive);
        }
        self.cmt = Some(CommitDef::start(
            txn,
            self.job,
            lcklvl,
            Arc::clone(&self.locks),
            dft_jrn,
            now,
        )?);
        Ok(())
    }

    /// The commit definition, when `STRCMTCTL` ran.
    #[must_use]
    pub fn commit_def(&self) -> Option<&CommitRef> {
        self.cmt.as_ref()
    }

    fn refresh_odps(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.odps.retain(|w| w.strong_count() > 0);
        for w in &self.odps {
            if let Some(o) = w.upgrade() {
                o.borrow_mut().after_commit(txn)?;
            }
        }
        Ok(())
    }

    /// `COMMIT`: the cycle committed, 0 when none was open.
    pub fn commit(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<u64> {
        let c = Rc::clone(self.cmt.as_ref().ok_or(DbError::CommitNotActive)?);
        let cycle = c.borrow_mut().commit(txn, now)?;
        self.refresh_odps(txn)?;
        Ok(cycle)
    }

    /// `ROLLBCK`.
    pub fn rollbck(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<ChangeReport> {
        let c = Rc::clone(self.cmt.as_ref().ok_or(DbError::CommitNotActive)?);
        let r = c.borrow_mut().rollback(txn, now)?;
        self.refresh_odps(txn)?;
        Ok(r)
    }

    /// `ENDCMTCTL`: an open cycle is rolled back.
    pub fn endcmtctl(&mut self, txn: &mut Txn<'_>, now: Option<u64>) -> Result<ChangeReport> {
        if self.cmt.is_none() {
            return Err(DbError::CommitNotActive);
        }
        self.odps.retain(|w| w.strong_count() > 0);
        let open = self.odps.iter().filter_map(Weak::upgrade).any(|o| {
            let o = o.borrow();
            o.is_open() && o.has_commit()
        });
        if open {
            return Err(DbError::CommitFilesOpen);
        }
        let c = self.cmt.take().ok_or(DbError::CommitNotActive)?;
        let r = c.borrow_mut().end(txn, now)?;
        self.refresh_odps(txn)?;
        Ok(r)
    }

    /// Release everything the job holds: an open commit cycle (rolled
    /// back), shared paths, overrides, record and object locks.
    pub fn end(&mut self, txn: &mut Txn<'_>) {
        if let Some(c) = self.cmt.take()
            && let Err(e) = c.borrow_mut().end(txn, None)
        {
            tracing::warn!("rollback at the end of the job: {e}");
        }
        for s in self.shared.values() {
            s.odp.borrow_mut().release(txn);
        }
        self.shared.clear();
        self.overrides.clear();
        self.locks.lock().expect("lock table").release_job(self.job);
        txn.store().locks_mut().release_job(self.job);
    }
}

/// Fold an override into the open options.
fn apply(o: &Override, opts: &mut OpenOpts) {
    if let Some(m) = o.member {
        opts.member = m;
    }
    if o.lvlchk == Some(false) {
        opts.lvlchk = None;
    }
    if let Some(w) = o.wait_rcd {
        opts.wait_rcd = Some(w);
    }
    if let Some(n) = o.seqonly {
        opts.seqonly = Some(n);
    }
    if let Some(i) = o.inhwrt {
        opts.inhwrt = i;
    }
    if let Some(s) = o.share {
        opts.share = s;
    }
}

/// Whether a shared path opened for `have` serves an open asking `want`.
fn covers(have: Usage, want: Usage) -> bool {
    (!want.input || have.input)
        && (!want.output || have.output)
        && (!want.update || have.update)
        && (!want.delete || have.delete)
}

/// Apply a `POSITION` override to a freshly opened path, then clear
/// the feedback the positioning left.
fn position_odp(odp: &mut Odp, txn: &mut Txn<'_>, p: &Position) -> Result<()> {
    if !odp.opts().usage.input {
        return Ok(());
    }
    let keyed = odp.opts().keyed && odp.member().attr().key_len > 0;
    let by_key = |v: &Vec<Value>| KeyArg::Values(v.clone());
    match p {
        Position::Start => odp.setll(txn, &KeyArg::Start)?,
        Position::End => odp.setll(txn, &KeyArg::End)?,
        Position::Rrn(n) => odp.setll(txn, &KeyArg::Rrn(*n))?,
        Position::KeyBefore(_)
        | Position::KeyBeforeEqual(_)
        | Position::KeyAfter(_)
        | Position::KeyAfterEqual(_)
            if !keyed =>
        {
            return Err(DbError::Definition {
                what: "POSITION by key on a file opened in arrival sequence".into(),
            });
        }
        Position::KeyAfterEqual(v) => odp.setll(txn, &by_key(v))?,
        Position::KeyAfter(v) => odp.setgt(txn, &by_key(v))?,
        Position::KeyBefore(v) => {
            odp.setll(txn, &by_key(v))?;
            back_one(odp, txn)?;
        }
        Position::KeyBeforeEqual(v) => {
            odp.setll(txn, &by_key(v))?;
            if !odp.fb.equal {
                back_one(odp, txn)?;
            }
        }
    }
    odp.fb.eof = false;
    odp.fb.found = false;
    odp.fb.equal = false;
    odp.fb.status = status::OK;
    odp.fb.msgid = None;
    Ok(())
}

/// Move the position so the next read returns the record before the
/// one it would have returned. At the start it stays there.
fn back_one(odp: &mut Odp, txn: &mut Txn<'_>) -> Result<()> {
    if odp.readp(txn, false)?.is_some() {
        odp.readp(txn, false)?;
    }
    Ok(())
}

/// A resolver over a fixed set of files in one library, for tests and
/// the console.
pub fn resolve_in(
    lib: Name,
    files: Vec<(Name, ObjectId)>,
) -> impl FnMut(&FileRef) -> Result<ObjectId> {
    move |fr: &FileRef| {
        let ok_lib = match fr.library {
            LibSpec::Name(l) => l == lib,
            LibSpec::Libl | LibSpec::Curlib => true,
        };
        files
            .iter()
            .find(|(n, _)| ok_lib && *n == fr.file)
            .map(|(_, id)| *id)
            .ok_or_else(|| {
                DbError::Store(store::StoreError::ObjectNotFound {
                    name: fr.file.to_string(),
                    library: fr.library.to_string(),
                })
            })
    }
}
