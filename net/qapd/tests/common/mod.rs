// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QAP client built from `qap-proto` and a server in memory over
//! loopback: [`start`] gives the client and the host's controls.

#![allow(dead_code)]

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ipc::Wire;
use qap_proto::ops::connection::Hello;
use qap_proto::{Envelope, Kind, Op, Reassembly, ServerMessage, Status};
use qapd::frames::FrameReader;
use qapd::{Closed, Config, Ending, Host, SystemId};

/// Holds operations in progress while closed.
#[derive(Default)]
pub struct Gate {
    open: Mutex<bool>,
    cv: Condvar,
}

impl Gate {
    pub fn set(&self, open: bool) {
        *self.open.lock().unwrap() = open;
        self.cv.notify_all();
    }

    fn pass(&self) {
        let g = self.open.lock().unwrap();
        drop(self.cv.wait_while(g, |o| !*o).unwrap());
    }
}

/// What a test controls of the host.
#[derive(Default)]
pub struct Controls {
    pub gate: Gate,
    pub ending: Mutex<Option<Ending>>,
}

/// The host in memory.
pub struct MemHost {
    pub c: Arc<Controls>,
}

impl Host for MemHost {
    fn system_id(&mut self) -> SystemId {
        SystemId {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            time_zone: "QN0000UTC".into(),
            ccsid: 37,
        }
    }

    fn now_ms(&mut self) -> i64 {
        self.c.gate.pass();
        1_793_000_000_000
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        ServerMessage {
            id: id.into(),
            severity: 0,
            data: data.join(" ").into_bytes(),
            text: format!("{id} {}", data.join(" ")),
            help: String::new(),
        }
    }

    fn ending(&mut self) -> Option<Ending> {
        self.c.ending.lock().unwrap().take()
    }
}

/// A client over a TCP stream.
pub struct Client {
    pub s: TcpStream,
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

    /// The reply to request `id`, skipping nothing: it must come next.
    pub fn reply(&mut self, id: u32) -> Envelope {
        let r = self.recv().expect("a reply");
        assert_eq!((r.kind, r.request_id), (Kind::Reply, id), "{r:?}");
        r
    }

    /// Send a request and take its reply.
    pub fn call<B: Wire>(&mut self, op: Op, body: &B) -> Envelope {
        let id = self.request(op, body);
        self.reply(id)
    }

    /// Hello for version 1, ID 1.
    pub fn hello(&mut self) -> Envelope {
        self.next_id = 0;
        let r = self.call(
            Op::Hello,
            &Hello {
                min_version: 1,
                max_version: 1,
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
}

/// Start a server on loopback with `cfg`.
pub fn start(cfg: Config) -> Run {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let c = Arc::new(Controls::default());
    c.gate.set(true);
    let hc = Arc::clone(&c);
    let server = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let mut host = MemHost { c: hc };
        qapd::serve(s, &mut host, cfg)
    });
    let s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(50))).unwrap();
    Run {
        client: Client {
            s,
            frames: FrameReader::new(),
            asm: Reassembly::new(),
            next_id: 0,
        },
        c,
        server,
    }
}

/// Quick limits for tests.
pub fn cfg() -> Config {
    Config {
        poll: Duration::from_millis(10),
        ..Config::default()
    }
}
