// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The events: what the server sees of the system each turn compared
//! with what the log last said ([`Said`]), and what differs pushed. A
//! code posted is base `Refcode`, a change of state base `State`, a
//! problem opened, closed or noted the service family's events, a dump
//! taken or removed base `DumpTaken` and `DumpRemoved`.
//!
//! The first time a server ever starts it pushes the IPL's codes and the
//! state, and takes the problems and dumps there as said.

use qsi::any::AnyEvent;
use qsi::base::{self, Refcode, State, SystemDump};
use qsi::service::{self, Line, Noted, Noticed, ProblemStatus, Summary};
use qsisess::EventLog;

use crate::store::events::{Said, SaidProblem};

/// A problem as the server sees it: its summary and its notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeenProblem {
    /// The summary.
    pub summary: Summary,
    /// Its notes, oldest first.
    pub notes: Vec<Line>,
}

/// What the server saw this turn; `None` for what it could not look at.
#[derive(Clone, Copy, Debug, Default)]
pub struct Seen<'a> {
    /// This IPL's codes, oldest first.
    pub codes: Option<&'a [Refcode]>,
    /// The state.
    pub state: Option<&'a State>,
    /// The problems.
    pub problems: Option<&'a [SeenProblem]>,
    /// The dumps.
    pub dumps: Option<&'a [SystemDump]>,
}

/// The prefix of a note a console wrote, before its user.
pub const NOTE_PREFIX: &str = "QSI ";

/// A note's text as the problem log keeps it: who wrote it, from a
/// management console.
#[must_use]
pub fn note_text(user: &str, text: &str) -> String {
    format!("{NOTE_PREFIX}{user}: {text}")
}

/// Who wrote a note: the user of a console's note, else empty.
#[must_use]
pub fn note_by(text: &str) -> String {
    text.strip_prefix(NOTE_PREFIX)
        .and_then(|t| t.split_once(": "))
        .map(|(u, _)| u.to_string())
        .unwrap_or_default()
}

/// The state as the log compares it.
#[must_use]
pub fn state_key(s: &State) -> String {
    format!(
        "{:?} {} {:?} {}",
        s.power,
        s.refcode.as_ref().map_or("", |c| c.code.as_str()),
        s.keylock,
        s.slot
    )
}

fn noticed(p: &SeenProblem, now: u64) -> Noticed {
    Noticed {
        problem: p.summary.clone(),
        time: now,
    }
}

/// Push what differs between `seen` and `said` into `log`, and say it.
/// The number of events pushed.
pub fn pump(log: &mut EventLog, said: &mut Said, seen: &Seen<'_>, now: u64) -> usize {
    let first = !said.started;
    let mut events = Vec::new();
    if let Some(codes) = seen.codes {
        let last = said.refcode;
        for c in codes.iter().filter(|c| c.seq > last) {
            events.push(AnyEvent::Base(base::Event::Refcode(c.clone())));
            said.refcode = c.seq;
        }
    }
    if let Some(s) = seen.state {
        let key = state_key(s);
        if key != said.state {
            events.push(AnyEvent::Base(base::Event::State(s.clone())));
            said.state = key;
        }
    }
    if let Some(problems) = seen.problems {
        problem_events(&mut events, said, problems, first, now);
    }
    if let Some(dumps) = seen.dumps {
        if !first {
            dump_events(&mut events, said, dumps, now);
        }
        said.dumps = dumps.iter().map(|d| d.name.clone()).collect();
    }
    said.started = true;
    events.iter().filter(|e| log.push_any(e).is_ok()).count()
}

