// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's connections: the control connection (plain, after
//! `AUTH TLS`, or TLS from the first byte), replies read a line at a
//! time, and data connections, passive (`PASV`, `EPSV`) or active
//! (`PORT`), under TLS after `PROT P`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use tls::rustls::ClientConfig;
use tls::stream::TlsStream;

use crate::reply::{Reply, ReplyReader};

/// How long a connection may take.
const CONNECT_WAIT: Duration = Duration::from_secs(30);
/// How long a reply may take.
const REPLY_WAIT: Duration = Duration::from_secs(120);

/// A stream that reads and writes.
pub trait Stream: Read + Write + Send {}
impl<T: Read + Write + Send> Stream for T {}

enum Ctl {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    Gone,
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

/// How the next data connection is made.
pub enum DataSetup {
    /// Connect to the server's passive port.
    Connect(SocketAddr),
    /// The server connects to this listener.
    Listen(TcpListener),
}

/// The control connection.
pub struct Control {
    ctl: Ctl,
    pending: Vec<u8>,
    /// The server's address.
    pub peer: SocketAddr,
    /// The name connected to, for TLS.
    pub host: String,
    tls: Option<Arc<ClientConfig>>,
    /// Data connections under TLS (`PROT P`).
    pub private: bool,
}

fn tls_error(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

impl Control {
    /// Connect to `host` on `port`; TLS from the first byte with
    /// `implicit`.
    pub fn connect(host: &str, port: u16, tls: Option<Arc<ClientConfig>>, implicit: bool) -> io::Result<Self> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{host} has no address"));
        let mut found = None;
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, CONNECT_WAIT) {
                Ok(s) => {
                    found = Some((s, addr));
                    break;
                }
                Err(e) => last = e,
            }
        }
        let (s, addr) = found.ok_or(last)?;
        let _ = s.set_nodelay(true);
        s.set_read_timeout(Some(REPLY_WAIT))?;
        let mut c = Self {
            ctl: Ctl::Plain(s),
            pending: Vec::new(),
            peer: addr,
            host: host.to_string(),
            tls,
            private: false,
        };
        if implicit {
            c.start_tls()?;
            c.private = true;
        }
        Ok(c)
    }

    /// The TLS handshake on the control connection.
    pub fn start_tls(&mut self) -> io::Result<()> {
        let cfg = self.tls.clone().ok_or_else(|| tls_error("no TLS configuration"))?;
        let name = tls::config::server_name(&self.host).map_err(tls_error)?;
        match std::mem::replace(&mut self.ctl, Ctl::Gone) {
            Ctl::Plain(s) => {
                let t = TlsStream::connect(cfg, name, s).map_err(tls_error)?;
                self.ctl = Ctl::Tls(Box::new(t));
                Ok(())
            }
            other => {
                self.ctl = other;
                Ok(())
            }
        }
    }

    /// Send one command line.
    pub fn send(&mut self, line: &str) -> io::Result<()> {
        self.ctl.write_all(format!("{line}\r\n").as_bytes())?;
        self.ctl.flush()
    }

    /// Read one reply.
    pub fn reply(&mut self) -> io::Result<Reply> {
        let mut rr = ReplyReader::default();
        loop {
            let line = self.line()?;
            if let Some(r) = rr.push(&line) {
                return Ok(r);
            }
        }
    }

    /// Send a command and read its reply.
    pub fn command(&mut self, line: &str) -> io::Result<Reply> {
        self.send(line)?;
        self.reply()
    }

    fn line(&mut self) -> io::Result<String> {
        loop {
            if let Some(at) = self.pending.iter().position(|&b| b == b'\n') {
                let raw: Vec<u8> = self.pending.drain(..=at).collect();
                let s: String = String::from_utf8_lossy(&raw).trim_end_matches(['\r', '\n']).to_string();
                return Ok(s);
            }
            let mut buf = [0u8; 4096];
            let n = self.ctl.read(&mut buf)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
            }
            self.pending.extend_from_slice(&buf[..n]);
        }
    }

    /// Set up the next data connection: `PASV` (or `EPSV` for IPv6)
    /// when `passive`, else `PORT` (`EPRT`) to a listener of ours.
    pub fn setup(&mut self, passive: bool, log: &mut dyn FnMut(&Reply)) -> io::Result<DataSetup> {
        let local = match &self.ctl {
            Ctl::Plain(s) => s.local_addr()?,
            Ctl::Tls(t) => t.get_ref().local_addr()?,
            Ctl::Gone => return Err(io::ErrorKind::NotConnected.into()),
        };
        if passive {
            let v6 = self.peer.is_ipv6();
            let r = self.command(if v6 { "EPSV" } else { "PASV" })?;
            log(&r);
            let text = r.lines.join(" ");
            let addr = if v6 { epsv_addr(&text, self.peer.ip()) } else { pasv_addr(&text, self.peer.ip()) };
            return addr
                .map(DataSetup::Connect)
                .ok_or_else(|| io::Error::other(format!("no data address in reply {}", r.code)));
        }
        let l = TcpListener::bind(SocketAddr::new(local.ip(), 0))?;
        let a = l.local_addr()?;
        let cmd = match a.ip() {
            IpAddr::V4(v4) => {
                let o = v4.octets();
                format!("PORT {},{},{},{},{},{}", o[0], o[1], o[2], o[3], a.port() >> 8, a.port() & 0xFF)
            }
            IpAddr::V6(v6) => format!("EPRT |2|{v6}|{}|", a.port()),
        };
        let r = self.command(&cmd)?;
        log(&r);
        if r.code >= 400 {
            return Err(io::Error::other(format!("{} {}", r.code, r.lines.join(" "))));
        }
        Ok(DataSetup::Listen(l))
    }

    /// The data connection, once the transfer command was accepted.
    pub fn data(&self, setup: DataSetup) -> io::Result<Box<dyn Stream>> {
        let s = match setup {
            DataSetup::Connect(a) => TcpStream::connect_timeout(&a, CONNECT_WAIT)?,
            DataSetup::Listen(l) => {
                l.set_nonblocking(false)?;
                l.accept()?.0
            }
        };
        let _ = s.set_nodelay(true);
        s.set_read_timeout(Some(REPLY_WAIT))?;
        if self.private {
            let cfg = self.tls.clone().ok_or_else(|| tls_error("no TLS configuration"))?;
            let name = tls::config::server_name(&self.host).map_err(tls_error)?;
            let t = TlsStream::connect(cfg, name, s).map_err(tls_error)?;
            return Ok(Box::new(TlsData(t)));
        }
        Ok(Box::new(s))
    }

    /// Close the control connection.
    pub fn close(&mut self) {
        if let Ctl::Tls(t) = &mut self.ctl {
            let _ = t.close();
        }
        if let Ctl::Plain(s) = &self.ctl {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Ctl::Tls(t) = &self.ctl {
            let _ = t.get_ref().shutdown(Shutdown::Both);
        }
        self.ctl = Ctl::Gone;
    }
}

