// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the client does with an offset (`CHGNTPA`): below `MINADJ` the
//! clock is left alone; up to `ADJTHLD` it is slewed (adjusted
//! gradually, as IBM's `QWCADJTM` does); beyond it, up to `MAXADJ`, it
//! is set at once; beyond `MAXADJ` it is not touched and the operator
//! is told. `ADJTHLD(*MAXADJ)` means never set at once.

/// The client's adjustment limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// `MINADJ`, nanoseconds; 0 for `*NONE`.
    pub min: i64,
    /// `MAXADJ`, nanoseconds.
    pub max: i64,
    /// `ADJTHLD`, nanoseconds; `None` for `*MAXADJ`.
    pub threshold: Option<i64>,
}

/// What to do with one offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Within `MINADJ`: nothing.
    Keep,
    /// Slew by this many nanoseconds.
    Slew(i64),
    /// Step by this many nanoseconds.
    Step(i64),
    /// Beyond `MAXADJ`: nothing, and a message.
    TooFar(i64),
}

impl Policy {
    /// From the command's units: `MINADJ` in milliseconds (`None` for
    /// `*NONE`), `MAXADJ` in minutes, `ADJTHLD` in seconds (`None` for
    /// `*MAXADJ`).
    #[must_use]
    pub fn of(minadj_ms: Option<u32>, maxadj_min: u32, adjthld_s: Option<u32>) -> Self {
        Self {
            min: i64::from(minadj_ms.unwrap_or(0)) * 1_000_000,
            max: i64::from(maxadj_min) * 60_000_000_000,
            threshold: adjthld_s.map(|s| i64::from(s) * 1_000_000_000),
        }
    }

    /// The decision for `offset`.
    #[must_use]
    pub fn decide(&self, offset: i64) -> Decision {
        let size = offset.unsigned_abs();
        if size > self.max.unsigned_abs() {
            return Decision::TooFar(offset);
        }
        if size < self.min.unsigned_abs() || offset == 0 {
            return Decision::Keep;
        }
        match self.threshold {
            Some(t) if size > t.unsigned_abs() => Decision::Step(offset),
            _ => Decision::Slew(offset),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 1_000_000;
    const S: i64 = 1_000 * MS;

    #[test]
    fn the_four_outcomes() {
        // MINADJ 20 ms, MAXADJ 20 min, ADJTHLD 60 s.
        let p = Policy::of(Some(20), 20, Some(60));
        assert_eq!(p.decide(5 * MS), Decision::Keep);
        assert_eq!(p.decide(-19 * MS), Decision::Keep);
        assert_eq!(p.decide(25 * MS), Decision::Slew(25 * MS));
        assert_eq!(p.decide(-60 * S), Decision::Slew(-60 * S));
        assert_eq!(p.decide(61 * S), Decision::Step(61 * S));
        assert_eq!(p.decide(-20 * 60 * S), Decision::Step(-20 * 60 * S));
        assert_eq!(p.decide(20 * 60 * S + 1), Decision::TooFar(20 * 60 * S + 1));
    }

    #[test]
    fn special_values() {
        // MINADJ(*NONE) adjusts anything; ADJTHLD(*MAXADJ) never steps.
        let p = Policy::of(None, 1, None);
        assert_eq!(p.decide(1), Decision::Slew(1));
        assert_eq!(p.decide(0), Decision::Keep);
        assert_eq!(p.decide(59 * S), Decision::Slew(59 * S));
        assert_eq!(p.decide(61 * S), Decision::TooFar(61 * S));
    }
}
