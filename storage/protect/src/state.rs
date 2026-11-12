// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The states of a unit.

use std::fmt;

/// What a unit is doing, named as the disk configuration displays
/// name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitState {
    /// Not mirrored.
    #[default]
    Unprotected,
    /// Half of a pair, in step with its partner.
    Active,
    /// Half of a pair, not written to; its data is stale.
    Suspended,
    /// Half of a pair being brought back in step.
    Resuming,
    /// Present but failing I/O.
    NotOperational,
    /// Not found at IPL.
    Missing,
}

impl UnitState {
    /// The byte a unit table records.
    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            UnitState::Unprotected => 0,
            UnitState::Active => 1,
            UnitState::Suspended => 2,
            UnitState::Resuming => 3,
            UnitState::NotOperational => 4,
            UnitState::Missing => 5,
        }
    }

    /// From the unit table byte.
    #[must_use]
    pub fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => UnitState::Unprotected,
            1 => UnitState::Active,
            2 => UnitState::Suspended,
            3 => UnitState::Resuming,
            4 => UnitState::NotOperational,
            5 => UnitState::Missing,
            _ => return None,
        })
    }

    /// Whether the unit is half of a pair.
    #[must_use]
    pub fn mirrored(self) -> bool {
        self != UnitState::Unprotected
    }

    /// Whether reads may be served from the unit.
    #[must_use]
    pub fn readable(self) -> bool {
        matches!(self, UnitState::Unprotected | UnitState::Active)
    }

    /// Whether writes must go to the unit.
    #[must_use]
    pub fn writable(self) -> bool {
        matches!(
            self,
            UnitState::Unprotected | UnitState::Active | UnitState::Resuming
        )
    }

    /// Whether the unit is attached and answering.
    #[must_use]
    pub fn present(self) -> bool {
        !matches!(self, UnitState::Missing | UnitState::NotOperational)
    }
}

impl fmt::Display for UnitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(match self {
            UnitState::Unprotected => "Unprotected",
            UnitState::Active => "Active",
            UnitState::Suspended => "Suspended",
            UnitState::Resuming => "Resuming",
            UnitState::NotOperational => "Not operational",
            UnitState::Missing => "Missing",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_and_name_themselves() {
        for v in 0..6 {
            let s = UnitState::from_u8(v).unwrap();
            assert_eq!(s.as_u8(), v);
        }
        assert!(UnitState::from_u8(6).is_none());
        assert_eq!(UnitState::NotOperational.to_string(), "Not operational");
        assert!(UnitState::Resuming.writable() && !UnitState::Resuming.readable());
        assert!(!UnitState::Suspended.writable());
        assert!(!UnitState::Missing.present());
    }
}