/// A TLS data connection: end of data without `close_notify` is still
/// the end.
struct TlsData(TlsStream<TcpStream>);

impl Read for TlsData {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.read(buf) {
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

impl Drop for TlsData {
    fn drop(&mut self) {
        let _ = self.0.close();
        // Unread input (a TLS 1.3 ticket) would make the close a reset and
        // cut the upload short: half-close, then drain until the server ends.
        let mut s = self.0.get_ref();
        let _ = s.shutdown(Shutdown::Write);
        let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
        let mut b = [0u8; 512];
        while matches!(s.read(&mut b), Ok(n) if n > 0) {}
    }
}

/// The address of a `227` reply; a private address the server cannot
/// mean for us is replaced by the control connection's.
pub fn pasv_addr(text: &str, ctl: IpAddr) -> Option<SocketAddr> {
    let open = text.find('(').map_or(0, |i| i + 1);
    let digits: String = text[open..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .collect();
    let n: Vec<u8> = digits.split(',').map(|p| p.parse::<u8>().ok()).collect::<Option<_>>()?;
    if n.len() != 6 {
        return None;
    }
    let port = u16::from(n[4]) << 8 | u16::from(n[5]);
    let ip = IpAddr::from([n[0], n[1], n[2], n[3]]);
    let ip = match (ip, ctl) {
        (IpAddr::V4(v), IpAddr::V4(c)) if (v.is_private() || v.is_unspecified()) && !c.is_private() && !c.is_loopback() => ctl,
        _ => ip,
    };
    Some(SocketAddr::new(ip, port))
}

/// The address of a `229` reply: the control connection's host.
pub fn epsv_addr(text: &str, ctl: IpAddr) -> Option<SocketAddr> {
    let start = text.find("(|||")? + 4;
    let end = text[start..].find('|')? + start;
    Some(SocketAddr::new(ctl, text[start..end].parse().ok()?))
}

/// Read everything a data connection carries.
pub fn read_all(s: &mut dyn Stream) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut r = BufReader::new(s);
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok(out);
        }
        let n = buf.len();
        out.extend_from_slice(buf);
        r.consume(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_addresses() {
        let ctl: IpAddr = "203.0.113.5".parse().unwrap();
        assert_eq!(
            pasv_addr("Entering Passive Mode (185,113,5,134,106,119).", ctl),
            Some("185.113.5.134:27255".parse().unwrap())
        );
        // A private address from a public server: its own address.
        assert_eq!(
            pasv_addr("Entering Passive Mode (10,0,2,15,4,1)", ctl),
            Some("203.0.113.5:1025".parse().unwrap())
        );
        assert_eq!(
            epsv_addr("Entering Extended Passive Mode (|||64476|)", ctl),
            Some("203.0.113.5:64476".parse().unwrap())
        );
        assert!(pasv_addr("nothing", ctl).is_none());
    }
}
