// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One connection, on a thread of its own. Messages are framed and
//! decoded here and sent to the service as their bytes; the answers'
//! messages are written as they come. The job's bookkeeping:
//!
//! - a search is streamed: `search`, then `next` with the budget until
//!   the cursor is done, paused (paged results) or watching (persistent
//!   search). Between batches the socket is read without waiting, so an
//!   abandon (or unbind) of the search stops it (`close` of its ID);
//! - persistent searches are polled with `changes` while the read waits
//!   (every [`POLL`]), their entries written as they come;
//! - `StartTLS` is refused with protocolError (2) when the instance has
//!   no certificate for it, operationsError (1) under TLS or with
//!   searches outstanding; else the service answers and the handshake
//!   follows the success response;
//! - the idle time-out (a connection with persistent searches is never
//!   idle), the write time-out (a write that does not finish ends the
//!   connection), the job's end (a notice of disconnection,
//!   unavailable), a message that does not decode (a notice,
//!   protocolError).
//!
//! Size and time limits are the service's.

use std::collections::{BTreeSet, VecDeque};
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ldap::ResultCode;
use ldap::ext;
use ldap::msg::{LdapMessage, ProtocolOp};
use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

use super::reply;
use super::{
    Answer, Budget, Close, ConnInfo, Counters, Framer, JobSettings, Service, State, SvcError,
};

/// How long a read waits before the connection looks at its watches,
/// the idle time-out and the job's end.
pub const TICK: Duration = Duration::from_millis(200);
/// How often persistent searches are polled.
pub const POLL: Duration = Duration::from_millis(500);
/// How many changes one poll takes.
pub const POLL_MAX: u32 = 100;
/// How long a TLS handshake may take.
pub const HANDSHAKE: Duration = Duration::from_secs(30);

/// Why a connection ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// The client closed it, or a read or write failed.
    Closed,
    /// Unbind.
    Unbind,
    /// Not a message, or not a request: a notice, protocolError.
    Protocol,
    /// The job ends: a notice, unavailable.
    Stop,
    /// The service's link is gone: a notice, unavailable.
    Broken,
    /// Idle past `ibm-slapdIdleTimeOut`.
    Idle,
}

enum Io {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
}

impl Io {
    fn tcp(&self) -> Option<&TcpStream> {
        match self {
            Self::Plain(s) => Some(s),
            Self::Tls(t) => Some(t.get_ref()),
            Self::Gone => None,
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(t) => t.read(buf),
            Self::Gone => Ok(0),
        }
    }

    fn write_all(&mut self, b: &[u8]) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.write_all(b),
            Self::Tls(t) => t.write_all(b).and_then(|()| t.flush()),
            Self::Gone => Err(io::ErrorKind::NotConnected.into()),
        }
    }
}

