// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTP/1.1 with keep-alive: requests, forms and uploads, responses,
//! and the loop that serves one connection.
//!
//! The loop serves any `Read + Write`: a TCP socket, or a TLS stream
//! the caller brings (rustls' `StreamOwned` over the socket). It sets
//! no timeouts of its own: the caller sets the read and write timeouts
//! on the socket (they bound how long a request may take to arrive and
//! how long a connection may sit idle). What it enforces:
//!
//! - a request line and headers of at most [`request::HEAD_MAX`] bytes
//!   together (431 beyond);
//! - a body read whole of at most [`request::BODY_MAX`] bytes (413
//!   beyond), `Content-Length` or chunked; a multipart upload read
//!   whole is held to the same limit;
//! - a body the handler streams ([`Handler::streams`]) has no limit
//!   here: the handler reads it as it arrives ([`request::BodyStream`],
//!   [`multipart::Parts`]) and stops when it has had enough; a body
//!   left unread closes the connection after the response;
//! - at most 200 requests on one connection; HTTP/1.0 and 1.1 only.
//!
//! A large download is a [`Body::Stream`] (chunked) or a
//! [`Body::Sized`] (with `Content-Length`): either is read from the
//! handler's reader a piece at a time.

pub mod date;
pub mod form;
pub mod guard;
pub mod multipart;
pub mod range;
pub mod request;
pub mod response;

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub use form::Form;
pub use request::{BodyStream, ReadError, Request};
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

    /// The `Server` header's value
    /// ([`crate::brand::Brand::server_header`]).
    fn server(&self) -> String;

    /// Whether `req`, its head read and its body not, takes its body as
    /// a stream ([`Handler::handle_stream`]) instead of read whole.
    fn streams(&self, _req: &Request, _peer: &Peer) -> bool {
        false
    }

    /// The response to `req` (its `body` empty) whose body is `body`,
    /// read as it arrives.
    fn handle_stream(&self, req: &Request, _body: &mut dyn Read, peer: &Peer) -> Response {
        self.handle(req, peer)
    }
}

/// Serve requests on `conn` until the client closes, asks to close, or
/// errs. The caller sets the read timeout and closes the socket.
pub fn serve<C: Read + Write>(conn: &mut C, peer: &Peer, handler: &dyn Handler) {
    serve_with(
        conn,
        &|_| {},
        &mut Direct {
            handler,
            peer,
            server: handler.server(),
        },
    );
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
    serve_with(
        conn,
        timeout,
        &mut Guarded {
            handler,
            peer: *peer,
            server: handler.server(),
        },
    );
}

/// How the loop answers: a response and whether the connection may
/// carry another.
trait Respond {
    fn server(&self) -> &str;
    fn streams(&self, req: &Request) -> bool;
    fn respond(&mut self, req: Request) -> (Response, bool);
    fn respond_stream(&mut self, req: Request, body: &mut BodyStream<'_>) -> (Response, bool);
}

struct Direct<'a> {
    handler: &'a dyn Handler,
    peer: &'a Peer,
    server: String,
}

impl Respond for Direct<'_> {
    fn server(&self) -> &str {
        &self.server
    }
    fn streams(&self, req: &Request) -> bool {
        self.handler.streams(req, self.peer)
    }
    fn respond(&mut self, req: Request) -> (Response, bool) {
        (self.handler.handle(&req, self.peer), true)
    }
    fn respond_stream(&mut self, req: Request, body: &mut BodyStream<'_>) -> (Response, bool) {
        (self.handler.handle_stream(&req, body, self.peer), true)
    }
}

struct Guarded<'a> {
    handler: &'a Arc<dyn Handler>,
    peer: Peer,
    server: String,
}

impl Guarded<'_> {
    fn answer(&self, method: &str, path: &str, r: Result<Response, String>) -> (Response, bool) {
        match r {
            Ok(r) => (r, true),
            Err(e) => {
                eprintln!("qsweb: {method} {path} from {}: {e}", self.peer.addr);
                (guard::failed(), false)
            }
        }
    }
}

impl Respond for Guarded<'_> {
    fn server(&self) -> &str {
        &self.server
    }
    fn streams(&self, req: &Request) -> bool {
        self.handler.streams(req, &self.peer)
    }
    fn respond(&mut self, req: Request) -> (Response, bool) {
        let (method, path) = (req.method.clone(), req.path.clone());
        let r = guard::handle(self.handler, req, self.peer);
        self.answer(&method, &path, r)
    }
    fn respond_stream(&mut self, req: Request, body: &mut BodyStream<'_>) -> (Response, bool) {
        let (method, path) = (req.method.clone(), req.path.clone());
        let r = guard::handle_stream(self.handler, req, self.peer, body);
        self.answer(&method, &path, r)
    }
}

