// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The two slots and the choice between them, from the copies'
//! sequence numbers (`None` is an invalid or empty copy).

/// One of the two: a copy of the record, or a boot slot on the ESP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// The first.
    A,
    /// The second.
    B,
}

impl Slot {
    /// The other slot.
    #[must_use]
    pub fn other(self) -> Slot {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    /// 0 for A, 1 for B: its place in a pair, and its byte on disk.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Slot::A => 0,
            Slot::B => 1,
        }
    }
}

/// The copy a read serves: the one with the higher sequence, A when
/// they are equal.
#[must_use]
pub fn newest(seq: [Option<u64>; 2]) -> Option<Slot> {
    match seq {
        [Some(a), Some(b)] => Some(if b > a { Slot::B } else { Slot::A }),
        [Some(_), None] => Some(Slot::A),
        [None, Some(_)] => Some(Slot::B),
        [None, None] => None,
    }
}

/// The copy a write replaces and the sequence it gets: an invalid copy
/// first, else the older one (A when they are equal), numbered one past
/// the copy that stays. The newest good copy is never the target.
#[must_use]
pub fn target(seq: [Option<u64>; 2]) -> (Slot, u64) {
    let (slot, kept) = match seq {
        [None, b] => (Slot::A, b),
        [a @ Some(_), None] => (Slot::B, a),
        [Some(a), Some(b)] => {
            if a <= b {
                (Slot::A, Some(b))
            } else {
                (Slot::B, Some(a))
            }
        }
    };
    (slot, kept.unwrap_or(0) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_takes_the_newest() {
        assert_eq!(newest([None, None]), None);
        assert_eq!(newest([Some(4), None]), Some(Slot::A));
        assert_eq!(newest([None, Some(1)]), Some(Slot::B));
        assert_eq!(newest([Some(4), Some(5)]), Some(Slot::B));
        assert_eq!(newest([Some(5), Some(4)]), Some(Slot::A));
        assert_eq!(newest([Some(5), Some(5)]), Some(Slot::A));
    }

    #[test]
    fn a_write_never_takes_the_newest() {
        assert_eq!(target([None, None]), (Slot::A, 1));
        assert_eq!(target([Some(1), None]), (Slot::B, 2));
        assert_eq!(target([None, Some(2)]), (Slot::A, 3));
        assert_eq!(target([Some(3), Some(2)]), (Slot::B, 4));
        assert_eq!(target([Some(3), Some(4)]), (Slot::A, 5));
        assert_eq!(target([Some(5), Some(5)]), (Slot::A, 6));
        assert_eq!(Slot::A.other(), Slot::B);
        assert_eq!(Slot::B.other().index(), 0);
    }
}
