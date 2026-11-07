// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client driven over a socket: [`acquire`] gets one lease and
//! returns (the IPL, the installer); an [`Agent`] is a thread that
//! keeps a lease on a link for an interface, puts its address on and
//! takes it off through a [`Sink`], and stops with or without a
//! `RELEASE`.

use std::fmt;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::Error;
use crate::client::{Client, Config, Dest, Out, State};
use crate::lease::Lease;
use crate::packet::Message;
use crate::sys::Socket;

/// Milliseconds since the epoch.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// What carries the messages.
pub trait Wire {
    /// Send to port 67.
    fn send(&self, to: Dest, bytes: &[u8]) -> io::Result<()>;
    /// One datagram within `wait`.
    fn recv(&self, wait: Duration) -> io::Result<Option<Vec<u8>>>;
}

impl Wire for Socket {
    fn send(&self, to: Dest, bytes: &[u8]) -> io::Result<()> {
        Socket::send(self, to, bytes)
    }
    fn recv(&self, wait: Duration) -> io::Result<Option<Vec<u8>>> {
        Socket::recv(self, wait)
    }
}

/// Where a lease's address goes.
pub trait Sink: Send {
    /// Put `l`'s address (and its router) on the link.
    fn bind(&mut self, l: &Lease) -> Result<(), String>;
    /// Take `l`'s address (and its router) off.
    fn unbind(&mut self, l: &Lease);
}

fn seed() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos());
    u64::try_from(t & u128::from(u64::MAX)).unwrap_or(1)
        ^ u64::from(std::process::id()).rotate_left(32)
}

/// Get one lease on `wire` within `timeout`, nothing applied.
pub fn acquire_on(
    wire: &dyn Wire,
    cfg: Config,
    timeout: Duration,
    link: &str,
) -> Result<Lease, Error> {
    let end = Instant::now() + timeout;
    let mut c = Client::new(cfg, now_ms(), seed());
    let mut outs = c.start(now_ms());
    loop {
        for o in outs.drain(..) {
            match o {
                Out::Send(d, m) => wire.send(d, &m.encode())?,
                Out::Bind(l) => return Ok(l),
                Out::Unbind(_) => {}
            }
        }
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::NoAnswer(link.to_string()));
        }
        let wait = Duration::from_millis(c.deadline().saturating_sub(now_ms())).min(left);
        if let Some(b) = wire.recv(wait)?
            && let Ok(m) = Message::decode(&b)
        {
            outs = c.receive(now_ms(), &m);
            continue;
        }
        outs = c.poll(now_ms());
    }
}

/// Get one lease on `link` within `timeout` (the IPL's `qs400.ip=dhcp`).
pub fn acquire(link: &str, cfg: Config, timeout: Duration) -> Result<Lease, Error> {
    let s = Socket::open(link)?;
    acquire_on(&s, cfg, timeout, link)
}

/// What an agent shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The client's state (`SELECTING`, `BOUND`, ...).
    pub state: String,
    /// The lease held.
    pub lease: Option<Lease>,
    /// The last failure (a socket, the address).
    pub error: String,
}

/// How an agent ends.
enum Stop {
    Release,
    Keep,
}

