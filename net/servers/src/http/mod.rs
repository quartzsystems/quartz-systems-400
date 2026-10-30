// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The minimal HTTP/1.1 administration listener (`STRTCPSVR *HTTP
//! HTTPSVR(*ADMIN)`): one request a connection, `GET` and `HEAD` of the
//! administration page, a short error page otherwise, then the
//! connection closes. The full HTTP server replaces this after Phase 12;
//! [`Admin`] is the [`Service`] its listener job runs on 2001 and, with
//! TLS from the certificate store's application ID, 2005.

pub mod date;
pub mod page;

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use certs::apps::AppRegistry;
use certs::store::CertStore;
use tls::config::{AppConfig, for_app};
use tls::policy::TlsPolicy;
use tls::rustls::ServerConfig;

use crate::listen::{Conn, Service};
pub use page::SiteInfo;

/// The certificate application ID of the administration instance.
pub const APP_ID: &str = "QIBM_HTTP_SERVER_ADMIN";

/// The most a request head may take.
const HEAD_MAX: usize = 16 * 1024;
/// How long a client has to send its request.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A request line and its headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestHead {
    /// `GET`, `HEAD`, ...
    pub method: String,
    /// The request target.
    pub target: String,
    /// `HTTP/1.1`.
    pub version: String,
    /// Header names (lower case) and values.
    pub headers: Vec<(String, String)>,
}

/// Why a request head could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadError {
    /// The connection ended or timed out first.
    Gone,
    /// Longer than [`HEAD_MAX`].
    TooLarge,
    /// Not HTTP.
    Malformed,
}

/// Read a request head from `r`: up to the blank line.
pub fn read_head(r: &mut dyn Read) -> Result<RequestHead, HeadError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let end = loop {
        if let Some(at) = find_end(&buf) {
            break at;
        }
        if buf.len() > HEAD_MAX {
            return Err(HeadError::TooLarge);
        }
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(HeadError::Gone),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    parse_head(&buf[..end])
}

fn find_end(b: &[u8]) -> Option<usize> {
    b.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .or_else(|| b.windows(2).position(|w| w == b"\n\n"))
}

/// Parse a request head (without the blank line).
pub fn parse_head(b: &[u8]) -> Result<RequestHead, HeadError> {
    let text = std::str::from_utf8(b).map_err(|_| HeadError::Malformed)?;
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let first = lines.next().ok_or(HeadError::Malformed)?;
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HeadError::Malformed);
    };
    if method.is_empty() || !method.bytes().all(|c| c.is_ascii_uppercase()) || !version.starts_with("HTTP/") {
        return Err(HeadError::Malformed);
    }
    let mut headers = Vec::new();
    for l in lines {
        if l.is_empty() {
            continue;
        }
        let (k, v) = l.split_once(':').ok_or(HeadError::Malformed)?;
        if k.is_empty() || k.contains(' ') {
            return Err(HeadError::Malformed);
        }
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    Ok(RequestHead {
        method: method.to_string(),
        target: target.to_string(),
        version: version.to_string(),
        headers,
    })
}

/// A response: status, extra headers, body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// Its reason phrase.
    pub reason: &'static str,
    /// Headers besides the ones every response has.
    pub headers: Vec<(&'static str, String)>,
    /// The body (empty for `HEAD`, which keeps the length).
    pub body: String,
    /// Whether to leave the body out.
    pub head_only: bool,
}

impl Response {
    fn page(status: u16, reason: &'static str, body: String) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body,
            head_only: false,
        }
    }

    fn error(status: u16, reason: &'static str, says: &str) -> Self {
        Self::page(status, reason, page::error(status, reason, says))
    }

    /// The bytes on the wire.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {} {}\r\nDate: {}\r\nServer: QS400/{}\r\nContent-Type: text/html; charset=utf-8\r\n\
             Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
            self.status,
            self.reason,
            date::now(),
            release::RELEASE,
            self.body.len()
        );
        for (k, v) in &self.headers {
            let _ = write!(out, "{k}: {v}\r\n");
        }
        out.push_str("\r\n");
        let mut b = out.into_bytes();
        if !self.head_only {
            b.extend_from_slice(self.body.as_bytes());
        }
        b
    }
}

