// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the superblock records about parity sets: per unit its set,
//! position, level and state, and the one piece of work under way. See
//! `docs/STORE_FORMAT.md` §5 and §11.

use std::fmt;

/// The protection level of a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Level {
    /// One parity page per stripe (`*PARITY`).
    Raid5,
    /// Two parity pages per stripe (`*RAID6`).
    Raid6,
}

impl Level {
    /// The byte in the unit table.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            Level::Raid5 => 5,
            Level::Raid6 => 6,
        }
    }

    /// From the unit table byte.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            5 => Some(Level::Raid5),
            6 => Some(Level::Raid6),
            _ => None,
        }
    }

    /// Parity pages per stripe.
    #[must_use]
    pub fn parity_pages(self) -> usize {
        match self {
            Level::Raid5 => 1,
            Level::Raid6 => 2,
        }
    }

    /// Fewest units in a set (IBM's limit).
    #[must_use]
    pub fn min_units(self) -> usize {
        match self {
            Level::Raid5 => 3,
            Level::Raid6 => 4,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            Level::Raid5 => "RAID 5",
            Level::Raid6 => "RAID 6",
        })
    }
}

/// Most units in a set (IBM's limit).
pub const MAX_MEMBERS: usize = 18;
/// Most sets in an ASP.
pub const MAX_SETS: u8 = 15;

/// A member's state in the unit table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemberState {
    /// In step with the set.
    Active,
    /// The set's parity is being computed.
    Syncing,
    /// Being rebuilt from the others.
    Rebuilding,
    /// Missing or failing; its pages come from the others.
    Failed,
}

impl MemberState {
    /// The byte in the unit table.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            MemberState::Active => 0,
            MemberState::Syncing => 1,
            MemberState::Rebuilding => 2,
            MemberState::Failed => 3,
        }
    }

    /// From the unit table byte.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => MemberState::Active,
            1 => MemberState::Syncing,
            2 => MemberState::Rebuilding,
            3 => MemberState::Failed,
            _ => return None,
        })
    }
}

/// A unit's membership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParityTag {
    /// The unit, by UUID so renumbering leaves the tag in place.
    pub unit_uuid: [u8; 16],
    /// Set number, 1 to [`MAX_SETS`].
    pub set: u8,
    /// Position in the set, from 0.
    pub member: u8,
    /// The set's level.
    pub level: Level,
    /// The member's state.
    pub state: MemberState,
}

/// What the background work does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    /// Compute the set's parity.
    Sync,
    /// Rebuild one member.
    Rebuild,
}

impl WorkKind {
    /// The superblock byte.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            WorkKind::Sync => 1,
            WorkKind::Rebuild => 2,
        }
    }

    /// From the superblock byte.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(WorkKind::Sync),
            2 => Some(WorkKind::Rebuild),
            _ => None,
        }
    }
}

/// The synchronization or rebuild under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParityWork {
    /// The set.
    pub set: u8,
    /// Sync or rebuild.
    pub kind: WorkKind,
    /// The member rebuilt; 0 for a sync.
    pub member: u8,
    /// Blocks done.
    pub done: u64,
}

/// Every unit's membership and the work under way.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParityTable {
    /// Members of any set.
    pub tags: Vec<ParityTag>,
    /// The work, if any.
    pub work: Option<ParityWork>,
}

impl ParityTable {
    /// The tag of a unit.
    #[must_use]
    pub fn tag(&self, unit_uuid: &[u8; 16]) -> Option<&ParityTag> {
        self.tags.iter().find(|t| &t.unit_uuid == unit_uuid)
    }

    /// The tag of a unit, to change.
    pub fn tag_mut(&mut self, unit_uuid: &[u8; 16]) -> Option<&mut ParityTag> {
        self.tags.iter_mut().find(|t| &t.unit_uuid == unit_uuid)
    }

    /// Set numbers in use, ascending.
    #[must_use]
    pub fn sets(&self) -> Vec<u8> {
        let mut s: Vec<u8> = self.tags.iter().map(|t| t.set).collect();
        s.sort_unstable();
        s.dedup();
        s
    }

    /// The members of a set, by position.
    #[must_use]
    pub fn members(&self, set: u8) -> Vec<&ParityTag> {
        let mut m: Vec<&ParityTag> = self.tags.iter().filter(|t| t.set == set).collect();
        m.sort_by_key(|t| t.member);
        m
    }

    /// The lowest set number not in use.
    #[must_use]
    pub fn free_set(&self) -> Option<u8> {
        (1..=MAX_SETS).find(|n| self.tags.iter().all(|t| t.set != *n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_round_trip() {
        for v in 0..4 {
            assert_eq!(MemberState::from_u8(v).unwrap().as_u8(), v);
        }
        assert!(MemberState::from_u8(4).is_none());
        assert_eq!(Level::from_u8(6), Some(Level::Raid6));
        assert!(Level::from_u8(1).is_none());
        assert_eq!(WorkKind::from_u8(2), Some(WorkKind::Rebuild));
        assert_eq!(Level::Raid6.to_string(), "RAID 6");
    }

    #[test]
    fn sets_and_members() {
        let tag = |u: u8, set: u8, member: u8| ParityTag {
            unit_uuid: [u; 16],
            set,
            member,
            level: Level::Raid5,
            state: MemberState::Active,
        };
        let t = ParityTable {
            tags: vec![tag(1, 2, 1), tag(2, 2, 0), tag(3, 1, 0)],
            work: None,
        };
        assert_eq!(t.sets(), vec![1, 2]);
        assert_eq!(t.members(2)[0].unit_uuid, [2; 16]);
        assert_eq!(t.free_set(), Some(3));
    }
}
