// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSIEVT`: the event log every session shares, kept so a server
//! started again (the next IPL, or `STRTCPSVR` after `ENDTCPSVR`) takes
//! it up and numbers on (`qsisess::EventLog::resume`), with what it last
//! said of each thing ([`Said`]), so what changed while it was away is
//! pushed when it starts.
//!
//! ```text
//! 0   magic "QS400QSE"   8
//! 8   version u16        2   = 1
//! 10  Stored                 ipc wire: next u64, events, said
//! ```

use ipc::wire_struct;
use qsisess::{Event, EventLog, Limits};

use super::{StoreError, body, file};

/// The file's name under [`super::DIR`].
pub const NAME: &str = "QSIEVT";
/// Its magic.
pub const MAGIC: &[u8; 8] = b"QS400QSE";
/// Its format version.
pub const FORMAT_VERSION: u16 = 1;

/// What the log last said of a problem.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaidProblem {
    /// Its number.
    pub number: u64,
    /// Whether it was closed.
    pub closed: bool,
    /// How many notes it had.
    pub notes: u32,
}
wire_struct!(SaidProblem {
    number,
    closed,
    notes
});

/// What the log last said of each thing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Said {
    /// Whether anything was said yet: a server never started before
    /// takes its first look as said, but for the IPL's codes.
    pub started: bool,
    /// The last reference code's number.
    pub refcode: u64,
    /// The state, as [`crate::pump::state_key`] has it.
    pub state: String,
    /// Each problem.
    pub problems: Vec<SaidProblem>,
    /// Each dump's name.
    pub dumps: Vec<String>,
}
wire_struct!(Said {
    started,
    refcode,
    state,
    problems,
    dumps
});

/// The file's content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored {
    /// The next event's number.
    pub next: u64,
    /// The events kept, oldest first, numbered up to `next - 1`.
    pub events: Vec<Event>,
    /// What the log last said.
    pub said: Said,
}
wire_struct!(Stored { next, events, said });

impl Stored {
    /// What `log` holds now, with `said`.
    #[must_use]
    pub fn of(log: &EventLog, said: &Said) -> Self {
        Self {
            next: log.next(),
            events: log.from(log.first()).cloned().collect(),
            said: said.clone(),
        }
    }

    /// The log it takes up again: its events numbered on, or, when they
    /// do not make a run up to `next`, the last run that does; with
    /// nothing kept, a log from 1.
    #[must_use]
    pub fn log(&self, limits: &Limits) -> EventLog {
        if self.next == 0 {
            return EventLog::new(limits);
        }
        if let Ok(l) = EventLog::resume(limits, self.next, self.events.iter().cloned()) {
            return l;
        }
        // The last run of numbers that ends before `next`.
        let mut run: Vec<Event> = Vec::new();
        for e in self
            .events
            .iter()
            .filter(|e| e.seq > 0 && e.seq < self.next)
        {
            if run.last().is_some_and(|l| l.seq + 1 != e.seq) {
                run.clear();
            }
            run.push(e.clone());
        }
        if run.last().is_some_and(|l| l.seq + 1 != self.next) {
            run.clear();
        }
        EventLog::resume(limits, self.next, run).unwrap_or_else(|_| EventLog::new(limits))
    }
}

/// The file's bytes.
#[must_use]
pub fn encode(s: &Stored) -> Vec<u8> {
    file(*MAGIC, FORMAT_VERSION, &ipc::encode(s))
}

/// What a file holds.
///
/// # Errors
/// Not the file, another version, or a body that does not read.
pub fn decode(bytes: &[u8]) -> Result<Stored, StoreError> {
    let b = body(bytes, *MAGIC, FORMAT_VERSION)?;
    ipc::decode(b).map_err(|e| StoreError::Body(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_is_kept_and_taken_up_again() {
        let limits = Limits::default();
        let mut log = EventLog::new(&limits);
        for i in 0..3u8 {
            log.push(1, 1, vec![i]).unwrap();
        }
        let said = Said {
            started: true,
            refcode: 9,
            state: "running".into(),
            problems: vec![SaidProblem {
                number: 1,
                closed: false,
                notes: 2,
            }],
            dumps: vec!["QSD0000001".into()],
        };
        let s = Stored::of(&log, &said);
        let back = decode(&encode(&s)).unwrap();
        assert_eq!(back, s);
        let again = back.log(&limits);
        assert_eq!(again.next(), 4);
        assert_eq!(again.len(), 3);
        // A gap keeps the last run.
        let mut gap = s.clone();
        gap.events.remove(0);
        gap.events[0].seq = 1;
        let l = gap.log(&limits);
        assert_eq!(l.next(), 4);
        assert_eq!(l.len(), 1);
        assert_eq!(Stored::default().log(&limits).next(), 1);
        assert_eq!(decode(b"QS400QSC\0\x01"), Err(StoreError::Magic));
    }
}
