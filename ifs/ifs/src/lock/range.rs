// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Byte-range locks with POSIX `fcntl` semantics: an owner's locks never
//! conflict with each other, a new lock replaces the owner's coverage of
//! its range (splitting what it overlaps), adjacent equal locks merge,
//! and a length of 0 runs to the end of file and beyond.

use super::Owner;

/// `l_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeKind {
    /// `F_RDLCK`.
    Read,
    /// `F_WRLCK`.
    Write,
    /// `F_UNLCK`.
    Unlock,
}

impl RangeKind {
    /// From IBM's `l_type` (`F_RDLCK` 1, `F_WRLCK` 2, `F_UNLCK` 3).
    #[must_use]
    pub fn from_ltype(t: i16) -> Option<Self> {
        match t {
            1 => Some(Self::Read),
            2 => Some(Self::Write),
            3 => Some(Self::Unlock),
            _ => None,
        }
    }

    /// IBM's `l_type`.
    #[must_use]
    pub fn ltype(self) -> i16 {
        match self {
            Self::Read => 1,
            Self::Write => 2,
            Self::Unlock => 3,
        }
    }
}

/// A lock on bytes `start..end` (`end` exclusive; `u64::MAX` is to the
/// end of file and beyond).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RangeLock {
    /// Holder.
    pub owner: Owner,
    /// Kind.
    pub kind: RangeKind,
    /// First byte.
    pub start: u64,
    /// Byte after the last.
    pub end: u64,
}

impl RangeLock {
    /// A lock from an absolute start and `l_len` (0: to the end).
    #[must_use]
    pub fn new(owner: Owner, kind: RangeKind, start: u64, len: u64) -> Self {
        Self {
            owner,
            kind,
            start,
            end: if len == 0 {
                u64::MAX
            } else {
                start.saturating_add(len)
            },
        }
    }

    /// `l_len` for `F_GETLK` (0 when it runs to the end).
    #[must_use]
    pub fn len(&self) -> u64 {
        if self.end == u64::MAX {
            0
        } else {
            self.end - self.start
        }
    }

    /// Whether it covers no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    fn overlaps(&self, o: &Self) -> bool {
        self.start < o.end && o.start < self.end
    }
}

/// Whether held lock `l` blocks `want`.
#[must_use]
pub fn blocks(l: &RangeLock, want: &RangeLock) -> bool {
    l.owner != want.owner
        && l.overlaps(want)
        && (l.kind == RangeKind::Write || want.kind == RangeKind::Write)
}

/// Apply `want` (a lock or an unlock) to an object's locks; the caller
/// has checked it does not conflict.
pub fn apply(locks: &mut Vec<RangeLock>, want: RangeLock) {
    let mut out = Vec::with_capacity(locks.len() + 2);
    for l in locks.drain(..) {
        if l.owner != want.owner || !l.overlaps(&want) {
            out.push(l);
            continue;
        }
        if l.start < want.start {
            out.push(RangeLock {
                end: want.start,
                ..l
            });
        }
        if l.end > want.end {
            out.push(RangeLock {
                start: want.end,
                ..l
            });
        }
    }
    if want.kind != RangeKind::Unlock {
        out.push(want);
    }
    out.sort_by_key(|l| (l.owner, l.start));
    // Merge an owner's adjacent locks of the same kind.
    let mut merged: Vec<RangeLock> = Vec::with_capacity(out.len());
    for l in out {
        if let Some(last) = merged.last_mut()
            && last.owner == l.owner
            && last.kind == l.kind
            && last.end == l.start
        {
            last.end = l.end;
            continue;
        }
        merged.push(l);
    }
    *locks = merged;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(owner: u64, kind: RangeKind, start: u64, end: u64) -> RangeLock {
        RangeLock {
            owner: Owner(owner),
            kind,
            start,
            end,
        }
    }

    #[test]
    fn split_and_merge() {
        use RangeKind::{Read, Unlock, Write};
        let mut v = Vec::new();
        apply(&mut v, RangeLock::new(Owner(1), Write, 0, 100));
        apply(&mut v, RangeLock::new(Owner(1), Read, 40, 20));
        assert_eq!(
            v,
            vec![l(1, Write, 0, 40), l(1, Read, 40, 60), l(1, Write, 60, 100)]
        );
        apply(&mut v, RangeLock::new(Owner(1), Write, 40, 20));
        assert_eq!(v, vec![l(1, Write, 0, 100)]);
        apply(&mut v, RangeLock::new(Owner(1), Unlock, 10, 0));
        assert_eq!(v, vec![l(1, Write, 0, 10)]);
        apply(&mut v, RangeLock::new(Owner(2), Read, 50, 0));
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].len(), 0);
    }

    #[test]
    fn conflicts() {
        use RangeKind::{Read, Write};
        let held = l(1, Read, 0, 10);
        assert!(!blocks(&held, &l(2, Read, 5, 6)));
        assert!(blocks(&held, &l(2, Write, 9, 20)));
        assert!(!blocks(&held, &l(2, Write, 10, 20)));
        assert!(!blocks(&held, &l(1, Write, 0, 10)));
        assert!(blocks(
            &l(1, Write, 0, u64::MAX),
            &l(2, Read, 1 << 40, (1 << 40) + 1)
        ));
        assert_eq!(RangeKind::from_ltype(2), Some(Write));
        assert_eq!(Write.ltype(), 2);
    }
}
