// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The protection level of a parity set.

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