fn waited(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

/// The subject DN of the client's certificate, when it sent one.
fn client_subject(t: &TlsStream<TcpStream>) -> Option<String> {
    let s = t.session()?;
    let der = s.peer_certs.first()?;
    certs::x509::CertInfo::from_der(der).ok().map(|i| i.subject)
}

/// What one connection is given.
pub struct Setup {
    /// The instance.
    pub instance: String,
    /// Accepted on the secure port: handshake first with this.
    pub ssl: Option<Arc<ServerConfig>>,
    /// `StartTLS` with this; `None`: refused with protocolError.
    pub start_tls: Option<Arc<ServerConfig>>,
    /// The job is ending.
    pub stop: Arc<AtomicBool>,
    /// The job's counts.
    pub counters: Arc<Counters>,
}

/// One connection's state.
pub struct Connection<S: Service> {
    svc: S,
    io: Io,
    framer: Framer,
    pending: VecDeque<(Vec<u8>, LdapMessage)>,
    info: ConnInfo,
    session: u64,
    settings: JobSettings,
    start_tls: Option<Arc<ServerConfig>>,
    watching: BTreeSet<i32>,
    paused: BTreeSet<i32>,
    stop: Arc<AtomicBool>,
    counters: Arc<Counters>,
    last: Instant,
    polled: Instant,
}

impl<S: Service> Connection<S> {
    /// Serve `stream` until it ends; why it ended.
    pub fn serve(svc: S, stream: TcpStream, setup: Setup) -> End {
        let counters = Arc::clone(&setup.counters);
        counters.opened(setup.ssl.is_some());
        let end = Self::run(svc, stream, setup);
        counters.closed();
        end
    }

    fn run(mut svc: S, stream: TcpStream, setup: Setup) -> End {
        let client = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        let _ = stream.set_nodelay(true);
        let mut info = ConnInfo {
            client,
            ..ConnInfo::default()
        };
        let io = if let Some(cfg) = &setup.ssl {
            let _ = stream.set_read_timeout(Some(HANDSHAKE));
            let _ = stream.set_write_timeout(Some(HANDSHAKE));
            match TlsStream::accept(Arc::clone(cfg), stream) {
                Ok(t) => {
                    info.tls = true;
                    info.client_subject = client_subject(&t);
                    Io::Tls(Box::new(t))
                }
                Err(e) => {
                    tracing::debug!("LDAP {}: TLS handshake: {e}", info.client);
                    return End::Closed;
                }
            }
        } else {
            Io::Plain(stream)
        };
        let opened = match svc.open(&setup.instance, &info) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("LDAP {}: open: {e}", info.client);
                let mut io = io;
                let _ = io.write_all(&reply::notice(ResultCode::Unavailable, &e.text));
                return End::Broken;
            }
        };
        let settings = opened.settings;
        let write = (settings.write_timeout > 0)
            .then(|| Duration::from_secs(u64::from(settings.write_timeout)));
        if let Some(t) = io.tcp() {
            let _ = t.set_write_timeout(write);
        }
        let mut c = Self {
            svc,
            io,
            framer: Framer::new(settings.max_len()),
            pending: VecDeque::new(),
            info,
            session: opened.session,
            settings,
            start_tls: setup.start_tls,
            watching: BTreeSet::new(),
            paused: BTreeSet::new(),
            stop: setup.stop,
            counters: setup.counters,
            last: Instant::now(),
            polled: Instant::now(),
        };
        let end = c.main();
        c.finish(end);
        end
    }

    fn main(&mut self) -> End {
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return End::Stop;
            }
            if let Some((raw, m)) = self.pending.pop_front() {
                self.last = Instant::now();
                if let Err(e) = self.handle(&raw, &m) {
                    return e;
                }
                continue;
            }
            match self.read_some() {
                Ok(true) => {}
                Ok(false) => {
                    if let Err(e) = self.tick() {
                        return e;
                    }
                }
                Err(e) => return e,
            }
        }
    }

    /// A read that waits up to [`TICK`]: whether bytes came.
    fn read_some(&mut self) -> Result<bool, End> {
        if let Some(t) = self.io.tcp() {
            let _ = t.set_read_timeout(Some(TICK));
        }
        let mut tmp = [0u8; 16 * 1024];
        match self.io.read(&mut tmp) {
            Ok(0) => Err(End::Closed),
            Ok(n) => {
                self.framer.push(&tmp[..n]);
                self.frames()?;
                Ok(true)
            }
            Err(e) if waited(&e) => Ok(false),
            Err(_) => Err(End::Closed),
        }
    }

    /// Read what has come without waiting (between a search's batches).
    fn pump(&mut self) -> Result<(), End> {
        let Some(t) = self.io.tcp() else {
            return Err(End::Closed);
        };
        let _ = t.set_nonblocking(true);
        let mut tmp = [0u8; 16 * 1024];
        let r = loop {
            match self.io.read(&mut tmp) {
                Ok(0) => break Err(End::Closed),
                Ok(n) => self.framer.push(&tmp[..n]),
                Err(e) if waited(&e) => break Ok(()),
                Err(_) => break Err(End::Closed),
            }
        };
        if let Some(t) = self.io.tcp() {
            let _ = t.set_nonblocking(false);
        }
        r?;
        self.frames()
    }

    /// Whole messages from the framer to the queue.
    fn frames(&mut self) -> Result<(), End> {
        loop {
            let raw = match self.framer.next_frame() {
                Ok(Some(r)) => r,
                Ok(None) => return Ok(()),
                Err(e) => {
                    tracing::debug!("LDAP {}: {e}", self.info.client);
                    return Err(End::Protocol);
                }
            };
            match LdapMessage::decode_with_limit(&raw, self.settings.max_len()) {
                Ok(m) if m.op.is_request() => self.pending.push_back((raw, m)),
                Ok(_) => return Err(End::Protocol),
                Err(e) => {
                    tracing::debug!("LDAP {}: {e}", self.info.client);
                    return Err(End::Protocol);
                }
            }
        }
    }

    /// Nothing came for a tick: the watches, the idle time-out.
    fn tick(&mut self) -> Result<(), End> {
        if !self.watching.is_empty() && self.polled.elapsed() >= POLL {
            self.polled = Instant::now();
            let ch = self.svc.changes(self.session, POLL_MAX).or_else(|e| {
                if e.broken {
                    Err(End::Broken)
                } else {
                    Ok(super::Changes::default())
                }
            })?;
            for r in &ch.replies {
                self.write(r)?;
            }
            for id in ch.ended {
                if self.watching.remove(&id) {
                    self.write(&reply::search_ended(id))?;
                }
            }
        }
        let idle = self.settings.idle_timeout;
        if idle > 0
            && self.watching.is_empty()
            && self.last.elapsed() >= Duration::from_secs(u64::from(idle))
        {
            return Err(End::Idle);
        }
        Ok(())
    }

    fn write(&mut self, b: &[u8]) -> Result<(), End> {
        self.io.write_all(b).map_err(|e| {
            tracing::debug!("LDAP {}: write: {e}", self.info.client);
            End::Closed
        })
    }

    fn answer(&mut self, a: &Answer) -> Result<(), End> {
        for r in &a.replies {
            self.write(r)?;
        }
        Ok(())
    }

    /// A refused request: its failure response; the connection ends
    /// when the link is gone.
    fn refused(&mut self, m: &LdapMessage, e: &SvcError) -> Result<(), End> {
        tracing::warn!("LDAP {}: message {}: {e}", self.info.client, m.id);
        if let Some(b) = reply::failure(m, ResultCode::Other(80), &e.text) {
            self.write(&b)?;
        }
        if e.broken { Err(End::Broken) } else { Ok(()) }
    }

    fn handle(&mut self, raw: &[u8], m: &LdapMessage) -> Result<(), End> {
        match &m.op {
            ProtocolOp::UnbindRequest => Err(End::Unbind),
            ProtocolOp::AbandonRequest(id) => self.abandon(*id),
            ProtocolOp::BindRequest { .. } => {
                // A bind ends what the connection had outstanding.
                for id in std::mem::take(&mut self.watching)
                    .into_iter()
                    .chain(std::mem::take(&mut self.paused))
                {
                    self.close_search(id)?;
                }
                match self.svc.bind(self.session, raw, &self.info) {
                    Ok(a) => self.answer(&a),
                    Err(e) => self.refused(m, &e),
                }
            }
            ProtocolOp::SearchRequest(_) => self.search(raw, m),
            ProtocolOp::ExtendedRequest { name, .. } if name == ext::START_TLS => {
                self.start_tls(raw, m)
            }
            _ => match self.svc.op(self.session, raw) {
                Ok(a) => self.answer(&a),
                Err(e) => self.refused(m, &e),
            },
        }
    }

    fn close_search(&mut self, id: i32) -> Result<(), End> {
        match self.svc.close(self.session, Close::Message(id)) {
            Err(e) if e.broken => Err(End::Broken),
            _ => Ok(()),
        }
    }

    fn abandon(&mut self, id: i32) -> Result<(), End> {
        if self.watching.remove(&id) | self.paused.remove(&id) {
            self.close_search(id)?;
        }
        Ok(())
    }

    /// Whether search `id` is to stop: an abandon of it (taken from the
    /// queue) or an unbind (left there), or the job's end.
    fn stopped(&mut self, id: i32) -> bool {
        let before = self.pending.len();
        self.pending
            .retain(|(_, m)| !matches!(m.op, ProtocolOp::AbandonRequest(x) if x == id));
        before != self.pending.len()
            || self
                .pending
                .iter()
                .any(|(_, m)| matches!(m.op, ProtocolOp::UnbindRequest))
            || self.stop.load(Ordering::Relaxed)
    }

    fn search(&mut self, raw: &[u8], m: &LdapMessage) -> Result<(), End> {
        let budget = Budget::default();
        let mut a = match self.svc.search(self.session, raw, budget) {
            Ok(a) => a,
            Err(e) => return self.refused(m, &e),
        };
        loop {
            self.answer(&a)?;
            match a.state {
                State::Done => return Ok(()),
                State::Paused(_) => {
                    self.paused.insert(m.id);
                    return Ok(());
                }
                State::Watching(_) => {
                    self.watching.insert(m.id);
                    self.polled = Instant::now();
                    return Ok(());
                }
                State::More(cursor) => {
                    self.pump()?;
                    if self.stopped(m.id) {
                        return self.close_search(m.id);
                    }
                    a = match self.svc.next(self.session, cursor, budget) {
                        Ok(a) => a,
                        Err(e) => {
                            let _ = self.close_search(m.id);
                            return self.refused(m, &e);
                        }
                    };
                }
            }
        }
    }

    fn start_tls(&mut self, raw: &[u8], req: &LdapMessage) -> Result<(), End> {
        let refuse = |code, text: &str| reply::failure(req, code, text).unwrap_or_default();
        if matches!(self.io, Io::Tls(_)) {
            let refusal = refuse(ResultCode::OperationsError, "TLS is already in effect.");
            return self.write(&refusal);
        }
        let Some(cfg) = self.start_tls.clone() else {
            let refusal = refuse(
                ResultCode::ProtocolError,
                "StartTLS is not available: the server has no certificate for it.",
            );
            return self.write(&refusal);
        };
        if !self.watching.is_empty() || !self.paused.is_empty() || !self.pending.is_empty() {
            let refusal = refuse(
                ResultCode::OperationsError,
                "Operations are outstanding on the connection.",
            );
            return self.write(&refusal);
        }
        let answer = match self.svc.op(self.session, raw) {
            Ok(a) => a,
            Err(e) => return self.refused(req, &e),
        };
        self.answer(&answer)?;
        if !answer.start_tls {
            return Ok(());
        }
        if !self.framer.is_empty() || !self.pending.is_empty() {
            return Err(End::Protocol);
        }
        let Io::Plain(s) = std::mem::replace(&mut self.io, Io::Gone) else {
            return Err(End::Closed);
        };
        let _ = s.set_read_timeout(Some(HANDSHAKE));
        match TlsStream::accept(cfg, s) {
            Ok(t) => {
                self.info.tls = true;
                self.info.client_subject = client_subject(&t);
                self.io = Io::Tls(Box::new(t));
                self.counters.started_tls();
                Ok(())
            }
            Err(e) => {
                tracing::debug!("LDAP {}: StartTLS handshake: {e}", self.info.client);
                Err(End::Closed)
            }
        }
    }

    /// The notice the end deserves, TLS closed, the socket shut, the
    /// session closed.
    fn finish(&mut self, end: End) {
        let notice = match end {
            End::Protocol => Some((ResultCode::ProtocolError, "The request could not be read.")),
            End::Stop => Some((ResultCode::Unavailable, "The directory server is ending.")),
            End::Broken => Some((
                ResultCode::Unavailable,
                "The directory server cannot reach its data.",
            )),
            End::Closed | End::Unbind | End::Idle => None,
        };
        if let Some((code, text)) = notice {
            let _ = self.io.write_all(&reply::notice(code, text));
        }
        if let Io::Tls(t) = &mut self.io {
            let _ = t.close();
        }
        if let Some(t) = self.io.tcp() {
            let _ = t.shutdown(Shutdown::Both);
        }
        if end != End::Broken {
            let _ = self.svc.close(self.session, Close::Session);
        }
    }
}
