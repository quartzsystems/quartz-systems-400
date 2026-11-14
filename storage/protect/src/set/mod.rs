// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A protected set as one block device, for a caller with no pool of
//! its own: a file system goes straight on top.
//!
//! A set keeps nothing on its members that says what they are. The
//! caller knows which devices make the set and records each member's
//! identity and state where it keeps its own state: it gives them to
//! `open`, reads them back from `members` whenever an event says one
//! changed, and keeps the resume map through a [`ResumeJournal`] and a
//! mirrored set's write-intent map through an [`Intent`]. A caller that
//! wants a label on each disk puts the set over a [`Window`] that leaves
//! the label's blocks out.

mod intent;
mod mirror;
mod parity;
mod window;

use std::io;

use blockdev::BlockError;

use crate::mirror::ResumeState;
pub use intent::{Intent, IntentStats};
pub use mirror::{MemberInfo, MirrorMember, MirrorSet, Resume, ResyncProgress};
pub use parity::{ParityMember, ParityMemberInfo, ParityProgress, ParitySet, ParityWork};
pub use window::Window;

/// Why a set refused or failed.
#[derive(Debug, thiserror::Error)]
pub enum SetError {
    /// A member's device failed and the set could not go on without it.
    #[error(transparent)]
    Block(#[from] BlockError),
    /// The resume map could not be kept.
    #[error("resume map: {0}")]
    Journal(#[source] io::Error),
    /// The write-intent map could not be read or kept.
    #[error("write-intent map: {0}")]
    Intent(#[source] io::Error),
    /// The members given do not make a set that can run.
    #[error("the set cannot run: {0}")]
    Unusable(String),
    /// The operation does not apply to the member as it is.
    #[error("member {member}: {reason}")]
    Refused {
        /// The member.
        member: usize,
        /// Why.
        reason: String,
    },
}

impl SetError {
    pub(crate) fn refused(member: usize, reason: impl Into<String>) -> Self {
        SetError::Refused {
            member,
            reason: reason.into(),
        }
    }
}

/// Something a set did on its own. The caller records the members'
/// states again when it sees one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetEvent {
    /// A member failed I/O and was taken out of service.
    Failed {
        /// The member.
        member: usize,
        /// What failed.
        what: String,
    },
    /// A member was not given a device at open; the set runs without it.
    Missing {
        /// The member.
        member: usize,
    },
    /// A member recorded as failed or missing is back; its data is
    /// stale until it is resumed or rebuilt.
    Stale {
        /// The member.
        member: usize,
    },
    /// A block computed from the others was written back to a member.
    Healed {
        /// The member.
        member: usize,
        /// The member's block.
        lba: u64,
    },
}

/// Keeps a resume's map across a crash.
pub trait ResumeJournal {
    /// The map saved for `unit_uuid`, if any.
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState>;
    /// Make `state` durable.
    fn save(&mut self, state: &ResumeState) -> io::Result<()>;
    /// Forget the saved map.
    fn clear(&mut self) -> io::Result<()>;
}

/// A journal in memory, for tests and sets that start over after a crash.
#[derive(Debug, Default)]
pub struct MemJournal {
    /// The saved map.
    pub saved: Option<ResumeState>,
}

impl ResumeJournal for MemJournal {
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState> {
        self.saved.clone().filter(|s| s.unit_uuid == unit_uuid)
    }
    fn save(&mut self, state: &ResumeState) -> io::Result<()> {
        self.saved = Some(state.clone());
        Ok(())
    }
    fn clear(&mut self) -> io::Result<()> {
        self.saved = None;
        Ok(())
    }
}

/// A device error for a set with no member to serve a transfer.
pub(crate) fn no_member(op: &'static str, lba: u64) -> BlockError {
    BlockError::Io {
        op,
        lba,
        source: io::Error::new(io::ErrorKind::NotFound, "no member of the set is available"),
    }
}
