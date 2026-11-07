// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request and its redirects.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use tls::rustls::ClientConfig;
use tls::stream::TlsStream;

use crate::url::Url;
use crate::wire;
use crate::{Error, Result};

/// A request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
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
    /// Accept any server certificate.
    pub tolerate: bool,
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

enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
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

/// The client configuration: the shipped roots, or any certificate.
fn config(tolerate: bool) -> Result<Arc<ClientConfig>> {
    let mut roots = tls::rustls::RootCertStore::empty();
    for r in certs::roots::roots() {
        let _ = roots.add(tls::rustls::pki_types::CertificateDer::from(r.der.clone()));
    }
    let mode = if tolerate {
        tls::config::ServerAuthMode::Passthru
    } else {
        tls::config::ServerAuthMode::Full
    };
    Ok(tls::config::client_config(
        &tls::policy::TlsPolicy::opsys(),
        roots,
        None,
        mode,
    )?)
}

fn connect(url: &Url, timeout: Duration, tolerate: bool) -> Result<Conn> {
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
    let tcp = tcp.ok_or_else(|| {
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
    let name = tls::config::server_name(&url.host)?;
    Ok(Conn::Tls(Box::new(TlsStream::connect(
        config(tolerate)?,
        name,
        tcp,
    )?)))
}

/// Send `req`, following up to `req.redirects` redirects (a 303, and a
/// 301 or 302 of a `POST`, become a `GET`).
pub fn send(req: &Request) -> Result<Response> {
    let timeout = Duration::from_secs(u64::from(if req.timeout == 0 { 60 } else { req.timeout }));
    let mut url = Url::parse(&req.url)?;
    let mut method = req.method.clone();
    let mut body = req.body.clone();
    for _ in 0..=req.redirects {
        let mut headers = req.headers.clone();
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
        let mut c = connect(&url, timeout, req.tolerate)?;
        wire::write_request(
            &mut c,
            &method,
            &url.target,
            &url.authority(),
            &headers,
            &body,
        )?;
        let (status, headers, rbody) = wire::read_response(&mut c, &method)?;
        let location = headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("Location"))
            .map(|(_, v)| v.clone());
        match (status, location) {
            (301 | 302 | 303 | 307 | 308, Some(loc)) if req.redirects > 0 => {
                url = url.join(&loc)?;
                if status == 303 || (matches!(status, 301 | 302) && method == "POST") {
                    method = "GET".into();
                    body.clear();
                }
            }
            _ => {
                return Ok(Response {
                    status,
                    headers,
                    body: rbody,
                });
            }
        }
    }
    Err(Error::Redirects(req.redirects))
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