/// A thread keeping a lease on one link.
pub struct Agent {
    status: Arc<Mutex<Status>>,
    stop: Sender<Stop>,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for Agent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Agent")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

fn apply(outs: Vec<Out>, wire: &dyn Wire, sink: &mut dyn Sink, st: &Mutex<Status>) {
    for o in outs {
        let r = match o {
            Out::Send(d, m) => wire.send(d, &m.encode()).map_err(|e| format!("send: {e}")),
            Out::Bind(l) => sink.bind(&l),
            Out::Unbind(l) => {
                sink.unbind(&l);
                Ok(())
            }
        };
        if let Err(e) = r {
            st.lock().unwrap_or_else(PoisonError::into_inner).error = e;
        }
    }
}

fn serve(
    wire: &dyn Wire,
    mut c: Client,
    mut sink: Box<dyn Sink>,
    st: &Mutex<Status>,
    stop: &Receiver<Stop>,
) {
    let show = |c: &Client, st: &Mutex<Status>| {
        let mut s = st.lock().unwrap_or_else(PoisonError::into_inner);
        s.state = c.state().name().to_string();
        s.lease = c.lease().cloned();
    };
    let outs = c.start(now_ms());
    apply(outs, wire, sink.as_mut(), st);
    show(&c, st);
    loop {
        match stop.try_recv() {
            Ok(Stop::Release) => {
                let outs = c.release(now_ms());
                apply(outs, wire, sink.as_mut(), st);
                break;
            }
            Ok(Stop::Keep) | Err(mpsc::TryRecvError::Disconnected) => {
                let outs = c.stop();
                apply(outs, wire, sink.as_mut(), st);
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        // At most a second per wait, so a stop is seen promptly.
        let wait = Duration::from_millis(c.deadline().saturating_sub(now_ms()).min(1000));
        let outs = match wire.recv(wait) {
            Ok(Some(b)) => match Message::decode(&b) {
                Ok(m) => c.receive(now_ms(), &m),
                Err(_) => Vec::new(),
            },
            Ok(None) => c.poll(now_ms()),
            Err(e) => {
                st.lock().unwrap_or_else(PoisonError::into_inner).error = format!("receive: {e}");
                std::thread::sleep(Duration::from_millis(500));
                c.poll(now_ms())
            }
        };
        apply(outs, wire, sink.as_mut(), st);
        show(&c, st);
    }
    let mut s = st.lock().unwrap_or_else(PoisonError::into_inner);
    s.state = State::Released.name().to_string();
    s.lease = None;
}

impl Agent {
    /// Start keeping a lease on `link`.
    pub fn spawn(link: &str, cfg: Config, sink: Box<dyn Sink>) -> io::Result<Self> {
        let socket = Socket::open(link)?;
        Ok(Self::spawn_on(Box::new(socket), cfg, sink))
    }

    /// Start one over any wire (tests).
    pub fn spawn_on(wire: Box<dyn Wire + Send>, cfg: Config, sink: Box<dyn Sink>) -> Self {
        let status = Arc::new(Mutex::new(Status {
            state: State::Init.name().into(),
            ..Status::default()
        }));
        let (tx, rx) = mpsc::channel();
        let st = Arc::clone(&status);
        let client = Client::new(cfg, now_ms(), seed());
        let thread = std::thread::Builder::new()
            .name("dhcp".into())
            .spawn(move || serve(wire.as_ref(), client, sink, &st, &rx))
            .ok();
        Self {
            status,
            stop: tx,
            thread,
        }
    }

    /// What it holds now.
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// End it: `release` gives the address back to the server (an
    /// interface ended), otherwise the lease is kept for the next start.
    /// The address comes off the link either way; waits for the thread.
    pub fn stop(mut self, release: bool) {
        let _ = self
            .stop
            .send(if release { Stop::Release } else { Stop::Keep });
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    /// Wait until it holds a lease, at most `wait`.
    pub fn wait_bound(&self, wait: Duration) -> Option<Lease> {
        let end = Instant::now() + wait;
        while Instant::now() < end {
            if let Some(l) = self.status().lease {
                return Some(l);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.stop.send(Stop::Keep);
    }
}

/// A server in memory for tests: answers `DISCOVER` and `REQUEST` from
/// one pool address, `RELEASE` noted.
#[doc(hidden)]
pub mod testing {
    use super::{Dest, Wire};
    use crate::packet::{BOOTREPLY, Kind, Message};
    use std::io;
    use std::net::Ipv4Addr;
    use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// The server's side of the conversation.
    #[derive(Default, Debug)]
    pub struct Log {
        /// Message types seen, in order.
        pub seen: Vec<Kind>,
    }

    /// A wire whose far end is a tiny DHCP server.
    pub struct MemServer {
        /// The address it hands out.
        pub addr: Ipv4Addr,
        /// Its own address.
        pub me: Ipv4Addr,
        /// Lease seconds.
        pub secs: u32,
        /// What it saw.
        pub log: Arc<Mutex<Log>>,
        tx: Sender<Vec<u8>>,
        rx: Receiver<Vec<u8>>,
    }

    impl MemServer {
        /// A server handing out `addr` for `secs`.
        pub fn new(addr: Ipv4Addr, secs: u32) -> Self {
            let (tx, rx) = channel();
            Self {
                addr,
                me: Ipv4Addr::new(10, 0, 2, 2),
                secs,
                log: Arc::default(),
                tx,
                rx,
            }
        }
    }

    impl Wire for MemServer {
        fn send(&self, _to: Dest, bytes: &[u8]) -> io::Result<()> {
            let m = Message::decode(bytes).map_err(|e| io::Error::other(e.to_string()))?;
            let Some(k) = m.kind() else { return Ok(()) };
            self.log.lock().expect("log").seen.push(k);
            let answer = match k {
                Kind::Discover => Kind::Offer,
                Kind::Request => Kind::Ack,
                _ => return Ok(()),
            };
            let mut r = Message::request(answer, m.xid, m.chaddr);
            r.op = BOOTREPLY;
            r.yiaddr = self.addr;
            r.options.server_id = Some(self.me);
            r.options.subnet_mask = Some(Ipv4Addr::new(255, 255, 255, 0));
            r.options.routers = vec![self.me];
            r.options.lease_secs = Some(self.secs);
            let _ = self.tx.send(r.encode());
            Ok(())
        }

        fn recv(&self, wait: Duration) -> io::Result<Option<Vec<u8>>> {
            match self.rx.recv_timeout(wait) {
                Ok(b) => Ok(Some(b)),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Ok(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::MemServer;
    use super::*;
    use crate::packet::Kind;
    use std::net::Ipv4Addr;

    #[derive(Clone, Default)]
    struct Rec(Arc<Mutex<Vec<String>>>);

    impl Sink for Rec {
        fn bind(&mut self, l: &Lease) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .push(format!("bind {}/{}", l.addr, l.prefix()));
            Ok(())
        }
        fn unbind(&mut self, l: &Lease) {
            self.0.lock().unwrap().push(format!("unbind {}", l.addr));
        }
    }

    const MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];

    #[test]
    fn acquire_one() {
        let s = MemServer::new(Ipv4Addr::new(10, 0, 2, 15), 3600);
        let l = acquire_on(&s, Config::new(MAC), Duration::from_secs(5), "test0").unwrap();
        assert_eq!(l.addr, Ipv4Addr::new(10, 0, 2, 15));
        assert_eq!(s.log.lock().unwrap().seen, [Kind::Discover, Kind::Request]);
    }

    #[test]
    fn agent_binds_and_releases() {
        let s = MemServer::new(Ipv4Addr::new(10, 0, 2, 16), 3600);
        let log = Arc::clone(&s.log);
        let rec = Rec::default();
        let a = Agent::spawn_on(Box::new(s), Config::new(MAC), Box::new(rec.clone()));
        let l = a.wait_bound(Duration::from_secs(5)).expect("bound");
        assert_eq!(l.addr, Ipv4Addr::new(10, 0, 2, 16));
        assert_eq!(a.status().state, "BOUND");
        a.stop(true);
        assert_eq!(
            *rec.0.lock().unwrap(),
            ["bind 10.0.2.16/24", "unbind 10.0.2.16"]
        );
        assert_eq!(log.lock().unwrap().seen.last(), Some(&Kind::Release));
    }

    #[test]
    fn agent_keeps_the_lease_on_stop() {
        let s = MemServer::new(Ipv4Addr::new(10, 0, 2, 17), 3600);
        let log = Arc::clone(&s.log);
        let rec = Rec::default();
        let a = Agent::spawn_on(Box::new(s), Config::new(MAC), Box::new(rec.clone()));
        a.wait_bound(Duration::from_secs(5)).expect("bound");
        a.stop(false);
        assert_eq!(
            rec.0.lock().unwrap().last().map(String::as_str),
            Some("unbind 10.0.2.17")
        );
        assert_ne!(log.lock().unwrap().seen.last(), Some(&Kind::Release));
    }

    #[test]
    fn no_server() {
        struct Silent;
        impl Wire for Silent {
            fn send(&self, _: Dest, _: &[u8]) -> io::Result<()> {
                Ok(())
            }
            fn recv(&self, wait: Duration) -> io::Result<Option<Vec<u8>>> {
                std::thread::sleep(wait.min(Duration::from_millis(20)));
                Ok(None)
            }
        }
        let e = acquire_on(
            &Silent,
            Config::new(MAC),
            Duration::from_millis(200),
            "eth9",
        )
        .unwrap_err();
        assert!(e.to_string().contains("eth9"));
    }
}
