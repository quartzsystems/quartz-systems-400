// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security attributes `CHGSECA` changes and `DSPSECA` shows that
//! are not system values: where generated user and group ID numbers
//! start. They live in `QUSRSYS/QSYSECA` ([`crate::textspace`]), lines
//! `UID n` and `GID n` after a `QSYSECA 1` header. A generated number is
//! the smallest free one from the start set, or one past the highest in
//! use when none was set.

use std::fmt::Write as _;

use store::obj::ObjectId;
use store::txn::Txn;

use crate::error::{Result, SecError};

const SPACE: (&str, &str) = ("QSYSECA", "Security attributes");
const HEADER: &str = "QSYSECA 1";

/// The lowest and highest user and group ID numbers `CHGSECA` takes.
pub const RANGE: (u32, u32) = (101, 4_294_967_294);

/// Where generated numbers start; `None` where `CHGSECA` set nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Starts {
    /// User ID numbers.
    pub uid: Option<u32>,
    /// Group ID numbers.
    pub gid: Option<u32>,
}

/// The starts in the store.
pub fn load(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Starts> {
    let mut s = Starts::default();
    if let Some(t) = crate::textspace::load(txn, qsys, SPACE.0, HEADER)? {
        for line in t.lines() {
            let mut w = line.split_whitespace();
            match (w.next(), w.next().and_then(|n| n.parse().ok())) {
                (Some("UID"), Some(n)) => s.uid = Some(n),
                (Some("GID"), Some(n)) => s.gid = Some(n),
                _ => {}
            }
        }
    }
    Ok(s)
}

/// `CHGSECA UID() GID()`: `None` keeps a value (`*SAME`).
pub fn change(txn: &mut Txn<'_>, qsys: ObjectId, uid: Option<u32>, gid: Option<u32>) -> Result<Starts> {
    for n in [uid, gid].into_iter().flatten() {
        if !(RANGE.0..=RANGE.1).contains(&n) {
            return Err(SecError::msg("CPF3C3C", format!("{n} is not from 101 to 4294967294.")));
        }
    }
    let mut s = load(txn, qsys)?;
    if uid.is_some() {
        s.uid = uid;
    }
    if gid.is_some() {
        s.gid = gid;
    }
    let mut body = String::new();
    if let Some(n) = s.uid {
        let _ = writeln!(body, "UID {n}");
    }
    if let Some(n) = s.gid {
        let _ = writeln!(body, "GID {n}");
    }
    crate::textspace::save(txn, qsys, SPACE, HEADER, &body, crate::textspace::now())?;
    Ok(s)
}

/// The next number to generate, given the numbers `used` and the start.
#[must_use]
pub fn next(used: &[u32], start: Option<u32>, first: u32) -> u32 {
    match start {
        Some(from) => {
            let mut n = from;
            while used.contains(&n) {
                n += 1;
            }
            n
        }
        None => used.iter().copied().max().map_or(first, |m| (m + 1).max(first)),
    }
}

#[cfg(test)]
mod tests {
    use super::next;

    #[test]
    fn a_start_skips_numbers_in_use() {
        assert_eq!(next(&[101, 102, 108], None, 101), 109);
        assert_eq!(next(&[101, 102, 108], Some(102), 101), 103);
        assert_eq!(next(&[], Some(500), 101), 500);
        assert_eq!(next(&[], None, 101), 101);
    }
}
