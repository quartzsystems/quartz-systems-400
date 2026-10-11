// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A job's view of the database: its overrides and the open data paths
//! it shares. `MIGRATE(phase5)`: one per job, with call-level scoping
//! (`OVRSCOPE`) once there are call levels.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use dds::FileRef;
use qsname::{LibSpec, Name};
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use super::ovrdbf::{Override, Position};
use super::{KeyArg, Odp, OpenOpts, SharedLocks, Usage, status};
use crate::error::{DbError, Result};
use crate::file::{self, Resolve};
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
    shared: HashMap<(ObjectId, Name), Shared>,
}

impl Session {
    /// A session for `job`.
    #[must_use]
    pub fn new(job: JobHandle, locks: SharedLocks) -> Self {
        Self {
            job,
            locks,
            overrides: Vec::new(),
            shared: HashMap::new(),
        }
    }

    /// The job.
    #[must_use]
    pub fn job(&self) -> JobHandle {
        self.job
    }

    /// `OVRDBF`: a second override of the same file replaces the first.
    pub fn ovrdbf(&mut self, ovr: Override) {
        self.overrides.retain(|o| o.file != ovr.file);
        self.overrides.push(ovr);
    }

    /// `DLTOVR FILE(name)`, or `FILE(*ALL)` with `None`; how many went.
    pub fn dltovr(&mut self, file: Option<Name>) -> usize {
        let before = self.overrides.len();
        match file {
            Some(f) => self.overrides.retain(|o| o.file != f),
            None => self.overrides.clear(),
        }
        before - self.overrides.len()
    }

    /// The overrides in effect, in the order they were issued.
    #[must_use]
    pub fn overrides(&self) -> &[Override] {
        &self.overrides
    }

    /// The override for a file name, if any.
    #[must_use]
    pub fn override_for(&self, file: Name) -> Option<&Override> {
        self.overrides.iter().find(|o| o.file == file)
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
        let ovr = self.override_for(file.file).cloned();
        let target = ovr
            .as_ref()
            .and_then(|o| o.tofile.clone())
            .unwrap_or_else(|| file.clone());
        let id = resolve(&target)?;
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
        let odp = Rc::new(RefCell::new(odp));
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

    /// Release everything the job holds: shared paths, overrides, record
    /// and object locks.
    pub fn end(&mut self, txn: &mut Txn<'_>) {
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
