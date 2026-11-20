// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Which codes still lack a written cause or recovery.

use crate::model::{Family, Src};
use crate::table::TABLE;

/// One family's counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gaps {
    /// The family.
    pub family: Family,
    /// Its codes.
    pub codes: usize,
    /// Codes without a cause.
    pub without_cause: usize,
    /// Codes without a recovery.
    pub without_recovery: usize,
}

/// The counts for every family, in [`Family::ALL`] order.
#[must_use]
pub fn gaps() -> [Gaps; 5] {
    Family::ALL.map(|family| {
        let mut g = Gaps {
            family,
            codes: 0,
            without_cause: 0,
            without_recovery: 0,
        };
        for s in TABLE.iter().filter(|s| s.family() == Some(family)) {
            g.codes += 1;
            g.without_cause += usize::from(s.cause.is_none());
            g.without_recovery += usize::from(s.recovery.is_none());
        }
        g
    })
}

/// Codes missing a cause or a recovery, in table order.
pub fn unexplained() -> impl Iterator<Item = &'static Src> {
    TABLE
        .iter()
        .filter(|s| s.cause.is_none() || s.recovery.is_none())
}
