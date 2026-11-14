// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mirrored set as one block device: every member holds every block.
//! Opened with an [`Intent`], it keeps a write-intent map, so that an
//! unclean stop copies only the regions that were being written.

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use blockdev::{AlignedBuf, BLOCK_SIZE, BlockDev, BlockError, check_range};
use tracing::{info, warn};

use crate::UnitState;
use crate::intent::{Cursor, copy_next};
use crate::mirror::{
    CheckedError, Fails, Half, Halves, MirrorError, RESUME_CHUNK, ResumeIo, ResumeProgress,
    ResumeState,
};
use crate::set::intent::{Intent, IntentStats, Tracker};
use crate::set::{ResumeJournal, SetError, SetEvent, no_member};
use crate::spare::{self, Spare};

/// One member, as the caller recorded it and as the set reports it.
pub struct MirrorMember {
    /// Its identity.
    pub uuid: [u8; 16],
    /// Its state. A member of a set starts `Active` when it is known to
    /// hold what the others hold, and `Resuming` when it is not.
    pub state: UnitState,
    /// Its device, when it was found.
    pub dev: Option<Box<dyn BlockDev>>,
}

/// A member, for displays and for the caller's record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    /// Its identity.
    pub uuid: [u8; 16],
    /// Its state.
    pub state: UnitState,
    /// Whether a device is attached.
    pub attached: bool,
    /// The device's size in blocks; 0 when missing.
    pub blocks: u64,
}

/// How far the copy between active members after an unclean stop has
/// come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResyncProgress {
    /// Chunks copied so far.
    pub chunks_copied: u64,
    /// Chunks of the regions that were dirty at open.
    pub chunks_dirty: u64,
    /// Chunks in the set.
    pub chunks: u64,
    /// Nothing is left to copy.
    pub complete: bool,
}

/// The copy between active members.
struct Resync {
    state: ResumeState,
    cursor: Cursor,
    since_save: u64,
    dirty: u64,
    copied: u64,
    complete: bool,
}

struct Inner {
    id: [u8; 16],
    blocks: u64,
    members: RwLock<Vec<MirrorMember>>,
    turn: AtomicU64,
    events: Mutex<Vec<SetEvent>>,
    intent: Option<Mutex<Tracker>>,
    resyncing: AtomicBool,
    resync: Mutex<Option<Resync>>,
}

/// A mirrored set. Clones are handles on the same set: one serves the
/// block device, another runs the operations.
#[derive(Clone)]
pub struct MirrorSet {
    inner: Arc<Inner>,
}

/// A resume in progress.
pub struct Resume {
    member: usize,
    state: ResumeState,
    since_save: u64,
    cursor: Cursor,
    copied: u64,
}

impl Resume {
    /// The member being resumed.
    #[must_use]
    pub fn member(&self) -> usize {
        self.member
    }

    /// Chunks this resume has copied, not counting those it found done
    /// or had no need to copy.
    #[must_use]
    pub fn copied(&self) -> u64 {
        self.copied
    }

    /// Progress so far.
    #[must_use]
    pub fn progress(&self) -> ResumeProgress {
        let chunks = self.state.chunks;
        let done = chunks - self.cursor.left();
        ResumeProgress {
            unit: u16::try_from(self.member).unwrap_or(u16::MAX),
            chunks_done: done,
            chunks,
            percent: (done * 100)
                .checked_div(chunks)
                .map_or(100, |p| u8::try_from(p).unwrap_or(100)),
            complete: self.cursor.left() == 0,
        }
    }
}

fn halves(members: &[MirrorMember]) -> Vec<Half<'_>> {
    members
        .iter()
        .enumerate()
        .map(|(i, m)| Half {
            unit: u16::try_from(i).expect("few members"),
            state: m.state,
            dev: m.dev.as_deref(),
        })
        .collect()
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn intent_error(op: &'static str, lba: u64, source: std::io::Error) -> BlockError {
    BlockError::Io { op, lba, source }
}

