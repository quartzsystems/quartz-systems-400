// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection's session: fed bytes and the clock, it produces
//! bytes to send and [`Incoming`] items.

mod close;
mod events;
mod hello;
mod recv;
mod requests;
mod streams;

use std::collections::{BTreeMap, VecDeque};

use qsi::{Agreed, Hello, PeerKind};
use qsipart::frame::Reader;

use crate::wire::parts::Assembler;
use crate::wire::{self, kind};
use crate::{Error, Incoming, Limits};

/// Which end a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The console: it asks, opens streams and takes events.
    Console,
    /// What is managed: it answers, allocates streams and pushes events.
    Managed,
}

/// A stream's number, given by the managed side, counting up from 1.
pub type StreamId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Waiting for the peer's hello.
    Waiting,
    /// Both hellos said.
    Open,
    /// Over.
    Closed,
}

/// One stream's state, both directions.
#[derive(Clone, Copy, Debug, Default)]
struct Stream {
    /// What this side may still send.
    send_credit: u64,
    /// What the peer may still send.
    recv_window: u64,
    /// This side sent its close.
    sent_closed: bool,
    /// The peer sent its close.
    recv_closed: bool,
    /// Bytes this side was given credit for and sent, in all.
    credit_total: u64,
    sent_total: u64,
    /// Bytes this side gave credit for and received, in all.
    granted_total: u64,
    received_total: u64,
}

/// One connection's session.
#[derive(Debug)]
pub struct Session {
    role: Role,
    ours: Hello,
    pinned: bool,
    limits: Limits,
    state: State,
    peer: Option<(Hello, Agreed)>,
    reader: Reader,
    parts: Assembler,
    out: Vec<u8>,
    incoming: VecDeque<Incoming>,
    /// Requests waiting for a reply: number to family and operation.
    pending: BTreeMap<u32, (u16, u16)>,
    next_id: u32,
    /// The managed side: the next event this session sends, when
    /// subscribed.
    cursor: Option<u64>,
    /// The console: the last event's number seen.
    last_event: Option<u64>,
    streams: BTreeMap<StreamId, Stream>,
    /// The highest stream number allocated (managed) or seen (console).
    max_stream: StreamId,
    last_rx: u64,
    last_ping: u64,
    ping_token: u64,
}

impl Session {
    /// A session for one connection, its hello already queued to send.
    /// `pinned` is whether the peer's key is pinned
    /// (`qsitrust::tls::peer`): on the managed side, an unpinned peer
    /// may only pair, for the session's whole life.
    ///
    /// # Errors
    /// The hello is out of bounds or not the role's kind.
    pub fn new(
        role: Role,
        hello: Hello,
        pinned: bool,
        limits: Limits,
        now_ms: u64,
    ) -> Result<Self, Error> {
        hello.check().map_err(Error::BadHello)?;
        if (hello.kind == PeerKind::Console) != (role == Role::Console) {
            return Err(Error::BadHello("its kind is not the role's"));
        }
        let mut s = Self {
            role,
            ours: hello,
            pinned,
            limits,
            state: State::Waiting,
            peer: None,
            reader: Reader::new(),
            parts: Assembler::default(),
            out: Vec::new(),
            incoming: VecDeque::new(),
            pending: BTreeMap::new(),
            next_id: 1,
            cursor: None,
            last_event: None,
            streams: BTreeMap::new(),
            max_stream: 0,
            last_rx: now_ms,
            last_ping: now_ms,
            ping_token: 0,
        };
        let body = ipc::encode(&s.ours);
        s.frame(kind::HELLO, &body);
        Ok(s)
    }

    /// The next thing for the caller.
    pub fn poll(&mut self) -> Option<Incoming> {
        self.incoming.pop_front()
    }

    /// The bytes to send, taken.
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.out)
    }

    /// Whether there are bytes to send.
    #[must_use]
    pub fn has_output(&self) -> bool {
        !self.out.is_empty()
    }

    /// This side's role.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// Whether the peer is pinned.
    #[must_use]
    pub fn pinned(&self) -> bool {
        self.pinned
    }

    /// The limits.
    #[must_use]
    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Whether both hellos are said and the session is not closed.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.state == State::Open
    }

    /// Whether the session is over.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.state == State::Closed
    }

    /// The peer's hello, once it came.
    #[must_use]
    pub fn peer(&self) -> Option<&Hello> {
        self.peer.as_ref().map(|p| &p.0)
    }

    /// What the two hellos agree on, once the peer's came.
    #[must_use]
    pub fn agreed(&self) -> Option<Agreed> {
        self.peer.as_ref().map(|p| p.1)
    }

    /// Requests waiting for a reply.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.pending.len()
    }

    /// Streams open.
    #[must_use]
    pub fn open_streams(&self) -> usize {
        self.streams.len()
    }

    /// The console: the last event's number seen, to subscribe with
    /// after a reconnect.
    #[must_use]
    pub fn last_event(&self) -> Option<u64> {
        self.last_event
    }

    /// Whether the managed side's session takes events.
    #[must_use]
    pub fn subscribed(&self) -> bool {
        self.cursor.is_some()
    }

    fn frame(&mut self, kind_: u16, body: &[u8]) {
        // Every body the session makes is bounded below a frame.
        if let Some(f) = wire::frame(kind_, body) {
            self.out.extend(f);
        }
    }

    fn ready(&self) -> Result<Agreed, Error> {
        match self.state {
            State::Waiting => Err(Error::NotReady),
            State::Closed => Err(Error::Closed),
            State::Open => self.agreed().ok_or(Error::NotReady),
        }
    }

    /// What must always hold, for the fuzzers.
    pub(crate) fn invariants(&self) -> Result<(), &'static str> {
        if self.streams.len() > self.limits.max_streams {
            return Err("more streams than the limit");
        }
        if self.pending.len() > self.limits.max_requests {
            return Err("more requests than the limit");
        }
        if self.parts.held() > self.limits.message() {
            return Err("a message held longer than the limit");
        }
        for s in self.streams.values() {
            if s.sent_total > s.credit_total || s.received_total > s.granted_total {
                return Err("a stream beyond its credit");
            }
            if s.credit_total - s.sent_total != s.send_credit
                || s.granted_total - s.received_total != s.recv_window
            {
                return Err("a stream's credit miscounted");
            }
        }
        if self.is_closed() && (!self.pending.is_empty() || !self.streams.is_empty()) {
            return Err("a closed session holding requests or streams");
        }
        Ok(())
    }
}
