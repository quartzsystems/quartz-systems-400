// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A QAP client built from `qap-proto` and a server in memory over
//! loopback: [`start`] gives the client and the host's controls.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ipc::Wire;
use qap_proto::ops::connection::Hello;
use qap_proto::ops::signon::SignOnReply;
use qap_proto::ops::system::{Product, SystemInfo};
use qap_proto::{Envelope, Kind, Op, Reassembly, Secret, ServerMessage, Status, Token};
use qapd::frames::FrameReader;
use qapd::sessions::Sessions;
use qap_proto::ops::command::{CommandDefinition, CommandReply};
use qap_proto::types::ObjectName;
use qapd::{Closed, Config, Ending, Host, Ran, Refusal, SystemId};

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

/// A user profile as the model keeps it.
#[derive(Clone, Debug, Default)]
pub struct User {
    pub password: String,
    pub disabled: bool,
    pub expired: bool,
    pub none: bool,
    pub invalid: u32,
}

/// What a test controls of the host. The sign-on model follows the
/// display's rules: `QMAXSIGN` 3 with the profile disabled, CPF1107,
/// CPF1392 before the last try, CPF1393 at it, CPF1394 disabled,
/// CPF1118 `*NONE`, CPD9BAE expired; audit entries as `PW` types.
#[derive(Default)]
pub struct Controls {
    pub gate: Gate,
    pub ending: Mutex<Option<Ending>>,
    pub users: Mutex<HashMap<String, User>>,
    pub sessions: Mutex<Sessions>,
    /// Unix seconds.
    pub clock: Mutex<u64>,
    pub idle_secs: u32,
    pub audit: Mutex<Vec<String>>,
    /// Products `SystemInfo` lists.
    pub products: Mutex<usize>,
    /// Who each connection job runs as, by job.
    pub running_as: Mutex<HashMap<u32, String>>,
    tokens: AtomicU32,
}

impl Controls {
    pub fn new() -> Arc<Self> {
        let c = Self {
            idle_secs: 600,
            products: Mutex::new(3),
            ..Self::default()
        };
        c.user("BOB", "SECRET1");
        Arc::new(c)
    }

    pub fn user(&self, name: &str, password: &str) {
        self.users.lock().unwrap().insert(
            name.into(),
            User {
                password: password.into(),
                ..User::default()
            },
        );
    }

    pub fn edit(&self, name: &str, f: impl FnOnce(&mut User)) {
        f(self.users.lock().unwrap().get_mut(name).unwrap());
    }

    pub fn get(&self, name: &str) -> User {
        self.users.lock().unwrap()[name].clone()
    }

    pub fn advance(&self, secs: u64) {
        *self.clock.lock().unwrap() += secs;
    }
}

/// The host in memory: one connection job.
pub struct MemHost {
    pub c: Arc<Controls>,
    pub job: u32,
}

impl MemHost {
    fn refuse(&mut self, status: Status, id: &str, data: &[&str]) -> Refusal {
        Refusal {
            status,
            message: self.message(id, data),
        }
    }

    fn reply(&self, token: Token, user: &str) -> SignOnReply {
        SignOnReply {
            token,
            idle_timeout_secs: self.c.idle_secs,
            profile: user.into(),
            text: format!("{user} profile"),
            ccsid: 37,
            date_format: "*MDY".into(),
            date_separator: "/".into(),
            time_separator: ":".into(),
            home_directory: format!("/home/{user}"),
            special_authorities: Vec::new(),
        }
    }

    /// The display's checks; the profile and whether it expired.
    fn check(&mut self, user: &str, password: &str) -> Result<(String, bool), Refusal> {
        let user = user.trim().to_ascii_uppercase();
        let entry = |c: &Controls, e: &str| c.audit.lock().unwrap().push(format!("PW {e} {user}"));
        let mut users = self.c.users.lock().unwrap();
        let Some(u) = users.get_mut(&user) else {
            drop(users);
            entry(&self.c, "U");
            return Err(self.refuse(Status::SignOnFailed, "CPF1107", &[]));
        };
        if u.disabled {
            drop(users);
            entry(&self.c, "Q");
            return Err(self.refuse(Status::SignOnFailed, "CPF1394", &[&user]));
        }
        if u.none {
            drop(users);
            return Err(self.refuse(Status::SignOnFailed, "CPF1118", &[&user]));
        }
        if u.password != password {
            u.invalid += 1;
            let n = u.invalid;
            if n >= 3 {
                u.disabled = true;
            }
            drop(users);
            entry(&self.c, "P");
            return Err(match n {
                3.. => self.refuse(Status::SignOnFailed, "CPF1393", &[&user]),
                2 => self.refuse(Status::SignOnFailed, "CPF1392", &[]),
                _ => self.refuse(Status::SignOnFailed, "CPF1107", &[]),
            });
        }
        u.invalid = 0;
        Ok((user, u.expired))
    }