/// The answer to a request.
pub fn respond(info: &SiteInfo, req: &RequestHead) -> Response {
    if req.version != "HTTP/1.1" && req.version != "HTTP/1.0" {
        return Response::error(505, "HTTP Version Not Supported", "This server speaks HTTP/1.1 and HTTP/1.0.");
    }
    let head_only = req.method == "HEAD";
    if req.method != "GET" && !head_only {
        let mut r = Response::error(405, "Method Not Allowed", "Only pages can be read here.");
        r.headers.push(("Allow", "GET, HEAD".into()));
        return r;
    }
    let path = req.target.split(['?', '#']).next().unwrap_or("");
    let mut r = match path {
        "/" | "/index.html" | "/HTTPAdmin" | "/HTTPAdmin/" => Response::page(200, "OK", page::admin(info)),
        _ => Response::error(404, "Not Found", "There is no such page on the administration server."),
    };
    r.head_only = head_only;
    r
}

/// The administration instance as a listener's service.
pub struct Admin {
    info: SiteInfo,
}

impl Admin {
    /// Serving the page for `info`.
    pub fn new(info: SiteInfo) -> Arc<Self> {
        Arc::new(Self { info })
    }

    /// Answer one request on `conn` and close it.
    pub fn answer(&self, conn: &mut Conn) {
        let _ = conn.tcp().set_read_timeout(Some(READ_TIMEOUT));
        let r = match read_head(conn) {
            Ok(req) => respond(&self.info, &req),
            Err(HeadError::Gone) => return,
            Err(HeadError::TooLarge) => {
                Response::error(431, "Request Header Fields Too Large", "The request was too long.")
            }
            Err(HeadError::Malformed) => Response::error(400, "Bad Request", "The request was not understood."),
        };
        let _ = conn.write_all(&r.encode());
        let _ = conn.flush();
    }
}

impl Service for Admin {
    fn serve(&self, mut conn: Conn, _peer: SocketAddr) {
        self.answer(&mut conn);
        conn.close();
    }
}

/// The TLS configuration of the administration instance: its
/// application ID's certificate in the `*SYSTEM` store (the store's
/// default when none is assigned).
pub fn admin_tls(store: &CertStore, apps: &AppRegistry, policy: &TlsPolicy, now: u64) -> Result<Arc<ServerConfig>, String> {
    match for_app(store, apps, APP_ID, policy, now).map_err(|e| e.to_string())? {
        AppConfig::Server { config, .. } => Ok(config),
        AppConfig::Client { .. } => Err(format!("{APP_ID} is not a server application")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(s: &str) -> RequestHead {
        parse_head(s.as_bytes()).unwrap()
    }

    #[test]
    fn parses_and_routes() {
        let info = SiteInfo::new("QS400");
        let r = head("GET / HTTP/1.1\r\nHost: x:2001\r\nUser-Agent: t");
        assert_eq!(r.headers[0], ("host".into(), "x:2001".into()));
        assert_eq!(respond(&info, &r).status, 200);
        assert_eq!(respond(&info, &head("GET /HTTPAdmin?x=1 HTTP/1.0")).status, 200);
        assert_eq!(respond(&info, &head("GET /nope HTTP/1.1")).status, 404);
        let post = respond(&info, &head("POST / HTTP/1.1"));
        assert_eq!(post.status, 405);
        assert!(String::from_utf8(post.encode()).unwrap().contains("Allow: GET, HEAD\r\n"));
        assert_eq!(respond(&info, &head("GET / HTTP/2.0")).status, 505);
        let h = respond(&info, &head("HEAD / HTTP/1.1"));
        let wire = String::from_utf8(h.encode()).unwrap();
        assert!(wire.ends_with("\r\n\r\n"));
        assert!(wire.contains(&format!("Content-Length: {}\r\n", h.body.len())));
        assert_eq!(parse_head(b"GET /"), Err(HeadError::Malformed));
        assert_eq!(parse_head(b"get / HTTP/1.1"), Err(HeadError::Malformed));
        assert_eq!(parse_head(b"GET / HTTP/1.1\r\nno colon"), Err(HeadError::Malformed));
    }

    #[test]
    fn head_limits() {
        let mut long = b"GET / HTTP/1.1\r\nX: ".to_vec();
        long.extend(std::iter::repeat_n(b'a', HEAD_MAX + 10));
        assert_eq!(read_head(&mut &long[..]), Err(HeadError::TooLarge));
        assert_eq!(read_head(&mut &b"GET / HTTP/1.1\r\n"[..]), Err(HeadError::Gone));
        assert!(read_head(&mut &b"GET / HTTP/1.1\n\n"[..]).is_ok());
    }
}
