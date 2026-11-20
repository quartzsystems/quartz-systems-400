// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzz targets: a console's connection to the server, in memory,
//! before authentication ([`pre_auth`]: a console not pinned, a pairing
//! password out) and after ([`post_auth`]: pinned, every family over a
//! system in memory). The input is a script: each step sends a request
//! through a console's session, injects raw bytes, moves the clock,
//! pairs with a right or wrong password, or pushes an event. Nothing may
//! panic, hang or grow without bound.

use std::sync::{Arc, Mutex};

use qsi::any::AnyRequest;
use qsi::host::model::SYSTEM_ID;
use qsi::host::{console, panel};
use qsi::{Hello, PeerKind, base, caps, pair};
use qsisess::{EventLog, Incoming, Limits, Role, Session};
use qsitrust::{Binding, Fingerprint, Peer};

use crate::conn::{Config, Conn};
use crate::mem::{MemHost, MemKeeper, MemOpener};

/// The password the fuzzed system has out.
const PASSWORD: &str = "7K3Q-9VNM-2HXD-PW4T";
/// The most steps an input runs.
const MAX_STEPS: usize = 256;
/// The most pairings an input runs (each is real cryptography).
const MAX_PAIRINGS: usize = 4;

/// Seeds: request scripts, a pairing, raw frames.
pub const SEEDS: &[&[u8]] = &[
    b"\x00\x00\x00\x01\x00\x02\x00\x03\x05\x10",
    b"\x06\x00\x05\x02\x06\x01\x05\x02",
    b"\x04\x0cQI\x10\x10\x15\x00\xfa\xef\x11\x00\x00\x00\x05\x01",
    b"\x01\x05\x01\x06\x02\x07\x03\x08\x07\x00\x05\x40",
];

/// Tokens worth inserting.
pub const DICT: &[&[u8]] = &[
    b"QI",
    b"\x01\x10",
    b"\x10\x10",
    b"\x13\x10",
    b"\x20\x10",
    b"\x21\x10",
    b"\x23\x10",
    b"\x01\x00",
    b"\x02\x00",
    b"\x03\x00",
    b"\x14\x00",
    b"\x17\x00",
    b"\xff\xff\xff\xff",
];

/// The system's hello.
#[must_use]
pub fn standalone_hello() -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: caps::STANDALONE,
        kind: PeerKind::Standalone,
        name: "QS400".into(),
        release: "V1R2M0".into(),
        serial: "S0000001".into(),
    }
}

/// A console's hello asking what a standalone system has.
#[must_use]
pub fn console_hello() -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: caps::STANDALONE,
        kind: PeerKind::Console,
        name: "MC01".into(),
        release: "V1R1M0".into(),
        serial: String::new(),
    }
}

/// The requests a step chooses from: every sample, and the panel's and
/// console's at the system's own ID.
fn requests() -> Vec<AnyRequest> {
    let mut out: Vec<AnyRequest> = qsi::samples::exchanges()
        .into_iter()
        .map(|(q, _)| q)
        .collect();
    for f in [1u8, 3, 11, 12, 20, 21, 22, 2] {
        out.push(AnyRequest::Panel(panel::Request::Function(
            panel::Function {
                id: SYSTEM_ID,
                function: f,
                args: panel::PanelArgs::default(),
            },
        )));
    }
    out.push(AnyRequest::Panel(panel::Request::Functions(
        panel::Functions { id: SYSTEM_ID },
    )));
    out.push(AnyRequest::Console(console::Request::Screen(
        console::Screen { id: SYSTEM_ID },
    )));
    out.push(AnyRequest::Base(base::Request::DownloadDump(
        base::DownloadDump {
            name: "QSD0000001".into(),
        },
    )));
    out
}

struct Rig {
    conn: Conn,
    console: Session,
    log: Arc<Mutex<EventLog>>,
    now: u64,
    pairing: Option<qsitrust::pair::Console>,
}

