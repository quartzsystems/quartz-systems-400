// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Two sessions wired together in memory, for tests and the fuzzers:
//! [`Loopback::pump`] moves bytes both ways in pieces of a chosen size
//! until neither has any to send, collecting what each hands up.

use qsi::any::{AnyReply, AnyRequest};
use qsi::{Hello, PeerKind, caps};

use crate::{Error, Incoming, Limits, Role, Session};

/// What the test hellos say: every capability bit `qsi` has.
pub const CAPS: u64 = caps::HOST_V1
    | caps::SERVICING
    | caps::PHYSICAL_IO
    | caps::PARTITION_V2
    | caps::SWITCHES
    | caps::POOLS
    | caps::PARTITION_V3
    | caps::ACTIVATIONS
    | caps::POWER_ON
    | caps::PANEL_FUNCTIONS
    | caps::PROBLEM_NOTES
    | caps::DUMP_SPACE
    | caps::FIX_LEVEL
    | caps::RESOURCES;

/// A console's hello for tests.
#[must_use]
pub fn console_hello() -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: CAPS,
        kind: PeerKind::Console,
        name: "MC01".into(),
        release: "V1R1M0".into(),
        serial: String::new(),
    }
}

/// A virtual server host's hello for tests.
#[must_use]
pub fn host_hello() -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: CAPS,
        kind: PeerKind::Host,
        name: "VS01".into(),
        release: "V1R2M0".into(),
        serial: "S0000001".into(),
    }
}

/// A console session and a managed session, connected.
#[derive(Debug)]
pub struct Loopback {
    /// The console.
    pub console: Session,
    /// The managed side.
    pub managed: Session,
    /// The clock both are fed.
    pub now: u64,
    /// The size of the pieces bytes move in.
    pub piece: usize,
    /// What the console handed up, oldest first.
    pub at_console: Vec<Incoming>,
    /// What the managed side handed up, oldest first.
    pub at_managed: Vec<Incoming>,
}

impl Loopback {
    /// Two sessions with the test hellos and default limits, the
    /// managed side's peer pinned or not, the hellos moved.
    #[must_use]
    pub fn new(pinned: bool) -> Self {
        Self::with(
            console_hello(),
            host_hello(),
            pinned,
            Limits::default(),
            Limits::default(),
        )
        .expect("the test hellos are good")
    }

    /// Two sessions of these hellos and limits, the hellos moved.
    ///
    /// # Errors
    /// A hello is out of bounds or not its role's.
    pub fn with(
        console: Hello,
        managed: Hello,
        pinned: bool,
        console_limits: Limits,
        managed_limits: Limits,
    ) -> Result<Self, Error> {
        let mut l = Self {
            console: Session::new(Role::Console, console, true, console_limits, 0)?,
            managed: Session::new(Role::Managed, managed, pinned, managed_limits, 0)?,
            now: 0,
            piece: 7,
            at_console: Vec::new(),
            at_managed: Vec::new(),
        };
        l.pump();
        Ok(l)
    }

    /// Move bytes both ways until neither side has any to send.
    pub fn pump(&mut self) {
        loop {
            let to_managed = self.console.take_output();
            let to_console = self.managed.take_output();
            if to_managed.is_empty() && to_console.is_empty() {
                break;
            }
            let piece = self.piece.max(1);
            for chunk in to_managed.chunks(piece) {
                self.managed.feed(chunk, self.now);
            }
            for chunk in to_console.chunks(piece) {
                self.console.feed(chunk, self.now);
            }
            self.collect();
        }
        self.collect();
    }

    /// Advance the clock and tick both.
    pub fn advance(&mut self, ms: u64) {
        self.now += ms;
        self.console.tick(self.now);
        self.managed.tick(self.now);
        self.collect();
    }

    fn collect(&mut self) {
        while let Some(i) = self.console.poll() {
            self.at_console.push(i);
        }
        while let Some(i) = self.managed.poll() {
            self.at_managed.push(i);
        }
    }

    /// Send `req` from the console, answer it with `reply` on the
    /// managed side, and give back what each side decoded: the request
    /// as the managed side read it and the reply as the console did.
    ///
    /// # Panics
    /// When any step does not happen as it should.
    pub fn round_trip(&mut self, req: &AnyRequest, reply: &AnyReply) -> (AnyRequest, AnyReply) {
        let id = self.console.request_any("TESTUSER", req).expect("request");
        self.pump();
        let got = self.take_request(id).expect("the request arrives");
        assert_eq!(got.user, "TESTUSER");
        let got_req = got.decode().expect("reads").expect("known");
        self.managed.reply_any(id, reply).expect("reply");
        self.pump();
        let r = self.take_reply(id).expect("the reply arrives");
        let got_reply = r.decode().expect("reads").expect("known");
        (got_req, got_reply)
    }

    /// The request `id` the managed side handed up, taken.
    pub fn take_request(&mut self, id: u32) -> Option<crate::Request> {
        let at = self
            .at_managed
            .iter()
            .position(|i| matches!(i, Incoming::Request(r) if r.id == id))?;
        match self.at_managed.remove(at) {
            Incoming::Request(r) => Some(r),
            _ => None,
        }
    }

    /// The reply to `id` the console handed up, taken.
    pub fn take_reply(&mut self, id: u32) -> Option<crate::Reply> {
        let at = self
            .at_console
            .iter()
            .position(|i| matches!(i, Incoming::Reply(r) if r.id == id))?;
        match self.at_console.remove(at) {
            Incoming::Reply(r) => Some(r),
            _ => None,
        }
    }
}
