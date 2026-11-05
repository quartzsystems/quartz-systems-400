// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rounding modes of `CURRENT DECFLOAT ROUNDING MODE`.

use std::cmp::Ordering;
use std::fmt;

/// How a `DECFLOAT` result that has too many digits is rounded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DfRounding {
    /// To the nearest; a tie to the even digit (IBM i's default).
    #[default]
    HalfEven,
    /// To the nearest; a tie away from zero.
    HalfUp,
    /// To the nearest; a tie towards zero.
    HalfDown,
    /// Towards zero (truncate).
    Down,
    /// Away from zero.
    Up,
    /// Towards positive infinity.
    Ceiling,
    /// Towards negative infinity.
    Floor,
}

impl DfRounding {
    /// Every mode.
    pub const ALL: [Self; 7] =
        [Self::HalfEven, Self::HalfUp, Self::HalfDown, Self::Down, Self::Up, Self::Ceiling, Self::Floor];

    /// The SQL name: `ROUND_HALF_EVEN` ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::HalfEven => "ROUND_HALF_EVEN",
            Self::HalfUp => "ROUND_HALF_UP",
            Self::HalfDown => "ROUND_HALF_DOWN",
            Self::Down => "ROUND_DOWN",
            Self::Up => "ROUND_UP",
            Self::Ceiling => "ROUND_CEILING",
            Self::Floor => "ROUND_FLOOR",
        }
    }

    /// The mode an SQL name spells (any case).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        let n = name.trim().to_ascii_uppercase();
        Self::ALL.into_iter().find(|m| m.name() == n)
    }

    /// Whether dropping `dropped` (its high digit last) from a value
    /// whose kept digits end in `last` and whose sign is `neg` rounds
    /// the kept magnitude up by one.
    pub(crate) fn rounds_up(self, neg: bool, last: u8, dropped: &[u8]) -> bool {
        let Some((&first, rest)) = dropped.split_last() else { return false };
        let nonzero = first != 0 || rest.iter().any(|d| *d != 0);
        if !nonzero {
            return false;
        }
        let half = match first.cmp(&5) {
            Ordering::Less => Ordering::Less,
            Ordering::Greater => Ordering::Greater,
            Ordering::Equal if rest.iter().any(|d| *d != 0) => Ordering::Greater,
            Ordering::Equal => Ordering::Equal,
        };
        match self {
            Self::HalfEven => half == Ordering::Greater || (half == Ordering::Equal && last % 2 == 1),
            Self::HalfUp => half != Ordering::Less,
            Self::HalfDown => half == Ordering::Greater,
            Self::Down => false,
            Self::Up => true,
            Self::Ceiling => !neg,
            Self::Floor => neg,
        }
    }
}

impl fmt::Display for DfRounding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