    fn open(&mut self, user: &str) -> SignOnReply {
        let n = self.c.tokens.fetch_add(1, Ordering::SeqCst) + 1;
        let token = Token([u8::try_from(n % 251).unwrap(); 32]);
        self.c.sessions.lock().unwrap().create(&token, user, self.c.idle_secs, self.job);
        self.c.running_as.lock().unwrap().insert(self.job, user.into());
        self.reply(token, user)
    }
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

    /// Its text says whose language it was looked up in.
    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        let lang = self
            .c
            .running_as
            .lock()
            .unwrap()
            .get(&self.job)
            .map_or_else(|| "system".to_string(), |u| format!("lang of {u}"));
        ServerMessage {
            id: id.into(),
            severity: 0,
            data: data.join(" ").into_bytes(),
            text: format!("{id} {} ({lang})", data.join(" ")),
            help: String::new(),
        }
    }

    fn system_info(&mut self) -> SystemInfo {
        self.c.gate.pass();
        let n = *self.c.products.lock().unwrap();
        SystemInfo {
            system_name: "QSHQ400".into(),
            release: "V1R0M0".into(),
            serial: "1234567".into(),
            model: "Q01".into(),
            ipl_time: 1_792_000_000_000,
            products: (0..n)
                .map(|i| Product {
                    id: format!("QS{i:05}"),
                    option: "0000".into(),
                    release: "V1R0M0".into(),
                })
                .collect(),
        }
    }

    fn ending(&mut self) -> Option<Ending> {
        self.c.ending.lock().unwrap().take()
    }

    fn sign_on(&mut self, user: &str, password: &Secret) -> Result<SignOnReply, Refusal> {
        let (user, expired) = self.check(user, password.expose())?;
        if expired {
            return Err(self.refuse(Status::PasswordExpired, "CPD9BAE", &[]));
        }
        Ok(self.open(&user))
    }

    fn change_password(&mut self, user: &str, old: &Secret, new: &Secret) -> Result<SignOnReply, Refusal> {
        let me = self.c.running_as.lock().unwrap().get(&self.job).cloned();
        let user = user.trim().to_ascii_uppercase();
        match &me {
            Some(m) if *m != user => return Err(self.refuse(Status::NotAuthorized, "CPF22E9", &[&user])),
            Some(_) => {
                if self.c.get(&user).password != old.expose() {
                    return Err(self.refuse(Status::Error, "CPF22E2", &[&user]));
                }
            }
            None => {
                self.check(&user, old.expose())?;
            }
        }
        if new.expose().len() < 6 || new.expose() == old.expose() {
            return Err(self.refuse(Status::Error, "CPF2362", &[]));
        }
        self.c.edit(&user, |u| {
            u.password = new.expose().into();
            u.expired = false;
        });
        Ok(match me {
            Some(_) => self.reply(Token([0; 32]), &user),
            None => self.open(&user),
        })
    }

    fn resume(&mut self, token: &Token) -> Result<SignOnReply, Refusal> {
        let now = *self.c.clock.lock().unwrap();
        let r = self.c.sessions.lock().unwrap().resume(token, self.job, now, &|_| true);
        match r {
            Ok(s) => {
                self.c.running_as.lock().unwrap().insert(self.job, s.user.clone());
                Ok(self.reply(token.clone(), &s.user))
            }
            Err(_) => Err(self.refuse(Status::SessionExpired, "TCP1A53", &[])),
        }
    }

    fn sign_off(&mut self, token: &Token) {
        self.c.sessions.lock().unwrap().end(token);
        self.c.running_as.lock().unwrap().remove(&self.job);
    }

    fn detach(&mut self, token: &Token) {
        let now = *self.c.clock.lock().unwrap();
        self.c.sessions.lock().unwrap().detach(token, self.job, now);
        self.c.running_as.lock().unwrap().remove(&self.job);
    }

    fn run_command(&mut self, _command: &str) -> Ran {
        Ran {
            status: Status::NotFound,
            escape: None,
            reply: CommandReply::default(),
        }
    }

    fn check_command(&mut self, _command: &str) -> (Status, Vec<ServerMessage>) {
        (Status::NotFound, Vec::new())
    }

    fn describe_command(&mut self, _command: &ObjectName) -> Result<CommandDefinition, Refusal> {
        Err(self.refuse(Status::NotFound, "CPD0030", &[]))
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
}

/// Start a server on loopback with `cfg`.
pub fn start(cfg: Config) -> Run {
    start_with(cfg, Controls::new())
}

/// Another connection job of the same system (`c`).
pub fn start_with(cfg: Config, c: Arc<Controls>) -> Run {
    static JOBS: AtomicU32 = AtomicU32::new(100);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    c.gate.set(true);
    let hc = Arc::clone(&c);
    let job = JOBS.fetch_add(1, Ordering::SeqCst);
    let server = thread::spawn(move || {
        let (s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        let mut host = MemHost { c: hc, job };
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