impl MirrorSet {
    /// The size of a set over these devices: the smallest of them.
    #[must_use]
    pub fn blocks_for(devs: &[&dyn BlockDev]) -> u64 {
        devs.iter().map(|d| d.blocks()).min().unwrap_or(0)
    }

    /// A set of `blocks` blocks over its members as the caller recorded
    /// them. A member without a device is missing and the set runs
    /// without it; one recorded missing or failed that is back is stale
    /// and stays suspended until it is resumed. At least one member
    /// must be active and attached. The set keeps no write-intent map:
    /// see [`open_with`](Self::open_with).
    pub fn open(id: [u8; 16], blocks: u64, members: Vec<MirrorMember>) -> Result<Self, SetError> {
        Self::open_with(id, blocks, members, None)
    }

    /// As [`open`](Self::open), keeping a write-intent map when `intent`
    /// is given. Regions the kept map has set are copied from the first
    /// active member to the other active ones by
    /// [`resync_step`](Self::resync_step); until a region is, it is read
    /// from that member only.
    pub fn open_with(
        id: [u8; 16],
        blocks: u64,
        mut members: Vec<MirrorMember>,
        intent: Option<Intent>,
    ) -> Result<Self, SetError> {
        if members.len() < 2 {
            return Err(SetError::Unusable(format!(
                "{} member(s); a mirrored set has two or more",
                members.len()
            )));
        }
        let mut events = Vec::new();
        for (i, m) in members.iter_mut().enumerate() {
            match (&m.dev, m.state) {
                (None, UnitState::Missing) => {}
                (None, _) => {
                    m.state = UnitState::Missing;
                    events.push(SetEvent::Missing { member: i });
                }
                (Some(d), _) if d.blocks() < blocks => {
                    return Err(SetError::refused(
                        i,
                        format!("{} blocks, the set has {blocks}", d.blocks()),
                    ));
                }
                (Some(_), UnitState::Missing | UnitState::NotOperational) => {
                    m.state = UnitState::Suspended;
                    events.push(SetEvent::Stale { member: i });
                }
                (Some(_), UnitState::Unprotected) => m.state = UnitState::Active,
                (Some(_), _) => {}
            }
        }
        let active = members
            .iter()
            .filter(|m| m.state == UnitState::Active && m.dev.is_some())
            .count();
        if active == 0 {
            return Err(SetError::Unusable("no member is active".into()));
        }
        let tracker = match intent {
            None => None,
            Some(intent) => {
                let rb = intent.region_blocks;
                if rb == 0 || !rb.is_multiple_of(RESUME_CHUNK) {
                    return Err(SetError::Unusable(format!(
                        "a region of {rb} blocks; a region is a multiple of {RESUME_CHUNK}"
                    )));
                }
                let info: Vec<([u8; 16], UnitState)> =
                    members.iter().map(|m| (m.uuid, m.state)).collect();
                Some(Tracker::open(id, blocks, intent, &info, active).map_err(SetError::Intent)?)
            }
        };
        let resync = tracker.as_ref().filter(|t| t.pending()).map(|t| {
            let state = t.resync_map();
            let cursor = Cursor::new(&state);
            info!(
                "mirrored set: {} of {} chunks to copy between the active members",
                cursor.left(),
                state.chunks
            );
            Resync {
                dirty: cursor.left(),
                state,
                cursor,
                since_save: 0,
                copied: 0,
                complete: false,
            }
        });
        Ok(Self {
            inner: Arc::new(Inner {
                id,
                blocks,
                members: RwLock::new(members),
                turn: AtomicU64::new(0),
                events: Mutex::new(events),
                intent: tracker.map(Mutex::new),
                resyncing: AtomicBool::new(resync.is_some()),
                resync: Mutex::new(resync),
            }),
        })
    }

