// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QAP client built from `qap-proto` against `qapd::testing`'s
//! system over loopback: [`start`] gives the client and the system.

#![allow(dead_code)]

pub mod log;

use std::io::{ErrorKind, Read, Write};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ipc::Wire;
use qap_proto::ops::connection::Hello;
use qap_proto::{Envelope, Kind, Op, Reassembly, Status};
use qapd::frames::FrameReader;
pub use qapd::testing::System as Controls;
use qapd::{Closed, Config, Polled};

/// A client over a TCP stream, non-blocking (no read timeout: see
/// `qapd::Polled`).
pub struct Client {
    pub s: Polled,
    frames: FrameReader,
    asm: Reassembly,
    next_id: u32,
}

impl Client {
    pub fn send(&mut self, env: &Envelope) {
        let mut b = Vec::new();
        env.write_to(&mut b).unwrap();
        self.s.write_all(&b).unwrap();
    }

    /// Raw bytes.
    pub fn raw(&mut self, b: &[u8]) {
        let _ = self.s.write_all(b);
    }

    /// A request for `op`, the next ID.
    pub fn request<B: Wire>(&mut self, op: Op, body: &B) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&Envelope::request(id, op, body));
        id
    }

    /// The next whole reply or event; `None` once the server closed.
    pub fn recv(&mut self) -> Option<Envelope> {
        let end = Instant::now() + Duration::from_secs(10);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            while let Some(env) = self.frames.next_frame().expect("a valid frame") {
                if let Some(whole) = self.asm.push(env) {
                    return Some(whole);
                }
            }
            assert!(Instant::now() < end, "no frame from the server");
            match self.s.read(&mut buf) {
                Ok(0) => return None,
                Ok(n) => self.frames.push(&buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return None,
            }
        }
    }

    /// The next frame as it came, not joined; `None` once the server
    /// closed.
    pub fn recv_frame(&mut self) -> Option<Envelope> {
        let end = Instant::now() + Duration::from_secs(10);
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            if let Some(env) = self.frames.next_frame().expect("a valid frame") {
                return Some(env);
            }
            assert!(Instant::now() < end, "no frame from the server");
            match self.s.read(&mut buf) {
                Ok(0) => return None,
                Ok(n) => self.frames.push(&buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => return None,
            }
        }
    }

    /// The reply to request `id`, skipping nothing: it must come next.
    pub fn reply(&mut self, id: u32) -> Envelope {
        let r = self.recv().expect("a reply");
        assert_eq!((r.kind, r.request_id), (Kind::Reply, id), "{r:?}");
        r
    }

    /// The replies to requests `ids`, in whatever order they come, by
    /// ID; nothing else may come between.
    pub fn replies(&mut self, ids: &[u32]) -> std::collections::HashMap<u32, Envelope> {
        let mut got = std::collections::HashMap::new();
        while got.len() < ids.len() {
            let r = self.recv().expect("a reply");
            assert!(r.kind == Kind::Reply && ids.contains(&r.request_id), "{r:?}");
            assert!(got.insert(r.request_id, r).is_none(), "one reply each");
        }
        got
    }

    /// Sign on as `user`.
    pub fn sign_on(&mut self, user: &str, password: &str) {
        let s = self.call(
            Op::SignOn,
            &qap_proto::ops::signon::SignOn {
                user: user.into(),
                password: qap_proto::Secret::from(password),
            },
        );
        assert_eq!(s.status, Status::Ok);
    }

    /// Send a request and take its reply.
    pub fn call<B: Wire>(&mut self, op: Op, body: &B) -> Envelope {
        let id = self.request(op, body);
        self.reply(id)
    }

    /// Hello for version 1, ID 1.
    pub fn hello(&mut self) -> Envelope {
        self.hello_range(1, 1)
    }

    /// Hello offering versions `min` to `max`, ID 1.
    pub fn hello_range(&mut self, min: u16, max: u16) -> Envelope {
        self.next_id = 0;
        let r = self.call(
            Op::Hello,
            &Hello {
                min_version: min,
                max_version: max,
                client_name: "test".into(),
                client_version: "0".into(),
                capabilities: Vec::new(),
            },
        );
        assert_eq!(r.status, Status::Ok);
        r
    }

    /// Whether the server closed the connection.
    pub fn closed(&mut self) -> bool {
        self.recv().is_none()
    }

    pub fn set_next_id(&mut self, id: u32) {
        self.next_id = id;
    }
}

/// A server and its client.
pub struct Run {
    pub client: Client,
    pub c: Arc<Controls>,
    pub server: JoinHandle<Closed>,
    /// The connection job's number, the connection's too.
    pub job: u32,
}

/// Start a server on loopback with `cfg`.
pub fn start(cfg: Config) -> Run {
    start_with(cfg, Controls::new())
}

/// Another connection job of the same system (`c`).
pub fn start_with(cfg: Config, c: Arc<Controls>) -> Run {
    c.gate.set(true);
    let job = qapd::testing::next_job();
    let (s, server) = qapd::testing::serve_loopback_as(cfg, &c, job);
    Run {
        client: Client {
            s: Polled::new(s).unwrap(),
            frames: FrameReader::new(),
            asm: Reassembly::new(),
            next_id: 0,
        },
        c,
        server,
        job,
    }
}

/// Quick limits for tests.
pub fn cfg() -> Config {
    Config {
        poll: Duration::from_millis(10),
        ..Config::default()
    }
}