impl Rig {
    fn new(pinned: bool) -> Option<Self> {
        let limits = Limits {
            max_message: 256 * 1024,
            ..Limits::default()
        };
        let log = Arc::new(Mutex::new(EventLog::new(&limits)));
        let console_fp = Fingerprint([9; 32]);
        let managed_fp = Fingerprint([1; 32]);
        let config = Config {
            hello: standalone_hello(),
            address: "192.0.2.1:4000".into(),
            peer: Peer {
                cert: Vec::new(),
                fingerprint: console_fp,
                pinned,
            },
            binding: Some(Binding {
                mine: managed_fp,
                peer: console_fp,
                exporter: [5; 32],
            }),
            limits,
        };
        let mut host = MemHost::new();
        host.keylock = qsi::base::Keylock::Manual;
        let conn = Conn::new(
            config,
            Box::new(host),
            Box::new(MemOpener),
            log.clone(),
            Arc::new(MemKeeper::with_password(PASSWORD)),
            0,
        )
        .ok()?;
        let console = Session::new(Role::Console, console_hello(), true, limits, 0).ok()?;
        Some(Self {
            conn,
            console,
            log,
            now: 0,
            pairing: None,
        })
    }

    /// Move the bytes both ways a few times; answer a pairing's answer.
    fn pump(&mut self) {
        for _ in 0..4 {
            let out = self.console.take_output();
            if !out.is_empty() {
                self.conn.feed(&out, self.now);
            }
            self.conn.step(self.now);
            let back = self.conn.take_output();
            if !back.is_empty() {
                self.console.feed(&back, self.now);
            }
            self.console.tick(self.now);
            let mut n = 0;
            while let Some(i) = self.console.poll() {
                n += 1;
                assert!(n < 1 << 16, "a console queued without bound");
                self.incoming(i);
            }
        }
        let _ = self.conn.said();
        let _ = self.conn.failed_pairings();
    }

    fn incoming(&mut self, i: Incoming) {
        if let Incoming::Reply(r) = i
            && let Ok(Some(pair::Reply::Answer(a))) = r.typed::<pair::Reply>()
            && let Some(c) = self.pairing.take()
        {
            let answer = qsitrust::pair::Answer {
                spake: a.spake,
                confirm: a.confirm,
                name: a.name,
            };
            if let Ok((confirm, _)) = c.answer(&answer) {
                let _ = self.console.request_msg(
                    "FUZZ",
                    &pair::Request::Confirm(pair::Confirm {
                        confirm: confirm.confirm,
                    }),
                );
            }
        }
    }

    fn pair(&mut self, right: bool) {
        let binding = Binding {
            mine: Fingerprint([9; 32]),
            peer: Fingerprint([1; 32]),
            exporter: [5; 32],
        };
        let password = if right {
            PASSWORD
        } else {
            "0000-0000-0000-0000"
        };
        if let Ok((c, start)) = qsitrust::pair::Console::start(password, binding, "MC01") {
            self.pairing = Some(c);
            let _ = self.console.request_msg(
                "FUZZ",
                &pair::Request::Start(pair::Start {
                    spake: start.spake,
                    name: start.name,
                }),
            );
        }
    }
}

fn run(data: &[u8], pinned: bool) {
    let Some(mut rig) = Rig::new(pinned) else {
        return;
    };
    let requests = requests();
    let mut it = data.iter().copied();
    let mut steps = 0;
    let mut pairings = 0;
    while let Some(b) = it.next() {
        steps += 1;
        if steps > MAX_STEPS {
            break;
        }
        match b % 8 {
            0..=3 => {
                let i = usize::from(it.next().unwrap_or(0)) + usize::from(b / 8) * 256;
                let req = &requests[i % requests.len()];
                let _ = rig.console.request_any("FUZZ", req);
            }
            4 => {
                let n = usize::from(it.next().unwrap_or(0));
                let raw: Vec<u8> = it.by_ref().take(n).collect();
                rig.conn.feed(&raw, rig.now);
            }
            5 => rig.now += u64::from(it.next().unwrap_or(1)) * 100,
            6 if pairings < MAX_PAIRINGS => {
                pairings += 1;
                rig.pair(it.next().unwrap_or(0) % 2 == 0);
            }
            7 => {
                let events = qsi::samples::events();
                let e = &events[usize::from(it.next().unwrap_or(0)) % events.len()];
                let _ = crate::conn::lock(&rig.log).push_any(e);
            }
            _ => {}
        }
        rig.pump();
        if rig.conn.is_closed() {
            break;
        }
    }
    rig.conn.close("done");
    rig.pump();
}

/// A console not pinned, with a pairing password out.
pub fn pre_auth(data: &[u8]) {
    run(data, false);
}

/// A pinned console, answered by a system in memory.
pub fn post_auth(data: &[u8]) {
    run(data, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seeds_run() {
        for s in SEEDS {
            pre_auth(s);
            post_auth(s);
        }
        let all: Vec<u8> = (0..=255u8).collect();
        pre_auth(&all);
        post_auth(&all);
    }
}