    fn read_lock(&self) -> RwLockReadGuard<'_, Vec<MirrorMember>> {
        self.inner
            .members
            .read()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn write_lock(&self) -> RwLockWriteGuard<'_, Vec<MirrorMember>> {
        self.inner
            .members
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn tracker(&self) -> Option<MutexGuard<'_, Tracker>> {
        self.inner.intent.as_ref().map(lock)
    }

    fn push_event(&self, e: SetEvent) {
        lock(&self.inner.events).push(e);
    }

    /// Take the failed members out of service.
    fn apply(&self, fails: Fails) {
        if fails.is_empty() {
            return;
        }
        let mut members = self.write_lock();
        for (_, unit, what) in fails {
            let i = usize::from(unit);
            let others = Halves {
                set: 0,
                halves: &halves(&members),
            }
            .others(unit);
            if others && members[i].state != UnitState::NotOperational {
                warn!("mirrored set: member {i} taken out of service: {what}");
                let was = members[i].state;
                members[i].state = UnitState::NotOperational;
                if was.writable()
                    && let Some(mut t) = self.tracker()
                {
                    t.left(i, members[i].uuid, was == UnitState::Resuming);
                }
                self.push_event(SetEvent::Failed { member: i, what });
            }
        }
    }

    /// The set's identity.
    #[must_use]
    pub fn id(&self) -> [u8; 16] {
        self.inner.id
    }

    /// The members now, in order: what the caller records.
    #[must_use]
    pub fn members(&self) -> Vec<MemberInfo> {
        self.read_lock()
            .iter()
            .map(|m| MemberInfo {
                uuid: m.uuid,
                state: m.state,
                attached: m.dev.is_some(),
                blocks: m.dev.as_deref().map_or(0, BlockDev::blocks),
            })
            .collect()
    }

    /// Events since the last call.
    #[must_use]
    pub fn drain_events(&self) -> Vec<SetEvent> {
        std::mem::take(&mut *lock(&self.inner.events))
    }

    /// Whether another member than `member` is active and attached.
    fn active_other(members: &[MirrorMember], member: usize) -> bool {
        members
            .iter()
            .enumerate()
            .any(|(i, m)| i != member && m.state == UnitState::Active && m.dev.is_some())
    }

    fn check(members: &[MirrorMember], member: usize) -> Result<(), SetError> {
        if member >= members.len() {
            return Err(SetError::refused(member, "no such member"));
        }
        if !Self::active_other(members, member) {
            return Err(SetError::refused(member, "no other member is active"));
        }
        Ok(())
    }

    /// Stop writing to an active member. Another must be active.
    pub fn suspend(&self, member: usize) -> Result<(), SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        if members[member].state != UnitState::Active {
            return Err(SetError::refused(member, members[member].state.to_string()));
        }
        members[member].state = UnitState::Suspended;
        if let Some(mut t) = self.tracker() {
            t.left(member, members[member].uuid, false);
        }
        info!("mirrored set: member {member} suspended");
        Ok(())
    }

    /// Take a member's device away (it was pulled, or is to be): the
    /// member is missing from now on. Another must be active.
    pub fn detach(&self, member: usize) -> Result<Option<Box<dyn BlockDev>>, SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        let was = members[member].state;
        members[member].state = UnitState::Missing;
        if was.writable()
            && let Some(mut t) = self.tracker()
        {
            t.left(member, members[member].uuid, was == UnitState::Resuming);
        }
        info!("mirrored set: member {member} detached");
        Ok(members[member].dev.take())
    }

    /// Give a missing member its own device back (the disk that was
    /// pulled, not another): it is suspended until it is resumed, which
    /// copies only what was written while it was away when the set kept
    /// a map for it.
    pub fn reattach(&self, member: usize, dev: Box<dyn BlockDev>) -> Result<(), SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        let m = &mut members[member];
        if m.state != UnitState::Missing || m.dev.is_some() {
            return Err(SetError::refused(member, m.state.to_string()));
        }
        if dev.blocks() < self.inner.blocks {
            return Err(SetError::refused(
                member,
                format!("{} blocks, the set has {}", dev.blocks(), self.inner.blocks),
            ));
        }
        m.dev = Some(dev);
        m.state = UnitState::Suspended;
        info!("mirrored set: member {member} attached again; suspended");
        Ok(())
    }

    /// Put a device in the place of a missing, failed or suspended
    /// member. It becomes the resuming member; the caller records that
    /// and runs the resume.
    pub fn replace(
        &self,
        member: usize,
        dev: Box<dyn BlockDev>,
        uuid: [u8; 16],
    ) -> Result<(), SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        if !matches!(
            members[member].state,
            UnitState::Missing | UnitState::NotOperational | UnitState::Suspended
        ) {
            return Err(SetError::refused(member, members[member].state.to_string()));
        }
        if dev.blocks() < self.inner.blocks {
            return Err(SetError::refused(
                member,
                format!("{} blocks, the set has {}", dev.blocks(), self.inner.blocks),
            ));
        }
        let m = &mut members[member];
        m.dev = Some(dev);
        m.uuid = uuid;
        m.state = UnitState::Resuming;
        if let Some(mut t) = self.tracker() {
            t.back(member);
        }
        info!("mirrored set: member {member} replaced; resuming");
        Ok(())
    }

    /// The members a hot spare could replace now: missing or failed
    /// beside an active one.
    #[must_use]
    pub fn failed(&self) -> Vec<usize> {
        let members = self.read_lock();
        (0..members.len())
            .filter(|&i| {
                matches!(
                    members[i].state,
                    UnitState::Missing | UnitState::NotOperational
                ) && Self::active_other(&members, i)
            })
            .collect()
    }

    /// Give the first failed member the smallest hot spare that fits.
    /// The member it replaced and its new identity; the caller records
    /// them and runs the resume.
    pub fn take_spare(&self, spares: &mut Vec<Spare>) -> Option<(usize, [u8; 16])> {
        let member = *self.failed().first()?;
        let s = spare::take(spares, self.inner.blocks)?;
        let uuid = s.uuid;
        match self.replace(member, s.dev, uuid) {
            Ok(()) => Some((member, uuid)),
            Err(e) => {
                warn!("mirrored set: hot spare not used: {e}");
                None
            }
        }
    }

    /// Start bringing a member back in step, or go on with the resume
    /// the journal remembers. [`resume_step`](Self::resume_step) copies.
    /// With a write-intent map, a member whose own map was kept since it
    /// left in step is copied only where that map is set, and a resume
    /// the journal remembers copies again the regions written since it
    /// was saved.
    pub fn resume(
        &self,
        member: usize,
        journal: &mut dyn ResumeJournal,
    ) -> Result<Resume, SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        let m = &mut members[member];
        if m.dev.is_none() {
            return Err(SetError::refused(member, "not attached"));
        }
        match m.state {
            UnitState::Suspended | UnitState::Resuming | UnitState::NotOperational => {}
            other => return Err(SetError::refused(member, other.to_string())),
        }
        let blocks = self.inner.blocks;
        let tracker = self.inner.intent.as_ref().map(lock);
        let kept = journal
            .load(m.uuid)
            .filter(|s| s.asp_uuid == self.inner.id && s.chunks == blocks.div_ceil(RESUME_CHUNK))
            .filter(|_| !tracker.as_ref().is_some_and(|t| t.lost(member)));
        let state = match (kept, &tracker) {
            (Some(mut s), Some(t)) => {
                t.redo(member, &mut s);
                s
            }
            (Some(s), None) => s,
            (None, t) => t
                .as_ref()
                .and_then(|t| t.guide(member, m.uuid))
                .unwrap_or_else(|| ResumeState::new(self.inner.id, m.uuid, blocks)),
        };
        drop(tracker);
        m.state = UnitState::Resuming;
        journal.save(&state).map_err(SetError::Journal)?;
        let cursor = Cursor::new(&state);
        info!(
            "mirrored set: member {member} resuming, {} of {} chunks to copy",
            cursor.left(),
            state.chunks
        );
        Ok(Resume {
            member,
            state,
            since_save: 0,
            cursor,
            copied: 0,
        })
    }

    /// Copy up to `budget` chunks onto the resuming member. Transfers
    /// wait while a chunk is copied, so none lands between its read and
    /// its write. When the last chunk lands the member is active and the
    /// journal is cleared; the caller records the state.
    pub fn resume_step(
        &self,
        resume: &mut Resume,
        journal: &mut dyn ResumeJournal,
        budget: u64,
    ) -> Result<ResumeProgress, SetError> {
        for _ in 0..budget {
            let mut members = self.write_lock();
            let source = (0..members.len())
                .find(|&i| {
                    i != resume.member
                        && members[i].state == UnitState::Active
                        && members[i].dev.is_some()
                })
                .ok_or_else(|| SetError::refused(resume.member, "no other member is active"))?;
            if members[resume.member].dev.is_none() {
                return Err(SetError::refused(resume.member, "not attached"));
            }
            let mut io = Copy {
                source: members[source].dev.as_deref().expect("checked"),
                target: members[resume.member].dev.as_deref().expect("checked"),
                journal: &mut *journal,
            };
            let before = resume.cursor.left();
            let complete = copy_next(
                &mut io,
                &mut resume.state,
                &mut resume.cursor,
                &mut resume.since_save,
                self.inner.blocks,
                1,
            )?;
            resume.copied += before - resume.cursor.left();
            if complete {
                members[resume.member].state = UnitState::Active;
                drop(members);
                journal.clear().map_err(SetError::Journal)?;
                if let Some(mut t) = self.tracker() {
                    t.back(resume.member);
                }
                info!(
                    "mirrored set: member {} resumed, {} chunks copied",
                    resume.member, resume.copied
                );
                break;
            }
        }
        Ok(resume.progress())
    }

    /// Copy up to `budget` chunks of the regions an unclean stop left
    /// dirty, from the first active member to the other active ones.
    /// Transfers wait while a chunk is copied, as for a resume. A region
    /// is taken as in step once its chunks are copied and flushed; its
    /// bit is then cleared as any other. `None` when the set had nothing
    /// to copy at open (or keeps no map).
    pub fn resync_step(&self, budget: u64) -> Result<Option<ResyncProgress>, SetError> {
        let Some(intent) = self.inner.intent.as_ref() else {
            return Ok(None);
        };
        for _ in 0..budget {
            let mut fails = Fails::new();
            let step = {
                let members = self.write_lock();
                let mut guard = lock(&self.inner.resync);
                let Some(run) = guard.as_mut().filter(|r| !r.complete) else {
                    break;
                };
                let active: Vec<usize> = (0..members.len())
                    .filter(|&i| members[i].state == UnitState::Active && members[i].dev.is_some())
                    .collect();
                if active.len() < 2 {
                    Ok(true)
                } else {
                    let mut io = Fan {
                        source: (
                            active[0],
                            members[active[0]].dev.as_deref().expect("attached"),
                        ),
                        targets: active[1..]
                            .iter()
                            .map(|&i| (i, members[i].dev.as_deref().expect("attached")))
                            .collect(),
                        fails: &mut fails,
                        saved: false,
                    };
                    let before = run.cursor.left();
                    let r = copy_next(
                        &mut io,
                        &mut run.state,
                        &mut run.cursor,
                        &mut run.since_save,
                        self.inner.blocks,
                        1,
                    );
                    run.copied += before - run.cursor.left();
                    if io.saved {
                        lock(intent).resynced_below(run.cursor.next());
                    }
                    r
                }
            };
            let source_failed = !fails.is_empty();
            self.apply(fails);
            match step {
                Ok(false) => {}
                Ok(true) => {
                    self.resync_complete();
                    break;
                }
                // The source failed: the next step reads another.
                Err(_) if source_failed => {}
                Err(e) => return Err(e),
            }
        }
        Ok(self.resync_progress())
    }

    fn resync_complete(&self) {
        let mut guard = lock(&self.inner.resync);
        if let Some(run) = guard.as_mut()
            && !run.complete
        {
            run.complete = true;
            if let Some(mut t) = self.tracker() {
                t.resynced();
            }
            self.inner.resyncing.store(false, Ordering::Release);
            info!(
                "mirrored set: active members in step, {} chunks copied",
                run.copied
            );
        }
    }

    /// The copy between active members: `None` when there was none.
    #[must_use]
    pub fn resync_progress(&self) -> Option<ResyncProgress> {
        lock(&self.inner.resync).as_ref().map(|r| ResyncProgress {
            chunks_copied: r.copied,
            chunks_dirty: r.dirty,
            chunks: r.state.chunks,
            complete: r.complete,
        })
    }

    /// Clear the bits of regions idle for the delay. A flush does this;
    /// a caller whose set may go quiet calls it from a timer. The
    /// regions cleared.
    pub fn sweep(&self) -> Result<u64, SetError> {
        self.tracker()
            .map_or(Ok(0), |mut t| t.sweep(false))
            .map_err(SetError::Intent)
    }

    /// Flush, and clear every bit that can be: the map of a set stopped
    /// cleanly has no region set (unless a copy was left unfinished).
    pub fn close(&self) -> Result<(), SetError> {
        self.flush()?;
        if let Some(mut t) = self.tracker() {
            t.sweep(true).map_err(SetError::Intent)?;
        }
        Ok(())
    }

    /// What the write-intent map has done; `None` without one.
    #[must_use]
    pub fn intent_stats(&self) -> Option<IntentStats> {
        self.tracker().map(|t| t.stats())
    }

    /// The regions set in the write-intent map now.
    #[must_use]
    pub fn dirty_regions(&self) -> Vec<u64> {
        self.tracker().map_or_else(Vec::new, |t| t.dirty())
    }

    /// Lend the members for a read of `lba..lba + count`: while a region
    /// it touches waits for the copy between active members, only the
    /// first active member serves it.
    fn read_halves<'a>(&self, members: &'a [MirrorMember], lba: u64, count: u64) -> Vec<Half<'a>> {
        let mut hs = halves(members);
        if self.inner.resyncing.load(Ordering::Acquire)
            && let Some(t) = self.tracker()
        {
            let (lo, hi) = t.regions_of(lba, count);
            if t.pending_in(lo, hi)
                && let Some(src) = hs
                    .iter()
                    .position(|h| h.state == UnitState::Active && h.dev.is_some())
            {
                for (i, h) in hs.iter_mut().enumerate() {
                    if i != src && h.state == UnitState::Active {
                        h.state = UnitState::Resuming;
                    }
                }
            }
        }
        hs
    }

    /// Count a write to `lba..lba + count` in the map, its bits durable
    /// first; the regions to end it with.
    fn begin(
        &self,
        op: &'static str,
        lba: u64,
        count: u64,
    ) -> blockdev::Result<Option<(u64, u64)>> {
        let Some(mut t) = self.tracker() else {
            return Ok(None);
        };
        if lba
            .checked_add(count)
            .is_none_or(|end| end > self.inner.blocks)
        {
            return Err(BlockError::OutOfRange {
                lba,
                count,
                blocks: self.inner.blocks,
            });
        }
        let (lo, hi) = t.regions_of(lba, count);
        t.begin(lo, hi).map_err(|e| intent_error(op, lba, e))?;
        Ok(Some((lo, hi)))
    }

    fn end(&self, span: Option<(u64, u64)>) {
        if let (Some((lo, hi)), Some(mut t)) = (span, self.tracker()) {
            t.end(lo, hi);
        }
    }
}

