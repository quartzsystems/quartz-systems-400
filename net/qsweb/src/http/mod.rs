// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTP/1.1 with keep-alive: requests, forms and uploads, responses,
//! and the loop that serves one connection.

pub mod date;
pub mod form;
pub mod guard;
pub mod multipart;
pub mod request;
pub mod response;

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub use form::Form;
pub use request::{ReadError, Request};
pub use response::{Body, Response};

/// Requests one connection may carry.
const MAX_REQUESTS: u32 = 200;

/// Who is on the other end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Peer {
    /// The client's address.
    pub addr: SocketAddr,
    /// Whether the connection is TLS.
    pub secure: bool,
}

/// What answers requests.
pub trait Handler: Send + Sync {
    /// The response to `req`.
    fn handle(&self, req: &Request, peer: &Peer) -> Response;
}

/// Serve requests on `conn` until the client closes, asks to close, or
/// errs. The caller sets the read timeout and closes the socket.
pub fn serve<C: Read + Write>(conn: &mut C, peer: &Peer, handler: &dyn Handler) {
    serve_with(conn, &|_| {}, &mut |req| (handler.handle(&req, peer), true));
}

/// [`serve`], each request on a thread of its own ([`guard`]): a panic
/// answers that request with an error page, closes the connection and
/// leaves the process running.
pub fn serve_guarded<C: Read + Write>(conn: &mut C, peer: &Peer, handler: &Arc<dyn Handler>) {
    serve_guarded_timed(conn, peer, handler, &|_| {});
}

/// [`serve_guarded`] with `timeout` setting the connection's read
/// timeout, for a WebSocket the connection is upgraded to
/// ([`crate::ws`]).
pub fn serve_guarded_timed<C: Read + Write>(
    conn: &mut C,
    peer: &Peer,
    handler: &Arc<dyn Handler>,
    timeout: &dyn Fn(Option<Duration>),
) {
    serve_with(conn, timeout, &mut |req| {
        let (method, path) = (req.method.clone(), req.path.clone());
        match guard::handle(handler, req, *peer) {
            Ok(r) => (r, true),
            Err(e) => {
                eprintln!("qsweb: {method} {path} from {}: {e}", peer.addr);
                (guard::failed(), false)
            }
        }
    });
}

/// The loop: `respond` answers a request and says whether the
/// connection may carry another.
fn serve_with<C: Read + Write>(
    conn: &mut C,
    timeout: &dyn Fn(Option<Duration>),
    respond: &mut dyn FnMut(Request) -> (Response, bool),
) {
    let mut pending = Vec::new();
    for n in 1..=MAX_REQUESTS {
        let req = match request::read_request(conn, &mut pending) {
            Ok(r) => r,
            Err(ReadError::Gone) => return,
            Err(e) => {
                let (status, says) = match e {
                    ReadError::HeadTooLarge => (431, "The request was too long."),
                    ReadError::BodyTooLarge => (413, "The request body was too large."),
                    _ => (400, "The request was not understood."),
                };
                let _ = plain(status, says).write(conn, false, false);
                return;
            }
        };
        if req.version != "HTTP/1.1" && req.version != "HTTP/1.0" {
            let _ = plain(505, "This server speaks HTTP/1.1 and HTTP/1.0.").write(conn, false, false);
            return;
        }
        let keep = req.keep_alive() && n < MAX_REQUESTS;
        let head_only = req.method == "HEAD";
        let (mut resp, more) = respond(req);
        if let response::Body::Upgrade(_) = resp.body {
            let response::Body::Upgrade(run) = std::mem::replace(&mut resp.body, Body::Bytes(Vec::new())) else {
                return;
            };
            let mut head = format!("HTTP/1.1 101 {}\r\n", response::reason(101));
            for (k, v) in &resp.headers {
                let _ = write!(head, "{k}: {v}\r\n");
            }
            head.push_str("Connection: Upgrade\r\n\r\n");
            if conn.write_all(head.as_bytes()).and_then(|()| conn.flush()).is_ok() {
                // Bytes read past the request are the protocol's.
                let mut c = Rewind {
                    pre: std::mem::take(&mut pending),
                    inner: conn,
                };
                run(&mut c, timeout);
            }
            return;
        }
        let keep = keep && more;
        if resp.write(conn, head_only, keep).is_err() || !keep {
            return;
        }
    }
}

/// A connection with bytes already read put back in front.
struct Rewind<'a, C> {
    pre: Vec<u8>,
    inner: &'a mut C,
}

impl<C: Read> Read for Rewind<'_, C> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pre.is_empty() {
            return self.inner.read(buf);
        }
        let n = buf.len().min(self.pre.len());
        buf[..n].copy_from_slice(&self.pre[..n]);
        self.pre.drain(..n);
        Ok(n)
    }
}

impl<C: Write> Write for Rewind<'_, C> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A short plain-text answer for requests that never reach a handler.
pub fn plain(status: u16, says: &str) -> Response {
    Response::new(status, "text/plain; charset=utf-8", format!("{says}\n").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    struct Echo;
    impl Handler for Echo {
        fn handle(&self, req: &Request, _: &Peer) -> Response {
            Response::html(200, format!("{} {}", req.method, req.path))
        }
    }

    struct Duplex {
        input: io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Duplex {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            self.input.read(b)
        }
    }
    impl Write for Duplex {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.output.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn keep_alive_then_close() {
        let mut d = Duplex {
            input: io::Cursor::new(b"GET /a HTTP/1.1\r\n\r\nHEAD /b HTTP/1.1\r\nConnection: close\r\n\r\nGET /c HTTP/1.1\r\n\r\n".to_vec()),
            output: Vec::new(),
        };
        let peer = Peer {
            addr: "127.0.0.1:1".parse().unwrap(),
            secure: false,
        };
        serve(&mut d, &peer, &Echo);
        let out = String::from_utf8(d.output).unwrap();
        assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 2);
        assert!(out.contains("GET /a"));
        assert!(!out.contains("HEAD /b") && !out.contains("/c"));
        assert!(out.contains("Connection: close"));
    }
}
