// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who a job is: name, user and a six-digit number, written
//! `number/user/name`, and the 16-byte internal job identifier the APIs
//! hand out.

use std::fmt;

use qsname::Name;

/// The highest job number; the counter wraps to 1 after it.
pub const MAX_NUMBER: u32 = 999_999;

/// A job's qualified name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId {
    /// Job name.
    pub name: Name,
    /// User profile.
    pub user: Name,
    /// Job number, 1 to 999999.
    pub number: u32,
}

impl JobId {
    /// From text parts; `None` when a name is not valid.
    #[must_use]
    pub fn new(name: &str, user: &str, number: u32) -> Option<Self> {
        Some(Self {
            name: Name::parse(name).ok()?,
            user: Name::parse(user).ok()?,
            number,
        })
    }

    /// `000123`.
    #[must_use]
    pub fn number_text(&self) -> String {
        format!("{:06}", self.number)
    }

    /// Parse `number/user/name`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.trim().split('/');
        let number = parts.next()?.parse().ok()?;
        let user = parts.next()?;
        let name = parts.next()?;
        if parts.next().is_some() || number == 0 || number > MAX_NUMBER {
            return None;
        }
        Self::new(name, user, number)
    }

    /// The 16-byte internal job identifier: `QJ`, the number
    /// big-endian, the start stamp, a version byte and a check byte.
    /// Callers treat it as opaque; [`InternalId::number`] reads it back.
    #[must_use]
    pub fn internal(&self, started: u64) -> InternalId {
        let mut b = [0u8; 16];
        b[0..2].copy_from_slice(b"QJ");
        b[2..6].copy_from_slice(&self.number.to_be_bytes());
        b[6..14].copy_from_slice(&started.to_be_bytes());
        b[14] = 1;
        b[15] = b[..15].iter().fold(0u8, |a, x| a.wrapping_add(*x));
        InternalId(b)
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:06}/{}/{}", self.number, self.user, self.name)
    }
}

/// The internal job identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InternalId(pub [u8; 16]);

impl InternalId {
    /// The job number it carries, when it is one of ours.
    #[must_use]
    pub fn number(&self) -> Option<u32> {
        let b = &self.0;
        let sum = b[..15].iter().fold(0u8, |a, x| a.wrapping_add(*x));
        (b[0..2] == *b"QJ" && b[15] == sum)
            .then(|| u32::from_be_bytes(b[2..6].try_into().expect("sized")))
    }
}

/// The next job number after `n`, skipping the ones `in_use` says are
/// taken; wraps after [`MAX_NUMBER`]. `None` when every number is taken.
pub fn next_number(n: u32, in_use: impl Fn(u32) -> bool) -> Option<u32> {
    let mut candidate = n;
    for _ in 0..MAX_NUMBER {
        candidate = if candidate >= MAX_NUMBER {
            1
        } else {
            candidate + 1
        };
        if !in_use(candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text() {
        let j = JobId::new("qpadev0001", "qsecofr", 42).unwrap();
        assert_eq!(j.to_string(), "000042/QSECOFR/QPADEV0001");
        assert_eq!(JobId::parse("000042/QSECOFR/QPADEV0001"), Some(j));
        assert_eq!(JobId::parse("0/QSECOFR/X"), None);
        assert_eq!(JobId::parse("1/QSECOFR"), None);
    }

    #[test]
    fn internal() {
        let j = JobId::new("QSVCJOB", "QSYS", 123_456).unwrap();
        let i = j.internal(1_700_000_000);
        assert_eq!(i.number(), Some(123_456));
        let mut bad = i;
        bad.0[3] ^= 1;
        assert_eq!(bad.number(), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(next_number(5, |_| false), Some(6));
        assert_eq!(next_number(MAX_NUMBER, |_| false), Some(1));
        assert_eq!(next_number(5, |n| n < 9), Some(9));
        assert_eq!(next_number(5, |_| true), None);
    }
}
