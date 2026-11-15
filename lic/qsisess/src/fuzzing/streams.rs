// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stream state machine: bytes as a script of operations on two
//! connected sessions. Each operation is a byte and its arguments:
//!
//! | `op % 10` | Operation | Arguments |
//! |---|---|---|
//! | 0 | open a stream (a request answered with one) | |
//! | 1 | send | side, stream, length / 64 |
//! | 2 | grant | side, stream, amount / 64 |
//! | 3 | close a direction | side, stream |
//! | 4 | reset | side, stream |
//! | 5 | a request answered plainly | |
//! | 6 | deliver part of the bytes queued | side, how many / 16 |
//! | 7 | deliver everything | |
//! | 8 | advance the clock | seconds / 4 |
//! | 9 | inject a raw stream frame | side, kind, length, body |

use qsi::base;

use super::drain;
use crate::loopback::Loopback;
use crate::wire::{frame, kind};
use crate::{Incoming, Role, Session, StreamId};

/// The most bytes a script may leave queued between the sessions.
const MAX_QUEUED_BYTES: usize = 64 << 20;

struct Script<'a> {
    data: &'a [u8],
    at: usize,
}

impl Script<'_> {
    fn byte(&mut self) -> u8 {
        let b = self.data.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        b
    }

    fn done(&self) -> bool {
        self.at >= self.data.len()
    }
}

struct Run {
    l: Loopback,
    /// Bytes queued from the console and from the managed side.
    to_managed: Vec<u8>,
    to_console: Vec<u8>,
    /// Streams each side has seen open.
    streams: Vec<StreamId>,
}

impl Run {
    fn side(&mut self, b: u8) -> &mut Session {
        if b & 1 == 0 {
            &mut self.l.console
        } else {
            &mut self.l.managed
        }
    }

    fn stream(&self, b: u8) -> StreamId {
        if self.streams.is_empty() {
            return u32::from(b);
        }
        self.streams[usize::from(b) % self.streams.len()]
    }

    /// Queue what each side has to send, then hand up what came.
    fn settle(&mut self) {
        self.to_managed.extend(self.l.console.take_output());
        self.to_console.extend(self.l.managed.take_output());
        assert!(self.to_managed.len() + self.to_console.len() < MAX_QUEUED_BYTES);
        let mut opened = Vec::new();
        drain(&mut self.l.console, |s, i| match i {
            Incoming::Reply(r) => {
                if let Some(st) = r.stream {
                    opened.push(st);
                }
            }
            Incoming::Writable { stream, credit } => {
                // Nothing is sent while draining: the credit only grows,
                // unless a later item reset the stream.
                assert!(s.credit(stream).is_none_or(|c| c >= credit));
            }
            _ => {}
        });
        drain(&mut self.l.managed, |s, i| {
            if let Incoming::Request(r) = i {
                let st = if r.id % 2 == 0 {
                    s.reply_stream(r.id, Vec::new()).ok()
                } else {
                    s.reply_msg(
                        r.id,
                        &base::Reply::Time(base::Time {
                            unix_ns: 1,
                            offset_s: 0,
                            zone: String::new(),
                        }),
                    )
                    .ok();
                    None
                };
                opened.extend(st);
            }
        });
        for st in opened {
            if !self.streams.contains(&st) {
                self.streams.push(st);
            }
        }
        self.to_managed.extend(self.l.console.take_output());
        self.to_console.extend(self.l.managed.take_output());
    }

    fn deliver(&mut self, to: u8, n: usize) {
        let now = self.l.now;
        let (q, s) = if to & 1 == 0 {
            (&mut self.to_console, &mut self.l.console)
        } else {
            (&mut self.to_managed, &mut self.l.managed)
        };
        let n = n.min(q.len());
        let bytes: Vec<u8> = q.drain(..n).collect();
        s.feed(&bytes, now);
    }

    fn request(&mut self) {
        let _ = self
            .l
            .console
            .request_msg("FUZZ", &base::Request::Time(qsi::Empty));
    }
}

/// One script.
pub fn streams(data: &[u8]) {
    let mut sc = Script { data, at: 0 };
    let mut run = Run {
        l: Loopback::new(true),
        to_managed: Vec::new(),
        to_console: Vec::new(),
        streams: Vec::new(),
    };
    run.l.piece = 13;
    let mut steps = 0;
    while !sc.done() && steps < 4096 {
        steps += 1;
        match sc.byte() % 10 {
            0 => {
                // An even request number gets a stream.
                run.request();
                run.request();
                run.settle();
                run.deliver(1, usize::MAX);
                run.settle();
            }
            1 => {
                let (side, st, len) = (sc.byte(), sc.byte(), usize::from(sc.byte()) * 64);
                let st = run.stream(st);
                let s = run.side(side);
                let before = s.credit(st);
                if let Ok(n) = s.send(st, &vec![0x5A; len]) {
                    let before = before.unwrap_or(0);
                    assert!(n as u64 <= before && s.credit(st) == Some(before - n as u64));
                }
            }
            2 => {
                let (side, st, n) = (sc.byte(), sc.byte(), u32::from(sc.byte()) * 64);
                let st = run.stream(st);
                let _ = run.side(side).grant(st, n);
            }
            3 => {
                let (side, st) = (sc.byte(), sc.byte());
                let st = run.stream(st);
                let _ = run.side(side).close_stream(st);
            }
            4 => {
                let (side, st) = (sc.byte(), sc.byte());
                let st = run.stream(st);
                let _ = run.side(side).reset_stream(st, "QSI9999", "fuzz");
            }
            5 => run.request(),
            6 => {
                let (to, n) = (sc.byte(), usize::from(sc.byte()) * 16);
                run.deliver(to, n);
            }
            7 => {
                run.deliver(0, usize::MAX);
                run.deliver(1, usize::MAX);
            }
            8 => {
                let ms = u64::from(sc.byte()) * 250;
                run.l.now += ms;
                let now = run.l.now;
                run.l.console.tick(now);
                run.l.managed.tick(now);
            }
            _ => {
                let side = sc.byte();
                let k = kind::DATA + u16::from(sc.byte() % 4);
                let len = usize::from(sc.byte() % 16);
                let body: Vec<u8> = (0..len).map(|_| sc.byte()).collect();
                let f = frame(k, &body).unwrap_or_default();
                let now = run.l.now;
                run.side(side).feed(&f, now);
            }
        }
        run.settle();
    }
    // Everything delivered, the two settle and stay consistent.
    for _ in 0..8 {
        run.deliver(0, usize::MAX);
        run.deliver(1, usize::MAX);
        run.settle();
    }
    for s in [&run.l.console, &run.l.managed] {
        assert!(s.open_streams() <= s.limits().max_streams);
        assert!(s.role() == Role::Console || s.outstanding() <= s.limits().max_requests);
    }
}
