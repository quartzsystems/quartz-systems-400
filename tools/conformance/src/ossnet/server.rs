// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The harness host's HTTP server: plain HTTP/1.1 on the loopback, one
//! thread per connection, one request per connection (`Connection:
//! close`). It answers from a fixed route table and keeps every request
//! it read, for the checks.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One request as the server read it.
#[derive(Clone, Debug, Default)]
pub struct Request {
    /// `GET`, `POST`, ...
    pub method: String,
    /// The path with its query.
    pub path: String,
    /// Headers, names lower case, in order.
    pub headers: Vec<(String, String)>,
    /// The body, de-chunked.
    pub body: Vec<u8>,
}

impl Request {
    /// A header's value.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// An answer: status, reason, headers and body.
pub struct Answer {
    /// Status code.
    pub status: u16,
    /// Reason phrase.
    pub reason: &'static str,
    /// Extra headers.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: Vec<u8>,
}

impl Answer {
    /// A 200 with a content type.
    #[must_use]
    pub fn ok(ctype: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            reason: "OK",
            headers: vec![("Content-Type".into(), ctype.into())],
            body,
        }
    }
}

/// What the server answers a request with.
pub type Routes = Arc<dyn Fn(&Request, u16) -> Answer + Send + Sync>;

/// A running server.
pub struct Server {
    /// Its port on 127.0.0.1.
    pub port: u16,
    /// Every request read, in order.
    pub seen: Arc<Mutex<Vec<Request>>>,
}

impl Server {
    /// Listen on an ephemeral loopback port and serve in the background.
    ///
    /// # Errors
    ///
    /// The listener cannot be made.
    pub fn start(routes: Routes) -> std::io::Result<Self> {
        let l = TcpListener::bind("127.0.0.1:0")?;
        let port = l.local_addr()?.port();
        let seen: Arc<Mutex<Vec<Request>>> = Arc::default();
        let kept = seen.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let (routes, kept) = (routes.clone(), kept.clone());
                std::thread::spawn(move || {
                    let _ = serve(s, &routes, &kept, port);
                });
            }
        });
        Ok(Self { port, seen })
    }

    /// The requests so far, and forget them.
    ///
    /// # Panics
    ///
    /// A serving thread panicked holding the list.
    #[must_use]
    pub fn take(&self) -> Vec<Request> {
        std::mem::take(&mut *self.seen.lock().expect("the request list"))
    }
}

fn serve(s: TcpStream, routes: &Routes, seen: &Mutex<Vec<Request>>, port: u16) -> std::io::Result<()> {
    s.set_read_timeout(Some(Duration::from_secs(20)))?;
    let mut w = s.try_clone()?;
    let mut r = BufReader::new(s);
    let Some(req) = read_request(&mut r)? else { return Ok(()) };
    let a = routes(&req, port);
    if let Ok(mut v) = seen.lock() {
        v.push(req);
    }
    let mut head = format!("HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n", a.status, a.reason, a.body.len());
    for (n, v) in &a.headers {
        let _ = write!(head, "{n}: {v}\r\n");
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(&a.body)?;
    w.flush()?;
    // Let the client read to the end before the close.
    let mut sink = [0u8; 512];
    let _ = w.set_read_timeout(Some(Duration::from_millis(200)));
    let _ = r.read(&mut sink);
    Ok(())
}

fn line(r: &mut impl BufRead) -> std::io::Result<Option<String>> {
    let mut l = String::new();
    if r.read_line(&mut l)? == 0 {
        return Ok(None);
    }
    Ok(Some(l.trim_end_matches(['\r', '\n']).to_string()))
}

/// One request: the line, headers, and a body by `Content-Length` or
/// chunked.
fn read_request(r: &mut impl BufRead) -> std::io::Result<Option<Request>> {
    let Some(first) = line(r)? else { return Ok(None) };
    let mut parts = first.split_whitespace();
    let mut req = Request {
        method: parts.next().unwrap_or("").to_string(),
        path: parts.next().unwrap_or("").to_string(),
        ..Request::default()
    };
    while let Some(h) = line(r)? {
        if h.is_empty() {
            break;
        }
        if let Some((n, v)) = h.split_once(':') {
            req.headers.push((n.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    if req.header("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        while let Some(size) = line(r)? {
            let n = usize::from_str_radix(size.split(';').next().unwrap_or("0").trim(), 16).unwrap_or(0);
            if n == 0 {
                let _ = line(r)?;
                break;
            }
            let mut b = vec![0u8; n];
            r.read_exact(&mut b)?;
            req.body.extend_from_slice(&b);
            let _ = line(r)?;
        }
    } else if let Some(n) = req.header("content-length").and_then(|v| v.parse::<usize>().ok()) {
        let mut b = vec![0u8; n];
        r.read_exact(&mut b)?;
        req.body = b;
    }
    Ok(Some(req))
}
