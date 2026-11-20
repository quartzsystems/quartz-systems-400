// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A console on `qsisess`'s client side over `qsitrust`'s TLS, blocking:
//! what this crate's tests and the conformance harness drive a system
//! with. It pins the system it pairs with ([`ConsolePins`]).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use qsi::any::AnyRequest;
use qsi::{Agreed, Hello, PeerKind, caps, pair};
use qsisess::{Event, Incoming, Limits, Reply, Role, Session};
use qsitrust::pair::Console;
use qsitrust::{Fingerprint, Identity, Pins, tls};
use rustls::{ClientConnection, StreamOwned};

#[derive(Debug, Default)]
struct PinState {
    system: Option<Fingerprint>,
    pairing: bool,
}

/// What a console pins: the one system, or any while pairing.
#[derive(Clone, Debug, Default)]
pub struct ConsolePins(Arc<Mutex<PinState>>);

impl ConsolePins {
    /// Pin `fp`.
    pub fn pin(&self, fp: Fingerprint) {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.system = Some(fp);
        s.pairing = false;
    }

    /// Let any system in, to pair with it.
    pub fn pairing(&self, open: bool) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pairing = open;
    }

    /// The system pinned.
    #[must_use]
    pub fn system(&self) -> Option<Fingerprint> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).system
    }
}

impl Pins for ConsolePins {
    fn pinned(&self, peer: &Fingerprint) -> bool {
        self.system() == Some(*peer)
    }

    fn pairing_open(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pairing
    }
}

/// A console's hello asking for what a standalone system has.
#[must_use]
pub fn hello(name: &str) -> Hello {
    Hello {
        version: qsi::VERSION,
        caps: caps::STANDALONE,
        kind: PeerKind::Console,
        name: name.into(),
        release: "V1R1M0".into(),
        serial: String::new(),
    }
}