/// One chunk's copy, the members held.
struct Copy<'a> {
    source: &'a dyn BlockDev,
    target: &'a dyn BlockDev,
    journal: &'a mut dyn ResumeJournal,
}

impl ResumeIo for Copy<'_> {
    type Error = SetError;

    fn read(&mut self, lba: u64, count: u64) -> Result<AlignedBuf, SetError> {
        let mut buf = AlignedBuf::new(usize::try_from(count).expect("a chunk"));
        self.source.read(lba, &mut buf)?;
        Ok(buf)
    }

    fn wanted(&mut self, _lba: u64) -> bool {
        true
    }

    fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<(), SetError> {
        Ok(self.target.write(lba, bytes)?)
    }

    fn flush(&mut self) -> Result<(), SetError> {
        Ok(self.target.flush()?)
    }

    fn save(&mut self, state: &ResumeState) -> Result<(), SetError> {
        self.journal.save(state).map_err(SetError::Journal)
    }
}

/// One chunk's copy from the first active member to the others. A
/// member that fails is noted to be taken out of service; the copy goes
/// on without it.
struct Fan<'a, 'f> {
    source: (usize, &'a dyn BlockDev),
    targets: Vec<(usize, &'a dyn BlockDev)>,
    fails: &'f mut Fails,
    saved: bool,
}

impl Fan<'_, '_> {
    fn fail(&mut self, member: usize, what: String) {
        let unit = u16::try_from(member).expect("few members");
        if !self.fails.iter().any(|f| f.1 == unit) {
            self.fails.push((0, unit, what));
        }
    }

    fn failed(&self, member: usize) -> bool {
        self.fails.iter().any(|f| usize::from(f.1) == member)
    }
}

impl ResumeIo for Fan<'_, '_> {
    type Error = SetError;

    fn read(&mut self, lba: u64, count: u64) -> Result<AlignedBuf, SetError> {
        let mut buf = AlignedBuf::new(usize::try_from(count).expect("a chunk"));
        let (m, dev) = self.source;
        if let Err(e) = dev.read(lba, &mut buf) {
            self.fail(m, format!("read {count} pages at {lba} to copy: {e}"));
            return Err(e.into());
        }
        Ok(buf)
    }

    fn wanted(&mut self, _lba: u64) -> bool {
        true
    }

    fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<(), SetError> {
        for (m, dev) in self.targets.clone() {
            if !self.failed(m)
                && let Err(e) = dev.write(lba, bytes)
            {
                self.fail(m, format!("write page {lba} of a copy: {e}"));
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), SetError> {
        for (m, dev) in self.targets.clone() {
            if !self.failed(m)
                && let Err(e) = dev.flush()
            {
                self.fail(m, format!("flush a copy: {e}"));
            }
        }
        Ok(())
    }

    fn save(&mut self, _state: &ResumeState) -> Result<(), SetError> {
        self.saved = true;
        Ok(())
    }
}

fn block_error(e: MirrorError, op: &'static str, lba: u64) -> BlockError {
    match e {
        MirrorError::Block(e) => e,
        MirrorError::NoHalf => no_member(op, lba),
    }
}

impl BlockDev for MirrorSet {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.inner.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> blockdev::Result<()> {
        let count = check_range(self.inner.blocks, lba, buf.len())?;
        let mut fails = Fails::new();
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &self.read_halves(&members, lba, count),
            }
            .read_checked(lba, buf, lba, &self.inner.turn, &mut fails, |_: &[u8]| {
                Ok::<(), Infallible>(())
            })
        };
        self.apply(fails);
        match r {
            Ok(_) => Ok(()),
            Err(CheckedError::Block(e)) => Err(e),
            Err(CheckedError::NoHalf) => Err(no_member("read", lba)),
            Err(CheckedError::Check(never)) => match never {},
        }
    }

    fn write(&self, lba: u64, buf: &[u8]) -> blockdev::Result<()> {
        let count = check_range(self.inner.blocks, lba, buf.len())?;
        let span = self.begin("write", lba, count)?;
        let mut fails = Fails::new();
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &halves(&members),
            }
            .write(lba, buf, lba, &mut fails)
        };
        // A member that failed starts its own map before the regions
        // of this write can be cleared.
        self.apply(fails);
        self.end(span);
        r.map_err(|e| block_error(e, "write", lba))
    }

    fn flush(&self) -> blockdev::Result<()> {
        let epoch = self.tracker().map(|mut t| t.flush_begin());
        let mut fails = Fails::new();
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &halves(&members),
            }
            .flush(&mut fails)
        };
        self.apply(fails);
        if r.is_ok()
            && let (Some(e), Some(mut t)) = (epoch, self.tracker())
        {
            t.flush_done(e);
            if let Err(err) = t.sweep(false) {
                warn!("mirrored set: write-intent bits not cleared: {err}");
            }
        }
        r.map_err(|e| block_error(e, "flush", 0))
    }

    fn discard(&self, lba: u64, count: u64) -> blockdev::Result<()> {
        let span = self.begin("discard", lba, count)?;
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &halves(&members),
            }
            .discard(lba, count)
        };
        self.end(span);
        r
    }
}