/// The loop.
fn serve_with<C: Read + Write>(
    conn: &mut C,
    timeout: &dyn Fn(Option<Duration>),
    respond: &mut dyn Respond,
) {
    let mut pending = Vec::new();
    let server = respond.server().to_string();
    for n in 1..=MAX_REQUESTS {
        let refuse = |conn: &mut C, e: ReadError| {
            let (status, says) = match e {
                ReadError::HeadTooLarge => (431, "The request was too long."),
                ReadError::BodyTooLarge => (413, "The request body was too large."),
                _ => (400, "The request was not understood."),
            };
            let _ = plain(status, says).write(conn, &server, false, false);
        };
        let mut req = match request::read_head(conn, &mut pending) {
            Ok(r) => r,
            Err(ReadError::Gone) => return,
            Err(e) => return refuse(conn, e),
        };
        let streamed = respond.streams(&req);
        if !streamed && let Err(e) = request::read_body(conn, &mut pending, &mut req) {
            if e != ReadError::Gone {
                refuse(conn, e);
            }
            return;
        }
        if req.version != "HTTP/1.1" && req.version != "HTTP/1.0" {
            let _ = plain(505, "This server speaks HTTP/1.1 and HTTP/1.0.")
                .write(conn, &server, false, false);
            return;
        }
        let keep = req.keep_alive() && n < MAX_REQUESTS;
        let head_only = req.method == "HEAD";
        let began = std::time::Instant::now();
        let (mut resp, more) = if streamed {
            let mut body = match BodyStream::new(conn, &mut pending, &req) {
                Ok(b) => b,
                Err(e) => return refuse(conn, e),
            };
            let (resp, more) = respond.respond_stream(req, &mut body);
            (resp, more && body.finished())
        } else {
            respond.respond(req)
        };
        // How long the page took here, for the browser's network panel.
        let ms = began.elapsed().as_secs_f64() * 1000.0;
        resp.headers
            .push(("Server-Timing".into(), format!("app;dur={ms:.1}")));
        if let response::Body::Upgrade(_) = resp.body {
            let response::Body::Upgrade(run) =
                std::mem::replace(&mut resp.body, Body::Bytes(Vec::new()))
            else {
                return;
            };
            let mut head = format!("HTTP/1.1 101 {}\r\n", response::reason(101));
            for (k, v) in &resp.headers {
                let _ = write!(head, "{k}: {v}\r\n");
            }
            head.push_str("Connection: Upgrade\r\n\r\n");
            if conn
                .write_all(head.as_bytes())
                .and_then(|()| conn.flush())
                .is_ok()
            {
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
        if resp.write(conn, &server, head_only, keep).is_err() || !keep {
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
    Response::new(
        status,
        "text/plain; charset=utf-8",
        format!("{says}\n").into_bytes(),
    )
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
        fn server(&self) -> String {
            "Echo/1".into()
        }
        fn streams(&self, req: &Request, _: &Peer) -> bool {
            req.method == "PUT"
        }
        fn handle_stream(&self, req: &Request, body: &mut dyn Read, _: &Peer) -> Response {
            // Reads the first 3 bytes only when the path says so.
            let mut got = Vec::new();
            if req.path == "/short" {
                let mut b = [0u8; 3];
                body.read_exact(&mut b).unwrap();
                got.extend_from_slice(&b);
            } else {
                body.read_to_end(&mut got).unwrap();
            }
            Response::html(200, format!("got {}", got.len()))
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
        assert!(out.contains("Server: Echo/1\r\n"));
    }

    #[test]
    fn streamed_uploads() {
        let peer = Peer {
            addr: "127.0.0.1:1".parse().unwrap(),
            secure: false,
        };
        let mut wire = b"PUT /all HTTP/1.1\r\nContent-Length: 100000\r\n\r\n".to_vec();
        wire.extend(std::iter::repeat_n(b'x', 100_000));
        wire.extend_from_slice(
            b"PUT /short HTTP/1.1\r\nContent-Length: 10\r\n\r\n0123456789GET /c HTTP/1.1\r\n\r\n",
        );
        for guarded in [false, true] {
            let mut d = Duplex {
                input: io::Cursor::new(wire.clone()),
                output: Vec::new(),
            };
            if guarded {
                let h: Arc<dyn Handler> = Arc::new(Echo);
                serve_guarded(&mut d, &peer, &h);
            } else {
                serve(&mut d, &peer, &Echo);
            }
            let out = String::from_utf8(d.output).unwrap();
            assert!(out.contains("got 100000"), "{out}");
            assert!(out.contains("got 3"), "{out}");
            // The short reader left the body: the connection closes
            // (guarded, the pump may have read it all before the
            // handler was done, and then the connection goes on).
            assert!(guarded || !out.contains("GET /c"), "{out}");
        }
    }
}