fn problem_events(
    events: &mut Vec<AnyEvent>,
    said: &mut Said,
    problems: &[SeenProblem],
    first: bool,
    now: u64,
) {
    let ev = AnyEvent::Service;
    let mut next = Vec::with_capacity(problems.len());
    for p in problems {
        let closed = p.summary.status == ProblemStatus::Closed;
        let notes = u32::try_from(p.notes.len()).unwrap_or(u32::MAX);
        let before = said.problems.iter().find(|s| s.number == p.summary.number);
        match before {
            _ if first => {}
            None => {
                events.push(ev(service::Event::Opened(noticed(p, now))));
                if closed {
                    events.push(ev(service::Event::Closed(noticed(p, now))));
                }
            }
            Some(b) => {
                let had = usize::try_from(b.notes).unwrap_or(0);
                for line in p.notes.iter().skip(had) {
                    events.push(ev(service::Event::Noted(Noted {
                        number: p.summary.number,
                        line: line.clone(),
                        by: note_by(&line.text),
                        time: now,
                    })));
                }
                if closed && !b.closed {
                    events.push(ev(service::Event::Closed(noticed(p, now))));
                }
            }
        }
        next.push(SaidProblem {
            number: p.summary.number,
            closed,
            notes,
        });
    }
    said.problems = next;
}

fn dump_events(events: &mut Vec<AnyEvent>, said: &Said, dumps: &[SystemDump], now: u64) {
    for d in dumps.iter().filter(|d| !said.dumps.contains(&d.name)) {
        events.push(AnyEvent::Base(base::Event::DumpTaken(base::DumpTaken {
            name: d.name.clone(),
            dump: Some(d.clone()),
            error: None,
            time: now,
        })));
    }
    for gone in said
        .dumps
        .iter()
        .filter(|n| !dumps.iter().any(|d| &d.name == *n))
    {
        events.push(AnyEvent::Base(base::Event::DumpRemoved(
            base::DumpRemoved {
                name: gone.clone(),
                time: now,
            },
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Host;
    use crate::mem::MemHost;
    use qsisess::Limits;

    #[test]
    fn what_changed_is_pushed_once() {
        let mut h = MemHost::new();
        let mut log = EventLog::new(&Limits::default());
        let mut said = Said::default();
        let state = h.state().unwrap();
        let problems: Vec<SeenProblem> = h
            .problems
            .iter()
            .map(|(s, n)| SeenProblem {
                summary: s.clone(),
                notes: n.clone(),
            })
            .collect();
        let seen = Seen {
            codes: Some(&h.codes),
            state: Some(&state),
            problems: Some(&problems),
            dumps: Some(&[]),
        };
        // The first look: the IPL's three codes and the state.
        assert_eq!(pump(&mut log, &mut said, &seen, 1), 4);
        assert_eq!(pump(&mut log, &mut said, &seen, 2), 0);
        // A note, a close, a new problem, a dump, a code.
        let mut problems = problems;
        problems[0].notes.push(Line {
            time: 3,
            text: note_text("OPER", "looked"),
        });
        problems[0].summary.status = ProblemStatus::Closed;
        let mut p2 = problems[0].clone();
        p2.summary.number += 1;
        p2.summary.status = ProblemStatus::Open;
        problems.push(p2);
        h.take_dump("t", "u").unwrap();
        h.codes.push(Refcode {
            seq: 4,
            code: "D1001000".into(),
            words: Vec::new(),
            text: String::new(),
            time_ms: 0,
        });
        let state = h.state().unwrap();
        let seen = Seen {
            codes: Some(&h.codes),
            state: Some(&state),
            problems: Some(&problems),
            dumps: Some(&h.dumps),
        };
        // Code, state, noted, closed, opened, dump taken.
        assert_eq!(pump(&mut log, &mut said, &seen, 3), 6);
        assert_eq!(pump(&mut log, &mut said, &seen, 4), 0);
        assert_eq!(note_by(&note_text("OPER", "x: y")), "OPER");
        assert_eq!(note_by("plain"), "");
        let none = Seen {
            dumps: Some(&[]),
            ..Seen::default()
        };
        assert_eq!(pump(&mut log, &mut said, &none, 5), 1);
    }
}