fn quiet(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// One console session over TLS.
pub struct Client {
    sess: Session,
    tls: StreamOwned<ClientConnection, TcpStream>,
    identity: Identity,
    t0: Instant,
    gone: bool,
    /// What the session handed up that no wait took, oldest first.
    pub seen: Vec<Incoming>,
    /// What the two agreed on.
    pub agreed: Option<Agreed>,
    /// The system's hello.
    pub peer: Option<Hello>,
    /// The user named in requests.
    pub user: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Client {
    /// Connect to `addr`, finish TLS and the hellos within `wait`.
    ///
    /// # Errors
    /// The connection, the handshake (a system not pinned and pairing
    /// not open, or a console it refuses) or the hellos failed.
    pub fn connect(
        addr: SocketAddr,
        identity: &Identity,
        pins: &ConsolePins,
        hello: Hello,
        wait: Duration,
    ) -> Result<Self, String> {
        let config =
            tls::client_config(identity, Arc::new(pins.clone())).map_err(|e| e.to_string())?;
        let sock = TcpStream::connect_timeout(&addr, wait).map_err(|e| format!("connect: {e}"))?;
        sock.set_read_timeout(Some(Duration::from_millis(10)))
            .map_err(|e| e.to_string())?;
        let conn = ClientConnection::new(config, tls::server_name()).map_err(|e| e.to_string())?;
        let mut tls = StreamOwned::new(conn, sock);
        let t0 = Instant::now();
        while tls.conn.is_handshaking() {
            match tls.conn.complete_io(&mut tls.sock) {
                Ok(_) => {}
                Err(e) if quiet(&e) && t0.elapsed() < wait => {}
                Err(e) => return Err(format!("TLS: {e}")),
            }
        }
        let sess = Session::new(Role::Console, hello, true, Limits::default(), 0)
            .map_err(|e| e.to_string())?;
        let mut c = Self {
            sess,
            tls,
            identity: identity.clone(),
            t0,
            gone: false,
            seen: Vec::new(),
            agreed: None,
            peer: None,
            user: "QSMC".into(),
        };
        let ready = c.until(wait, |i| match i {
            Incoming::Ready { peer, agreed } => Some((peer.clone(), *agreed)),
            _ => None,
        });
        let (peer, agreed) = ready.ok_or_else(|| "no hello from the system".to_string())?;
        c.peer = Some(peer);
        c.agreed = Some(agreed);
        Ok(c)
    }

    fn now(&self) -> u64 {
        u64::try_from(self.t0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Send what waits, read what came, hand it up into [`Client::seen`].
    pub fn step(&mut self) {
        let out = self.sess.take_output();
        if !out.is_empty()
            && !self.gone
            && (self.tls.write_all(&out).is_err() || self.tls.flush().is_err())
        {
            self.gone = true;
        }
        let mut buf = [0u8; 16 * 1024];
        if !self.gone {
            match self.tls.read(&mut buf) {
                Ok(0) => self.gone = true,
                Ok(n) => {
                    let now = self.now();
                    self.sess.feed(&buf[..n], now);
                }
                Err(e) if quiet(&e) => {}
                Err(_) => self.gone = true,
            }
        }
        let now = self.now();
        self.sess.tick(now);
        while let Some(i) = self.sess.poll() {
            self.seen.push(i);
        }
    }

    /// Whether the system is gone or the session closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.gone || self.sess.is_closed()
    }

    /// Step until `want` takes something of [`Client::seen`], the system
    /// is gone, or `wait` passes.
    pub fn until<T>(
        &mut self,
        within: Duration,
        mut want: impl FnMut(&Incoming) -> Option<T>,
    ) -> Option<T> {
        let end = Instant::now() + within;
        loop {
            if let Some(at) = self.seen.iter().position(|i| want(i).is_some()) {
                let i = self.seen.remove(at);
                return want(&i);
            }
            if self.is_closed() || Instant::now() >= end {
                return None;
            }
            self.step();
        }
    }

    /// Ask, and wait up to `wait` for the reply.
    ///
    /// # Errors
    /// The session refused to send it, or no reply came.
    pub fn ask(&mut self, req: &AnyRequest, wait: Duration) -> Result<Reply, String> {
        let user = self.user.clone();
        let id = self
            .sess
            .request_any(&user, req)
            .map_err(|e| e.to_string())?;
        self.until(wait, |i| match i {
            Incoming::Reply(r) if r.id == id => Some(r.clone()),
            _ => None,
        })
        .ok_or_else(|| "no reply".to_string())
    }

    /// The events handed up so far, taken.
    pub fn events(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        self.seen.retain(|i| match i {
            Incoming::Event(e) => {
                out.push(e.clone());
                false
            }
            _ => true,
        });
        out
    }

    /// Step for `wait`, then take the events.
    pub fn events_for(&mut self, wait: Duration) -> Vec<Event> {
        let end = Instant::now() + wait;
        while Instant::now() < end && !self.is_closed() {
            self.step();
        }
        self.events()
    }

    /// The session's own goodbye, then the connection's end.
    pub fn close(mut self) {
        self.sess.close("done");
        self.step();
        self.tls.conn.send_close_notify();
        let _ = self.tls.flush();
    }

    /// Pair with `password` as `name`: the system's fingerprint and name.
    /// With `abandon`, a confirmation that does not hold is not sent (as
    /// a real console stops); else zeros are sent, as a hostile one would.
    ///
    /// # Errors
    /// Why not: the refusal, or that the system's answer did not hold.
    pub fn pair(
        &mut self,
        password: &str,
        name: &str,
        wait: Duration,
    ) -> Result<(Fingerprint, String), String> {
        let binding = tls::binding(&self.tls.conn, &self.identity).map_err(|e| e.to_string())?;
        let (console, start) =
            Console::start(password, binding, name).map_err(|e| e.to_string())?;
        let r = self.ask(
            &AnyRequest::Pair(pair::Request::Start(pair::Start {
                spake: start.spake,
                name: start.name,
            })),
            wait,
        )?;
        if let Some((id, text)) = r.refusal() {
            return Err(format!("{id} {text}"));
        }
        let Ok(Some(pair::Reply::Answer(a))) = r.typed::<pair::Reply>() else {
            return Err("not an answer".into());
        };
        let answer = qsitrust::pair::Answer {
            spake: a.spake,
            confirm: a.confirm,
            name: a.name,
        };
        let (confirm, system) = console
            .answer(&answer)
            .map_err(|_| "the system's answer did not hold: a wrong password".to_string())?;
        let done = self.ask(
            &AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
                confirm: confirm.confirm,
            })),
            wait,
        )?;
        if let Some((id, text)) = done.refusal() {
            return Err(format!("{id} {text}"));
        }
        Ok((system.fingerprint, system.name))
    }

    /// Send pairing messages raw: a start, then whatever confirmation is
    /// given; the replies. For a replay.
    ///
    /// # Errors
    /// No reply came.
    pub fn pair_raw(
        &mut self,
        start: &pair::Start,
        confirm: &[u8],
        wait: Duration,
    ) -> Result<(Reply, Reply), String> {
        let a = self.ask(&AnyRequest::Pair(pair::Request::Start(start.clone())), wait)?;
        let b = self.ask(
            &AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
                confirm: confirm.to_vec(),
            })),
            wait,
        )?;
        Ok((a, b))
    }

    /// A pairing exchange's messages, kept for a replay: the start and
    /// the confirmation, with whether it paired.
    ///
    /// # Errors
    /// As [`Client::pair`].
    pub fn pair_kept(
        &mut self,
        password: &str,
        name: &str,
        wait: Duration,
    ) -> Result<(pair::Start, Vec<u8>, bool), String> {
        let binding = tls::binding(&self.tls.conn, &self.identity).map_err(|e| e.to_string())?;
        let (console, start) =
            Console::start(password, binding, name).map_err(|e| e.to_string())?;
        let start = pair::Start {
            spake: start.spake,
            name: start.name,
        };
        let r = self.ask(&AnyRequest::Pair(pair::Request::Start(start.clone())), wait)?;
        let Ok(Some(pair::Reply::Answer(a))) = r.typed::<pair::Reply>() else {
            return Err(r
                .refusal()
                .map_or("not an answer".into(), |(i, t)| format!("{i} {t}")));
        };
        let answer = qsitrust::pair::Answer {
            spake: a.spake,
            confirm: a.confirm,
            name: a.name,
        };
        let (confirm, _) = console
            .answer(&answer)
            .map_err(|_| "wrong password".to_string())?;
        let done = self.ask(
            &AnyRequest::Pair(pair::Request::Confirm(pair::Confirm {
                confirm: confirm.confirm.clone(),
            })),
            wait,
        )?;
        Ok((start, confirm.confirm, done.refusal().is_none()))
    }
}
