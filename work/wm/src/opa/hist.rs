// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The backup history (`DSPBCKSTS`), kept in `QUSRSYS/QEZBCKHST`: one
//! line per backup that ran, newest last, the oldest dropped past
//! [`KEEP`].

use std::fmt::Write as _;

/// The user space that holds the history.
pub const OBJECT: (&str, &str) = ("QUSRSYS", "QEZBCKHST");
/// Runs kept.
pub const KEEP: usize = 100;

/// One backup that ran.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    /// `*DAILY`, `*WEEKLY`, `*MONTHLY`, or the `GO SAVE` option.
    pub option: String,
    /// Started, Unix seconds.
    pub started: u64,
    /// Ended, Unix seconds.
    pub ended: u64,
    /// The device.
    pub dev: String,
    /// The tape set, when one was rotated.
    pub tapset: String,
    /// Whether every save completed.
    pub ok: bool,
    /// Each save and how it ended, `;`-separated.
    pub steps: String,
}

/// The history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    /// Oldest first.
    pub runs: Vec<Run>,
}

impl History {
    /// Decode: tab-separated fields, one run per line.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let runs = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                (f.len() >= 7).then(|| Run {
                    option: f[0].into(),
                    started: f[1].parse().unwrap_or(0),
                    ended: f[2].parse().unwrap_or(0),
                    dev: f[3].into(),
                    tapset: f[4].into(),
                    ok: f[5] == "1",
                    steps: f[6].into(),
                })
            })
            .collect();
        Self { runs }
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> String {
        let clean = |s: &str| s.replace(['\t', '\n'], " ");
        let mut out = String::new();
        for r in &self.runs {
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                clean(&r.option),
                r.started,
                r.ended,
                clean(&r.dev),
                clean(&r.tapset),
                u8::from(r.ok),
                clean(&r.steps)
            );
        }
        out
    }

    /// Add a run, dropping the oldest past [`KEEP`].
    pub fn add(&mut self, r: Run) {
        self.runs.push(r);
        let n = self.runs.len();
        if n > KEEP {
            self.runs.drain(..n - KEEP);
        }
    }

    /// The last run of `option`.
    #[must_use]
    pub fn last(&self, option: &str) -> Option<&Run> {
        self.runs.iter().rev().find(|r| r.option == option)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_persists_and_is_bounded() {
        let mut h = History::default();
        for i in 0..(KEEP as u64 + 5) {
            h.add(Run {
                option: if i % 2 == 0 { "*DAILY" } else { "*WEEKLY" }.into(),
                started: i,
                ended: i + 1,
                dev: "TAP01".into(),
                tapset: "DAYA".into(),
                ok: i != 3,
                steps: "SAVLIB\tok;SAV ok".into(),
            });
        }
        assert_eq!(h.runs.len(), KEEP);
        assert_eq!(h.runs[0].started, 5);
        let back = History::decode(&h.encode());
        assert_eq!(back.runs.len(), KEEP);
        assert_eq!(back.last("*WEEKLY").unwrap().started, KEEP as u64 + 3);
        assert_eq!(back.runs[0].steps, "SAVLIB ok;SAV ok");
    }
}
