// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The event log: the managed side's caller owns it, it outlives
//! sessions, and a console that reconnects is given what it missed from
//! it. Bounded by count and by bytes; the oldest go first.

use std::collections::VecDeque;

use qsi::Message;
use qsi::any::AnyEvent;

use crate::{Error, Event, Limits};

/// An event's envelope bytes beyond its body.
const ENVELOPE: usize = 8 + 2 + 2 + 4;

/// The events pushed, newest last.
#[derive(Debug)]
pub struct EventLog {
    entries: VecDeque<Event>,
    bytes: usize,
    max_events: usize,
    max_bytes: usize,
    max_message: usize,
    next: u64,
}

impl EventLog {
    /// An empty log; the first event is 1.
    #[must_use]
    pub fn new(limits: &Limits) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            max_events: limits.log_events.max(1),
            max_bytes: limits.log_bytes,
            max_message: limits.message(),
            next: 1,
        }
    }

    /// Push an event; its number.
    ///
    /// # Errors
    /// The event is longer than a message may be.
    pub fn push(&mut self, family: u16, op: u16, body: Vec<u8>) -> Result<u64, Error> {
        let len = ENVELOPE + body.len();
        if len > self.max_message {
            return Err(Error::TooLong {
                len,
                limit: self.max_message,
            });
        }
        let seq = self.next;
        self.next += 1;
        self.bytes += body.len();
        self.entries.push_back(Event {
            seq,
            family,
            op,
            body,
        });
        // The newest stays even when it alone is over the bytes.
        while self.entries.len() > 1
            && (self.entries.len() > self.max_events || self.bytes > self.max_bytes)
        {
            if let Some(old) = self.entries.pop_front() {
                self.bytes -= old.body.len();
            }
        }
        Ok(seq)
    }

    /// Push a typed event.
    ///
    /// # Errors
    /// As [`Self::push`].
    pub fn push_msg<M: Message>(&mut self, m: &M) -> Result<u64, Error> {
        self.push(M::FAMILY, m.op(), m.encode())
    }

    /// Push any family's event.
    ///
    /// # Errors
    /// As [`Self::push`].
    pub fn push_any(&mut self, e: &AnyEvent) -> Result<u64, Error> {
        self.push(e.family(), e.op(), e.encode())
    }

    /// The number the next event will have.
    #[must_use]
    pub fn next(&self) -> u64 {
        self.next
    }

    /// The oldest event's number still kept; [`Self::next`] when empty.
    #[must_use]
    pub fn first(&self) -> u64 {
        self.entries.front().map_or(self.next, |e| e.seq)
    }

    /// Events kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether none is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Bytes of bodies kept.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The events from `seq` on, oldest first.
    pub fn from(&self, seq: u64) -> impl Iterator<Item = &Event> {
        let skip = usize::try_from(seq.saturating_sub(self.first())).unwrap_or(usize::MAX);
        self.entries.iter().skip(skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_by_count_and_bytes() {
        let limits = Limits {
            log_events: 3,
            log_bytes: 10,
            ..Limits::default()
        };
        let mut log = EventLog::new(&limits);
        assert_eq!((log.first(), log.next()), (1, 1));
        for i in 1..=5 {
            assert_eq!(log.push(1, 1, vec![0; 2]).unwrap(), i);
        }
        assert_eq!((log.first(), log.next(), log.len()), (3, 6, 3));
        assert_eq!(log.from(4).map(|e| e.seq).collect::<Vec<_>>(), vec![4, 5]);
        assert_eq!(log.from(1).count(), 3);
        log.push(1, 1, vec![0; 9]).unwrap();
        assert_eq!((log.first(), log.bytes()), (6, 9));
        log.push(1, 1, vec![0; 20]).unwrap();
        assert_eq!((log.first(), log.len()), (7, 1));
        assert!(matches!(
            log.push(1, 1, vec![0; limits.max_message]),
            Err(Error::TooLong { .. })
        ));
    }
}
