// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connections over sockets: the control connection (plain or TLS,
//! read a line at a time with telnet's `IAC` sequences dropped), data
//! connections accepted on a passive port (in the `DTACNNPORT` range
//! when one is set) or made to the client's `PORT`, TLS on them with
//! `PROT P`.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tls::rustls::ServerConfig;
use tls::stream::TlsStream;

use super::{DataConn, Net};
use crate::reply::Reply;

/// How long a passive listener waits for the client.
const ACCEPT_WAIT: Duration = Duration::from_secs(30);
/// How long an active connection may take.
const CONNECT_WAIT: Duration = Duration::from_secs(30);
/// The longest control line taken.
const MAX_LINE: usize = 8192;

/// The control connection.
enum Ctl {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
}

impl Ctl {
    fn tcp(&self) -> Option<&TcpStream> {
        match self {
            Ctl::Plain(s) => Some(s),
            Ctl::Tls(t) => Some(t.get_ref()),
            Ctl::Gone => None,
        }
    }
}

impl Read for Ctl {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Ctl::Plain(s) => s.read(buf),
            Ctl::Tls(t) => t.read(buf),
            Ctl::Gone => Ok(0),
        }
    }
}

impl Write for Ctl {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Ctl::Plain(s) => s.write(buf),
            Ctl::Tls(t) => t.write(buf),
            Ctl::Gone => Err(io::ErrorKind::NotConnected.into()),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Ctl::Plain(s) => s.flush(),
            Ctl::Tls(t) => t.flush(),
            Ctl::Gone => Ok(()),
        }
    }
}

/// A plain data connection.
struct PlainData(TcpStream);

impl Read for PlainData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for PlainData {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl DataConn for PlainData {
    fn finish(&mut self) -> io::Result<()> {
        self.0.flush()?;
        let _ = self.0.shutdown(Shutdown::Write);
        // Let the client read to the end before the socket goes.
        let _ = self.0.set_read_timeout(Some(Duration::from_secs(2)));
        let mut b = [0u8; 512];
        while matches!(self.0.read(&mut b), Ok(n) if n > 0) {}
        Ok(())
    }
}

/// A TLS data connection.
struct TlsData(Box<TlsStream<TcpStream>>);

impl Read for TlsData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
            // A client that closes without close_notify still sent all.
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(0),
            r => r,
        }
    }
}

impl Write for TlsData {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl DataConn for TlsData {
    fn finish(&mut self) -> io::Result<()> {
        self.0.flush()?;
        let _ = self.0.close();
        let _ = self.0.get_ref().shutdown(Shutdown::Write);
        Ok(())
    }
}

/// The connections of one session over sockets.
pub struct TcpNet {
    ctl: Ctl,
    inbuf: Vec<u8>,
    lines: VecDeque<String>,
    tls: Option<Arc<ServerConfig>>,
    private: bool,
    pasv: Option<TcpListener>,
    active: Option<SocketAddr>,
    ports: Option<(u16, u16)>,
    next_port: u16,
    abort: bool,
    peer: SocketAddr,
    local: SocketAddr,
}

impl TcpNet {
    /// Over an accepted control socket. `tls` is the server's TLS
    /// configuration (for `AUTH TLS` and `PROT P`); `implicit` does the
    /// handshake now (port 990); `ports` bounds passive ports.
    pub fn new(
        stream: TcpStream,
        tls: Option<Arc<ServerConfig>>,
        implicit: bool,
        ports: Option<(u16, u16)>,
    ) -> io::Result<Self> {
        let peer = stream.peer_addr()?;
        let local = stream.local_addr()?;
        let _ = stream.set_nodelay(true);
        let mut n = Self {
            ctl: Ctl::Plain(stream),
            inbuf: Vec::new(),
            lines: VecDeque::new(),
            tls,
            private: false,
            pasv: None,
            active: None,
            ports,
            next_port: ports.map_or(0, |(lo, _)| lo),
            abort: false,
            peer,
            local,
        };
        if implicit {
            n.start_tls()?;
            n.private = true;
        }
        Ok(n)
    }

