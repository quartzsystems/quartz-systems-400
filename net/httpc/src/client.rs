// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request and its redirects.

use std::io::{BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConnection, StreamOwned};

use crate::trust::Trust;
use crate::url::Url;
use crate::wire::{self, Head};
use crate::{Error, Result};

/// A request.
#[derive(Clone, Debug, Default)]
pub struct Request {
    /// `GET`, `POST` ...
    pub method: String,
    /// The URL.
    pub url: String,
    /// Headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
    /// Seconds to connect and to wait for each read; 0 is 60.
    pub timeout: u32,
    /// Redirects to follow.
    pub redirects: u32,
    /// What an `https://` connection trusts.
    pub trust: Trust,
    /// The `User-Agent` sent when `headers` have none; none when empty.
    pub agent: String,
}

/// A response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Headers in order.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: Vec<u8>,
}

/// A connection.
pub(crate) enum Conn {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

/// A TLS failure as the client reports it: rustls's own error when the
/// I/O error carries one.
fn tls_error(e: std::io::Error) -> Error {
    match e.get_ref().and_then(|i| i.downcast_ref::<rustls::Error>()) {
        Some(r) => Error::Tls(r.to_string()),
        None => Error::Io(e),
    }
}

fn connect(url: &Url, timeout: Duration, trust: &Trust) -> Result<Conn> {
    let mut last = None;
    let mut tcp = None;
    for addr in (url.host.as_str(), url.port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(s) => {
                tcp = Some(s);
                break;
            }
            Err(e) => last = Some(e),
        }
    }
    let mut tcp = tcp.ok_or_else(|| {
        last.map_or_else(
            || Error::Url(format!("{}: no address", url.host)),
            Error::Io,
        )
    })?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;
    if !url.tls {
        return Ok(Conn::Plain(tcp));
    }
    let name = ServerName::try_from(url.host.clone())
        .map_err(|_| Error::Tls(format!("server name {} is not valid", url.host)))?;
    let mut conn =
        ClientConnection::new(trust.config()?, name).map_err(|e| Error::Tls(e.to_string()))?;
    // The handshake now, so a refused certificate fails the connection.
    while conn.is_handshaking() {
        conn.complete_io(&mut tcp).map_err(tls_error)?;
    }
    Ok(Conn::Tls(Box::new(StreamOwned::new(conn, tcp))))
}

/// A response whose head is read; its body comes next on `reader`.
pub(crate) struct Opened {
    pub reader: BufReader<Conn>,
    pub head: Head,
    /// The method the last request was sent with.
    pub method: String,
}

/// Send `req` with `extra` headers, following up to `req.redirects`
/// redirects (a 303, and a 301 or 302 of a `POST`, become a `GET`).
pub(crate) fn open(req: &Request, extra: &[(String, String)]) -> Result<Opened> {
    let timeout = Duration::from_secs(u64::from(if req.timeout == 0 { 60 } else { req.timeout }));
    let mut url = Url::parse(&req.url)?;
    let mut method = req.method.clone();
    let mut body = req.body.clone();
    for _ in 0..=req.redirects {
        let mut headers = req.headers.clone();
        headers.extend(extra.iter().cloned());
        if let Some(u) = &url.userinfo
            && !headers
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("Authorization"))
        {
            headers.push((
                "Authorization".into(),
                format!("Basic {}", b64(u.as_bytes())),
            ));
        }
        let mut c = connect(&url, timeout, &req.trust)?;
        wire::write_request(
            &mut c,
            &method,
            &url.target,
            &url.authority(),
            &req.agent,
            &headers,
            &body,
        )?;
        let mut reader = BufReader::new(c);
        let head = wire::read_head(&mut reader)?;
        let location = wire::header(&head.1, "Location").map(str::to_string);
        match (head.0, location) {
            (301 | 302 | 303 | 307 | 308, Some(loc)) if req.redirects > 0 => {
                wire::read_body(&mut reader, &method, &head)?;
                url = url.join(&loc)?;
                if head.0 == 303 || (matches!(head.0, 301 | 302) && method == "POST") {
                    method = "GET".into();
                    body.clear();
                }
            }
            _ => {
                return Ok(Opened {
                    reader,
                    head,
                    method,
                });
            }
        }
    }
    Err(Error::Redirects(req.redirects))
}

/// Send `req` and read the whole answer, following up to
/// `req.redirects` redirects (a 303, and a 301 or 302 of a `POST`,
/// become a `GET`).
pub fn send(req: &Request) -> Result<Response> {
    let mut o = open(req, &[])?;
    let body = wire::read_body(&mut o.reader, &o.method, &o.head)?;
    Ok(Response {
        status: o.head.0,
        headers: o.head.1,
        body,
    })
}

fn b64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= c.len() {
                char::from(A[((n >> (18 - 6 * i)) & 63) as usize])
            } else {
                '='
            });
        }
    }
    out
}
