// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A blocking LDAP client: LDAPS, `StartTLS`, operations by message ID,
//! controls, paging and sorting (docs/DIRSRV.md §3).
//!
//! [`Client::send`] and [`Client::poll`] are the asynchronous face: send
//! a request, then collect its responses by message ID. The synchronous
//! operations ([`ops`]) are built on them. Referrals are returned, not
//! followed, unless [`Options::referrals`] is set; a chase binds to the
//! next server with what the rebind hook gives ([`referral`]).

pub mod ops;
pub mod referral;
mod stream;

use std::collections::{HashSet, VecDeque};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tls::rustls::ClientConfig;
use tls::stream::TlsStream;

use crate::ber::{BerError, frame_len};
use crate::control::Control;
use crate::ext;
use crate::msg::{DerefAliases, LdapMessage, LdapResult, MsgError, ProtocolOp};
use crate::url::{LdapUrl, UrlError};

pub use ops::{Response, SearchEntry, SearchResult};
pub use referral::Rebind;
use stream::Stream;

/// What went wrong talking to a server.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The connection.
    #[error("LDAP connection: {0}")]
    Io(#[from] io::Error),
    /// TLS.
    #[error("LDAP TLS: {0}")]
    Tls(#[from] tls::TlsError),
    /// A message that does not decode.
    #[error("{0}")]
    Decode(#[from] MsgError),
    /// A frame that does not decode.
    #[error("bad LDAP frame: {0}")]
    Ber(#[from] BerError),
    /// No answer within the timeout.
    #[error("the LDAP server did not answer in time")]
    Timeout,
    /// The server closed the connection.
    #[error("the LDAP server closed the connection")]
    Closed,
    /// The server sent a notice of disconnection.
    #[error("the LDAP server disconnected: {}", .0.code.text())]
    Disconnected(LdapResult),
    /// The server answered with something else than the request needs.
    #[error("unexpected LDAP response: {0}")]
    Unexpected(&'static str),
    /// An operation that needed success got another result.
    #[error("LDAP operation failed: {} {}", .0.code.text(), .0.message)]
    Result(LdapResult),
    /// A bad URL (a referral, or the caller's).
    #[error("{0}")]
    Url(#[from] UrlError),
    /// Something the client does not do.
    #[error("not supported by the LDAP client: {0}")]
    Unsupported(&'static str),
    /// Referrals chased too far.
    #[error("too many LDAP referral hops")]
    HopLimit,
}

/// The result type of the client.
pub type Result<T> = std::result::Result<T, ClientError>;

/// How a client behaves.
#[derive(Clone)]
pub struct Options {
    /// How long to wait for a connection.
    pub connect_timeout: Duration,
    /// How long a synchronous operation waits for its result; none
    /// waits for ever.
    pub timeout: Option<Duration>,
    /// The size limit put in searches that ask for none.
    pub size_limit: i32,
    /// The time limit (seconds) put in searches that ask for none.
    pub time_limit: i32,
    /// The alias dereferencing put in searches built by the helpers.
    pub deref: DerefAliases,
    /// Whether referrals are followed (off, as IBM's
    /// `LDAP_OPT_REFERRALS` default for this client).
    pub referrals: bool,
    /// The most referral hops followed.
    pub hop_limit: usize,
    /// The longest message accepted.
    pub max_message: usize,
    /// The protocol version binds send.
    pub version: i32,
    /// The TLS configuration for `ldaps` referrals and `StartTLS`.
    pub tls: Option<Arc<ClientConfig>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            timeout: None,
            size_limit: 0,
            time_limit: 0,
            deref: DerefAliases::Never,
            referrals: false,
            hop_limit: 10,
            max_message: crate::ber::MAX_LEN,
            version: 3,
            tls: None,
        }
    }
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("timeout", &self.timeout)
            .field("size_limit", &self.size_limit)
            .field("time_limit", &self.time_limit)
            .field("referrals", &self.referrals)
            .field("tls", &self.tls.is_some())
            .finish_non_exhaustive()
    }
}

/// A connection to one server.
pub struct Client {
    stream: Option<Stream>,
    host: String,
    buf: Vec<u8>,
    next_id: i32,
    queued: VecDeque<LdapMessage>,
    abandoned: HashSet<i32>,
    closed: Option<LdapResult>,
    /// The client's options.
    pub options: Options,
    rebind: Option<Rebind>,
    hops: usize,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("host", &self.host)
            .field("tls", &self.is_tls())
            .field("next_id", &self.next_id)
            .finish_non_exhaustive()
    }
}

/// The addresses of `host`: `localhost` is the loopback whatever the
/// resolver says (RFC 6761 §6.3; a job has no `/etc/hosts`).
fn addresses(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>> {
    let h = host.trim_end_matches('.');
    if h.eq_ignore_ascii_case("localhost") {
        return Ok(vec![
            (std::net::Ipv4Addr::LOCALHOST, port).into(),
            (std::net::Ipv6Addr::LOCALHOST, port).into(),
        ]);
    }
    Ok((host, port).to_socket_addrs()?.collect())
}

fn tcp(host: &str, port: u16, timeout: Duration) -> Result<TcpStream> {
    let mut last = None;
    for addr in addresses(host, port)? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => {
                s.set_nodelay(true)?;
                return Ok(s);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(last.map_or(
        ClientError::Unexpected("the host has no address"),
        ClientError::Io,
    ))
}

impl Client {
    /// Connects to `host:port` in the clear.
    ///
    /// # Errors
    ///
    /// The connection's.
    pub fn connect(host: &str, port: u16, options: Options) -> Result<Self> {
        let s = tcp(host, port, options.connect_timeout)?;
        Ok(Self::from_tcp(s, host, options))
    }

    /// Connects to `host:port` over TLS (LDAPS) with `config`.
    ///
    /// # Errors
    ///
    /// The connection's or the handshake's.
    pub fn connect_tls(
        host: &str,
        port: u16,
        config: Arc<ClientConfig>,
        options: Options,
    ) -> Result<Self> {
        let s = tcp(host, port, options.connect_timeout)?;
        Self::tls_over(s, host, config, options)
    }

    /// A client over TLS on a connected socket; `name` is the server
    /// name the certificate is checked against.
    ///
    /// # Errors
    ///
    /// The handshake's.
    pub fn tls_over(
        s: TcpStream,
        name: &str,
        config: Arc<ClientConfig>,
        options: Options,
    ) -> Result<Self> {
        s.set_nodelay(true)?;
        s.set_read_timeout(Some(options.connect_timeout))?;
        let server = tls::config::server_name(name)?;
        let t = TlsStream::connect(config, server, s)?;
        t.get_ref().set_read_timeout(None)?;
        let mut c = Self::from_tcp_unchecked(name, options);
        c.stream = Some(Stream::Tls(Box::new(t)));
        Ok(c)
    }

    /// Connects to the server of `url` (`ldap` or `ldaps`; `ldaps`
    /// needs [`Options::tls`]).
    ///
    /// # Errors
    ///
    /// [`ClientError::Unsupported`] for `ldapi` or `ldaps` without a
    /// configuration; the connection's.
    pub fn connect_url(url: &LdapUrl, options: Options) -> Result<Self> {
        let host = url.host.clone().unwrap_or_else(|| "localhost".into());
        let port = url.port_or_default();
        match url.scheme.as_str() {
            "ldap" => Self::connect(&host, port, options),
            "ldaps" => {
                let cfg = options.tls.clone().ok_or(ClientError::Unsupported(
                    "ldaps without a TLS configuration",
                ))?;
                Self::connect_tls(&host, port, cfg, options)
            }
            _ => Err(ClientError::Unsupported("ldapi URLs")),
        }
    }

    /// A client over a connected socket; `host` names the server for
    /// `StartTLS`.
    #[must_use]
    pub fn from_tcp(s: TcpStream, host: &str, options: Options) -> Self {
        let mut c = Self::from_tcp_unchecked(host, options);
        c.stream = Some(Stream::Plain(s));
        c
    }

    fn from_tcp_unchecked(host: &str, options: Options) -> Self {
        Self {
            stream: None,
            host: host.to_string(),
            buf: Vec::new(),
            next_id: 1,
            queued: VecDeque::new(),
            abandoned: HashSet::new(),
            closed: None,
            options,
            rebind: None,
            hops: 0,
        }
    }

    /// Whether the connection is under TLS.
    #[must_use]
    pub fn is_tls(&self) -> bool {
        matches!(self.stream, Some(Stream::Tls(_)))
    }

    /// The host this client talks to.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Sets the hook asked for credentials when a referral is chased
    /// (the C API's `ldap_set_rebind_proc`).
    pub fn set_rebind(&mut self, hook: Rebind) {
        self.rebind = Some(hook);
    }

    /// Upgrades the connection with `StartTLS`. Nothing may be
    /// outstanding.
    ///
    /// # Errors
    ///
    /// [`ClientError::Result`] if the server refuses, or the handshake's.
    pub fn start_tls(&mut self, config: Arc<ClientConfig>) -> Result<()> {
        if self.is_tls() {
            return Err(ClientError::Unsupported("StartTLS on a TLS connection"));
        }
        let id = self.send(ext::start_tls(), Vec::new())?;
        let m = self.wait(id)?;
        let ProtocolOp::ExtendedResponse { result, .. } = m.op else {
            return Err(ClientError::Unexpected(
                "StartTLS needs an extended response",
            ));
        };
        if !result.is_success() {
            return Err(ClientError::Result(result));
        }
        if !self.buf.is_empty() {
            return Err(ClientError::Unexpected("data after the StartTLS response"));
        }
        let Some(Stream::Plain(s)) = self.stream.take() else {
            return Err(ClientError::Closed);
        };
        s.set_read_timeout(Some(self.options.connect_timeout))?;
        let name = tls::config::server_name(&self.host)?;
        let t = TlsStream::connect(config, name, s)?;
        t.get_ref().set_read_timeout(None)?;
        self.stream = Some(Stream::Tls(Box::new(t)));
        Ok(())
    }

    fn stream(&mut self) -> Result<&mut Stream> {
        if let Some(r) = &self.closed {
            return Err(ClientError::Disconnected(r.clone()));
        }
        self.stream.as_mut().ok_or(ClientError::Closed)
    }

    /// Sends `op` with `controls` and returns its message ID.
    ///
    /// # Errors
    ///
    /// The connection's.
    pub fn send(&mut self, op: ProtocolOp, controls: Vec<Control>) -> Result<i32> {
        let id = self.next_id;
        self.next_id = if id == crate::msg::MAX_ID { 1 } else { id + 1 };
        let m = LdapMessage { id, op, controls };
        tracing::trace!(id, tag = m.op.tag(), "ldap send");
        let bytes = m.encode();
        let s = self.stream()?;
        s.write_all(&bytes)?;
        s.flush()?;
        Ok(id)
    }

    /// Abandons `id`: the server is told, and anything still coming for
    /// it is dropped.
    ///
    /// # Errors
    ///
    /// The connection's.
    pub fn abandon(&mut self, id: i32) -> Result<()> {
        self.queued.retain(|m| m.id != id);
        self.abandoned.insert(id);
        self.send(ProtocolOp::AbandonRequest(id), Vec::new())?;
        Ok(())
    }

    /// The next message for `id` (any ID when none), waiting at most
    /// `timeout` (none: for ever; zero: only what has arrived). None if
    /// nothing came in time.
    ///
    /// # Errors
    ///
    /// [`ClientError::Disconnected`] on a notice of disconnection, the
    /// connection's, or a message that does not decode.
    pub fn poll(
        &mut self,
        id: Option<i32>,
        timeout: Option<Duration>,
    ) -> Result<Option<LdapMessage>> {
        if let Some(p) = self
            .queued
            .iter()
            .position(|m| id.is_none_or(|i| m.id == i))
        {
            return Ok(self.queued.remove(p));
        }
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            match self.next_message(deadline)? {
                None => return Ok(None),
                Some(m) => {
                    if let Some(r) = ext::as_notice_of_disconnection(&m) {
                        self.closed = Some(r.clone());
                        self.stream = None;
                        return Err(ClientError::Disconnected(r.clone()));
                    }
                    if self.abandoned.contains(&m.id) {
                        continue;
                    }
                    if id.is_none_or(|i| m.id == i) {
                        return Ok(Some(m));
                    }
                    self.queued.push_back(m);
                }
            }
        }
    }

    /// The next message for `id`, within [`Options::timeout`].
    ///
    /// # Errors
    ///
    /// [`ClientError::Timeout`], or what [`Client::poll`] returns.
    pub fn wait(&mut self, id: i32) -> Result<LdapMessage> {
        let t = self.options.timeout;
        self.poll(Some(id), t)?.ok_or(ClientError::Timeout)
    }

    /// Reads one message off the wire, none past `deadline`.
    fn next_message(&mut self, deadline: Option<Instant>) -> Result<Option<LdapMessage>> {
        loop {
            if let Some(n) = frame_len(&self.buf, self.options.max_message)? {
                let m = LdapMessage::decode_with_limit(&self.buf[..n], self.options.max_message);
                self.buf.drain(..n);
                let m = m?;
                tracing::trace!(id = m.id, tag = m.op.tag(), "ldap receive");
                return Ok(Some(m));
            }
            let wait = match deadline {
                None => None,
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        // One look at what is already there.
                        Some(Duration::from_millis(1))
                    } else {
                        Some(left)
                    }
                }
            };
            let s = self.stream()?;
            s.tcp().set_read_timeout(wait)?;
            let mut tmp = [0u8; 16 * 1024];
            match s.read(&mut tmp) {
                Ok(0) => {
                    self.stream = None;
                    return Err(ClientError::Closed);
                }
                Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    if deadline.is_some_and(|d| Instant::now() >= d) {
                        return Ok(None);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Sends an unbind and closes the connection.
    ///
    /// # Errors
    ///
    /// The connection's.
    pub fn unbind(mut self) -> Result<()> {
        if self.stream.is_some() && self.closed.is_none() {
            self.send(ProtocolOp::UnbindRequest, Vec::new())?;
            if let Some(Stream::Tls(t)) = &mut self.stream {
                let _ = t.close();
            }
        }
        self.stream = None;
        Ok(())
    }
}