    /// The next command line, `None` when the client went away. A
    /// client silent for `idle` ends with `TimedOut`.
    pub fn read_line(&mut self, idle: Option<Duration>) -> io::Result<Option<String>> {
        if let Some(l) = self.lines.pop_front() {
            return Ok(Some(l));
        }
        if let Some(t) = self.ctl.tcp() {
            t.set_read_timeout(idle)?;
        }
        loop {
            if let Some(l) = self.take_line() {
                return Ok(Some(l));
            }
            let mut buf = [0u8; 4096];
            let n = match self.ctl.read(&mut buf) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => 0,
                Err(e) => return Err(e),
            };
            if n == 0 {
                return Ok(None);
            }
            self.inbuf.extend_from_slice(&buf[..n]);
            if self.inbuf.len() > MAX_LINE && !self.inbuf.contains(&b'\n') {
                self.inbuf.clear();
                return Ok(Some(String::new()));
            }
        }
    }

    /// A whole line from the buffer: telnet `IAC` sequences out, the
    /// line end off, UTF-8 when it is valid, else Latin-1.
    fn take_line(&mut self) -> Option<String> {
        let at = self.inbuf.iter().position(|&b| b == b'\n')?;
        let raw: Vec<u8> = self.inbuf.drain(..=at).collect();
        let mut clean = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            match raw[i] {
                0xFF => {
                    // IAC: a command, or IAC with an option byte after.
                    match raw.get(i + 1) {
                        Some(&0xFF) => {
                            clean.push(0xFF);
                            i += 2;
                        }
                        Some(&c) if (0xFB..=0xFE).contains(&c) => i += 3,
                        Some(_) => i += 2,
                        None => i += 1,
                    }
                }
                b => {
                    clean.push(b);
                    i += 1;
                }
            }
        }
        while matches!(clean.last(), Some(b'\n' | b'\r')) {
            clean.pop();
        }
        let line = match String::from_utf8(clean) {
            Ok(s) => s,
            Err(e) => e.into_bytes().iter().map(|&b| char::from(b)).collect(),
        };
        Some(line)
    }

    /// The control connection's socket, for shutting it down.
    pub fn shutdown(&mut self) {
        if let Ctl::Tls(t) = &mut self.ctl {
            let _ = t.close();
        }
        if let Some(t) = self.ctl.tcp() {
            let _ = t.shutdown(Shutdown::Both);
        }
    }

    fn bind_passive(&mut self) -> io::Result<TcpListener> {
        let ip = self.local.ip();
        match self.ports {
            None => TcpListener::bind(SocketAddr::new(ip, 0)),
            Some((lo, hi)) => {
                let span = u32::from(hi - lo) + 1;
                let mut last = None;
                for _ in 0..span {
                    let p = self.next_port;
                    self.next_port = if p >= hi { lo } else { p + 1 };
                    match TcpListener::bind(SocketAddr::new(ip, p)) {
                        Ok(l) => return Ok(l),
                        Err(e) => last = Some(e),
                    }
                }
                Err(last.unwrap_or_else(|| io::ErrorKind::AddrInUse.into()))
            }
        }
    }

    fn accept(l: &TcpListener) -> io::Result<TcpStream> {
        l.set_nonblocking(true)?;
        let until = Instant::now() + ACCEPT_WAIT;
        loop {
            match l.accept() {
                Ok((s, _)) => {
                    s.set_nonblocking(false)?;
                    return Ok(s);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= until {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Look at the control connection without waiting: `ABOR` sets the
    /// flag, other lines wait their turn.
    fn poll_control(&mut self) {
        let Some(t) = self.ctl.tcp() else { return };
        if t.set_nonblocking(true).is_err() {
            return;
        }
        let mut buf = [0u8; 1024];
        loop {
            match self.ctl.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => self.inbuf.extend_from_slice(&buf[..n]),
            }
        }
        if let Some(t) = self.ctl.tcp() {
            let _ = t.set_nonblocking(false);
        }
        while let Some(l) = self.take_line() {
            if l.trim().eq_ignore_ascii_case("ABOR") {
                self.abort = true;
            } else {
                self.lines.push_back(l);
            }
        }
    }
}

impl Net for TcpNet {
    fn send(&mut self, r: &Reply) -> io::Result<()> {
        self.ctl.write_all(r.render().as_bytes())?;
        self.ctl.flush()
    }

    fn passive(&mut self, _extended: bool) -> io::Result<SocketAddr> {
        self.active = None;
        let l = self.bind_passive()?;
        let port = l.local_addr()?.port();
        self.pasv = Some(l);
        Ok(SocketAddr::new(self.local.ip(), port))
    }

    fn active(&mut self, to: SocketAddr) -> io::Result<()> {
        self.pasv = None;
        self.active = Some(to);
        Ok(())
    }

    fn data(&mut self) -> io::Result<Box<dyn DataConn>> {
        self.abort = false;
        let s = if let Some(l) = self.pasv.take() {
            Self::accept(&l)?
        } else if let Some(to) = self.active.take() {
            // No third-party transfers (RFC 2577).
            if to.ip() != self.peer.ip() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "data connections go only to the client's own address",
                ));
            }
            TcpStream::connect_timeout(&to, CONNECT_WAIT)?
        } else {
            return Err(io::Error::new(io::ErrorKind::NotConnected, "no PASV or PORT"));
        };
        let _ = s.set_nodelay(true);
        if self.private {
            let cfg = self
                .tls
                .clone()
                .ok_or_else(|| io::Error::other("no TLS configuration"))?;
            let t = TlsStream::accept(cfg, s).map_err(io::Error::other)?;
            return Ok(Box::new(TlsData(Box::new(t))));
        }
        Ok(Box::new(PlainData(s)))
    }

    fn tls_available(&self) -> bool {
        self.tls.is_some()
    }

    fn start_tls(&mut self) -> io::Result<()> {
        let cfg = self
            .tls
            .clone()
            .ok_or_else(|| io::Error::other("no TLS configuration"))?;
        match std::mem::replace(&mut self.ctl, Ctl::Gone) {
            Ctl::Plain(s) => {
                s.set_read_timeout(Some(Duration::from_secs(30)))?;
                let t = TlsStream::accept(cfg, s).map_err(io::Error::other)?;
                self.ctl = Ctl::Tls(Box::new(t));
                Ok(())
            }
            other => {
                self.ctl = other;
                Ok(())
            }
        }
    }

    fn end_tls(&mut self) -> io::Result<()> {
        match std::mem::replace(&mut self.ctl, Ctl::Gone) {
            Ctl::Tls(mut t) => {
                let _ = t.close();
                let (_, s) = t.into_parts();
                self.ctl = Ctl::Plain(s);
                Ok(())
            }
            other => {
                self.ctl = other;
                Ok(())
            }
        }
    }

    fn secure(&self) -> bool {
        matches!(self.ctl, Ctl::Tls(_))
    }

    fn set_private(&mut self, private: bool) {
        self.private = private;
    }

    fn aborted(&mut self) -> bool {
        if !self.abort {
            self.poll_control();
        }
        self.abort
    }

    fn peer(&self) -> SocketAddr {
        self.peer
    }

    fn local(&self) -> SocketAddr {
        self.local
    }
}

/// The address of a `PASV` reply, `h1,h2,h3,h4,p1,p2`; IPv6 is not
/// possible there.
pub fn pasv_text(a: SocketAddr) -> Option<String> {
    match a.ip() {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            Some(format!(
                "{},{},{},{},{},{}",
                o[0],
                o[1],
                o[2],
                o[3],
                a.port() >> 8,
                a.port() & 0xFF
            ))
        }
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(|v4| {
            let o = v4.octets();
            format!("{},{},{},{},{},{}", o[0], o[1], o[2], o[3], a.port() >> 8, a.port() & 0xFF)
        }),
    }
}
