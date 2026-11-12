// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A mirrored set as one block device: every member holds every block.

use std::convert::Infallible;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use blockdev::{AlignedBuf, BLOCK_SIZE, BlockDev, BlockError, check_range};
use tracing::{info, warn};

use crate::UnitState;
use crate::mirror::{
    CheckedError, Fails, Half, Halves, MirrorError, ResumeIo, ResumeProgress, ResumeState,
    copy_step,
};
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

struct Inner {
    id: [u8; 16],
    blocks: u64,
    members: RwLock<Vec<MirrorMember>>,
    turn: AtomicU64,
    events: Mutex<Vec<SetEvent>>,
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
}

impl Resume {
    /// The member being resumed.
    #[must_use]
    pub fn member(&self) -> usize {
        self.member
    }

    /// Progress so far.
    #[must_use]
    pub fn progress(&self) -> ResumeProgress {
        ResumeProgress {
            unit: u16::try_from(self.member).unwrap_or(u16::MAX),
            chunks_done: self.state.chunks_done(),
            chunks: self.state.chunks,
            percent: self.state.percent(),
            complete: self.state.complete(),
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
    /// must be active and attached.
    pub fn open(
        id: [u8; 16],
        blocks: u64,
        mut members: Vec<MirrorMember>,
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
        if !members
            .iter()
            .any(|m| m.state == UnitState::Active && m.dev.is_some())
        {
            return Err(SetError::Unusable("no member is active".into()));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                id,
                blocks,
                members: RwLock::new(members),
                turn: AtomicU64::new(0),
                events: Mutex::new(events),
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

    fn push_event(&self, e: SetEvent) {
        self.inner
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(e);
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
                members[i].state = UnitState::NotOperational;
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
        std::mem::take(
            &mut *self
                .inner
                .events
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
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
        info!("mirrored set: member {member} suspended");
        Ok(())
    }

    /// Take a member's device away (it was pulled, or is to be): the
    /// member is missing from now on. Another must be active.
    pub fn detach(&self, member: usize) -> Result<Option<Box<dyn BlockDev>>, SetError> {
        let mut members = self.write_lock();
        Self::check(&members, member)?;
        members[member].state = UnitState::Missing;
        info!("mirrored set: member {member} detached");
        Ok(members[member].dev.take())
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
        let state = journal
            .load(m.uuid)
            .filter(|s| {
                s.asp_uuid == self.inner.id
                    && s.chunks == blocks.div_ceil(crate::mirror::RESUME_CHUNK)
            })
            .unwrap_or_else(|| ResumeState::new(self.inner.id, m.uuid, blocks));
        m.state = UnitState::Resuming;
        journal.save(&state).map_err(SetError::Journal)?;
        info!(
            "mirrored set: member {member} resuming, {} of {} chunks already copied",
            state.chunks_done(),
            state.chunks
        );
        Ok(Resume {
            member,
            state,
            since_save: 0,
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
            let complete = copy_step(
                &mut io,
                &mut resume.state,
                &mut resume.since_save,
                self.inner.blocks,
                1,
            )?;
            if complete {
                members[resume.member].state = UnitState::Active;
                drop(members);
                journal.clear().map_err(SetError::Journal)?;
                info!("mirrored set: member {} resumed", resume.member);
                break;
            }
        }
        Ok(resume.progress())
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
        check_range(self.inner.blocks, lba, buf.len())?;
        let mut fails = Fails::new();
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &halves(&members),
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
        check_range(self.inner.blocks, lba, buf.len())?;
        let mut fails = Fails::new();
        let r = {
            let members = self.read_lock();
            Halves {
                set: 0,
                halves: &halves(&members),
            }
            .write(lba, buf, lba, &mut fails)
        };
        self.apply(fails);
        r.map_err(|e| block_error(e, "write", lba))
    }

    fn flush(&self) -> blockdev::Result<()> {
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
        r.map_err(|e| block_error(e, "flush", 0))
    }

    fn discard(&self, lba: u64, count: u64) -> blockdev::Result<()> {
        let members = self.read_lock();
        Halves {
            set: 0,
            halves: &halves(&members),
        }
        .discard(lba, count)
    }
}
