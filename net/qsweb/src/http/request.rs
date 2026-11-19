// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading a request: the head up to the blank line, then a body by
//! `Content-Length` or chunked.

use std::io::Read;

use super::form::{Form, parse_pairs};
use super::multipart::{self, Part};

/// The most a request head may take.
pub const HEAD_MAX: usize = 16 * 1024;
/// The most a body may take (certificate uploads are the largest).
pub const BODY_MAX: usize = 8 * 1024 * 1024;

/// Why a request could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The connection ended or timed out before a request began.
    #[error("connection closed")]
    Gone,
    /// The head is longer than [`HEAD_MAX`].
    #[error("request head too large")]
    HeadTooLarge,
    /// The body is longer than [`BODY_MAX`].
    #[error("request body too large")]
    BodyTooLarge,
    /// Not HTTP.
    #[error("malformed request")]
    Malformed,
}

/// One request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// `GET`, `POST`, ...
    pub method: String,
    /// The path, percent-decoded.
    pub path: String,
    /// The raw query string, without `?`.
    pub query: String,
    /// `HTTP/1.1` or `HTTP/1.0`.
    pub version: String,
    /// Header names (lower case) and values.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

impl Request {
    /// A request for tests and internal calls.
    pub fn new(method: &str, target: &str) -> Self {
        let (path, query) = split_target(target);
        Self {
            method: method.to_string(),
            path,
            query,
            version: "HTTP/1.1".into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// The first value of header `name` (lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The value of cookie `name`.
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k == "cookie")
            .flat_map(|(_, v)| v.split(';'))
            .filter_map(|c| c.trim().split_once('='))
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v)
    }

    /// The query string's fields.
    pub fn query(&self) -> Form {
        Form::new(parse_pairs(&self.query))
    }

    /// A urlencoded body's fields, or a multipart body's text fields.
    pub fn form(&self) -> Form {
        if let Some(parts) = self.multipart() {
            return Form::new(
                parts
                    .into_iter()
                    .filter(|p| p.filename.is_none())
                    .map(|p| (p.name, String::from_utf8_lossy(&p.data).into_owned()))
                    .collect(),
            );
        }
        Form::new(parse_pairs(&String::from_utf8_lossy(&self.body)))
    }

    /// The parts of a `multipart/form-data` body.
    pub fn multipart(&self) -> Option<Vec<Part>> {
        let ct = self.header("content-type")?;
        let boundary = multipart::boundary(ct)?;
        multipart::parse(&self.body, &boundary)
    }

    /// Whether the client wants the connection kept open.
    pub fn keep_alive(&self) -> bool {
        let conn = self.header("connection").unwrap_or("").to_ascii_lowercase();
        if self.version == "HTTP/1.0" {
            conn.contains("keep-alive")
        } else {
            !conn.contains("close")
        }
    }
}

/// Split a request target into a decoded path and the raw query.
pub fn split_target(target: &str) -> (String, String) {
    let target = target.split('#').next().unwrap_or("");
    let (p, q) = target.split_once('?').unwrap_or((target, ""));
    (super::form::decode(p, false), q.to_string())
}

/// Read one request from `r`. `pending` holds bytes read past the
/// previous request on a kept-alive connection.
pub fn read_request(r: &mut dyn Read, pending: &mut Vec<u8>) -> Result<Request, ReadError> {
    let mut req = read_head(r, pending)?;
    read_body(r, pending, &mut req)?;
    Ok(req)
}

/// Read one request's head from `r`, its body left on the connection
/// for [`read_body`] or a [`BodyStream`].
pub fn read_head(r: &mut dyn Read, pending: &mut Vec<u8>) -> Result<Request, ReadError> {
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(at) = find_end(pending) {
            break at;
        }
        if pending.len() > HEAD_MAX {
            return Err(ReadError::HeadTooLarge);
        }
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadError::Gone),
            Ok(n) => pending.extend_from_slice(&chunk[..n]),
        }
    };
    let (head_len, sep) = end;
    let req = parse_head(&pending[..head_len])?;
    pending.drain(..head_len + sep);
    Ok(req)
}

/// Read `req`'s body whole, up to [`BODY_MAX`].
pub fn read_body(
    r: &mut dyn Read,
    pending: &mut Vec<u8>,
    req: &mut Request,
) -> Result<(), ReadError> {
    if req
        .header("transfer-encoding")
        .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"))
    {
        req.body = read_chunked(r, pending)?;
    } else if let Some(len) = req.header("content-length") {
        let len: usize = len.trim().parse().map_err(|_| ReadError::Malformed)?;
        if len > BODY_MAX {
            return Err(ReadError::BodyTooLarge);
        }
        fill(r, pending, len)?;
        req.body = pending.drain(..len).collect();
    }
    Ok(())
}

/// How a body is framed on the connection.
#[derive(Clone, Copy, Debug)]
enum Framing {
    /// `Content-Length`: bytes still to come.
    Length(u64),
    /// Chunked: bytes left in this chunk, whether one came before.
    Chunked { left: u64, started: bool },
    /// Read to its end.
    Done,
}

/// A request's body read as it arrives, with no limit on its size: a
/// large upload never sits in memory whole. What the reader leaves
/// unread stays on the connection, which then cannot carry another
/// request ([`BodyStream::finished`]).
pub struct BodyStream<'a> {
    r: &'a mut dyn Read,
    pending: &'a mut Vec<u8>,
    framing: Framing,
}

impl<'a> BodyStream<'a> {
    /// The body of `req`, whose head was read by [`read_head`].
    pub fn new(
        r: &'a mut dyn Read,
        pending: &'a mut Vec<u8>,
        req: &Request,
    ) -> Result<Self, ReadError> {
        let framing = if req
            .header("transfer-encoding")
            .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"))
        {
            Framing::Chunked {
                left: 0,
                started: false,
            }
        } else if let Some(len) = req.header("content-length") {
            Framing::Length(len.trim().parse().map_err(|_| ReadError::Malformed)?)
        } else {
            Framing::Done
        };
        Ok(Self {
            r,
            pending,
            framing,
        })
    }

    /// Whether the body was read to its end.
    pub fn finished(&self) -> bool {
        matches!(self.framing, Framing::Done | Framing::Length(0))
    }

    /// Up to `buf.len()` bytes of what is on the connection.
    fn raw(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.pending.is_empty() {
            let n = self.r.read(buf)?;
            if n == 0 {
                return Err(std::io::ErrorKind::UnexpectedEof.into());
            }
            return Ok(n);
        }
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

fn io_err(e: ReadError) -> std::io::Error {
    let kind = match e {
        ReadError::Gone => std::io::ErrorKind::UnexpectedEof,
        _ => std::io::ErrorKind::InvalidData,
    };
    std::io::Error::new(kind, e)
}

impl Read for BodyStream<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            match self.framing {
                Framing::Done | Framing::Length(0) => return Ok(0),
                Framing::Length(left) => {
                    let want = usize::try_from(left).unwrap_or(usize::MAX).min(buf.len());
                    let n = self.raw(&mut buf[..want])?;
                    self.framing = Framing::Length(left - n as u64);
                    return Ok(n);
                }
                Framing::Chunked { left: 0, started } => {
                    if started {
                        // The data's CRLF.
                        if !line(self.r, self.pending).map_err(io_err)?.is_empty() {
                            return Err(io_err(ReadError::Malformed));
                        }
                    }
                    let size_line = line(self.r, self.pending).map_err(io_err)?;
                    let hex = size_line.split(';').next().unwrap_or("").trim();
                    let size =
                        u64::from_str_radix(hex, 16).map_err(|_| io_err(ReadError::Malformed))?;
                    if size == 0 {
                        while !line(self.r, self.pending).map_err(io_err)?.is_empty() {}
                        self.framing = Framing::Done;
                        return Ok(0);
                    }
                    self.framing = Framing::Chunked {
                        left: size,
                        started: true,
                    };
                }
                Framing::Chunked { left, .. } => {
                    let want = usize::try_from(left).unwrap_or(usize::MAX).min(buf.len());
                    let n = self.raw(&mut buf[..want])?;
                    self.framing = Framing::Chunked {
                        left: left - n as u64,
                        started: true,
                    };
                    return Ok(n);
                }
            }
        }
    }
}

/// Read until `pending` holds `n` bytes.
fn fill(r: &mut dyn Read, pending: &mut Vec<u8>, n: usize) -> Result<(), ReadError> {
    let mut chunk = [0u8; 8192];
    while pending.len() < n {
        match r.read(&mut chunk) {
            Ok(0) | Err(_) => return Err(ReadError::Gone),
            Ok(k) => pending.extend_from_slice(&chunk[..k]),
        }
    }
    Ok(())
}

/// Read a line (without CRLF) from the front of `pending`.
fn line(r: &mut dyn Read, pending: &mut Vec<u8>) -> Result<String, ReadError> {
    loop {
        if let Some(at) = pending.iter().position(|&b| b == b'\n') {
            let l: Vec<u8> = pending.drain(..=at).collect();
            let s = String::from_utf8_lossy(&l);
            return Ok(s.trim_end_matches(['\r', '\n']).to_string());
        }
        if pending.len() > HEAD_MAX {
            return Err(ReadError::Malformed);
        }
        let need = pending.len() + 1;
        fill(r, pending, need)?;
    }
}

fn read_chunked(r: &mut dyn Read, pending: &mut Vec<u8>) -> Result<Vec<u8>, ReadError> {
    let mut body = Vec::new();
    loop {
        let size_line = line(r, pending)?;
        let hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(hex, 16).map_err(|_| ReadError::Malformed)?;
        if size == 0 {
            // Trailers up to the blank line.
            while !line(r, pending)?.is_empty() {}
            return Ok(body);
        }
        if body.len() + size > BODY_MAX {
            return Err(ReadError::BodyTooLarge);
        }
        fill(r, pending, size + 2)?;
        body.extend(pending.drain(..size));
        pending.drain(..2);
    }
}

/// Where the head ends: its length and the separator's.
fn find_end(b: &[u8]) -> Option<(usize, usize)> {
    let crlf = b
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|at| (at, 4));
    let lf = b.windows(2).position(|w| w == b"\n\n").map(|at| (at, 2));
    match (crlf, lf) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// Parse a request head (without the blank line).
pub fn parse_head(b: &[u8]) -> Result<Request, ReadError> {
    let text = std::str::from_utf8(b).map_err(|_| ReadError::Malformed)?;
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let first = lines.next().ok_or(ReadError::Malformed)?;
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(ReadError::Malformed);
    };
    if method.is_empty()
        || !method.bytes().all(|c| c.is_ascii_uppercase())
        || !version.starts_with("HTTP/")
    {
        return Err(ReadError::Malformed);
    }
    let mut headers = Vec::new();
    for l in lines {
        if l.is_empty() {
            continue;
        }
        let (k, v) = l.split_once(':').ok_or(ReadError::Malformed)?;
        if k.is_empty() || k.contains(' ') {
            return Err(ReadError::Malformed);
        }
        headers.push((k.to_ascii_lowercase(), v.trim().to_string()));
    }
    let (path, query) = split_target(target);
    Ok(Request {
        method: method.to_string(),
        path,
        query,
        version: version.to_string(),
        headers,
        body: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_two_requests_on_one_connection() {
        let wire = b"POST /a?x=1 HTTP/1.1\r\nContent-Length: 3\r\nCookie: a=1; QS400SID=abc\r\n\r\nk=vGET /b HTTP/1.1\r\n\r\n";
        let mut r = &wire[..];
        let mut pending = Vec::new();
        let one = read_request(&mut r, &mut pending).unwrap();
        assert_eq!(
            (one.method.as_str(), one.path.as_str(), one.query.as_str()),
            ("POST", "/a", "x=1")
        );
        assert_eq!(one.body, b"k=v");
        assert_eq!(one.cookie("QS400SID"), Some("abc"));
        assert_eq!(one.form().get("k"), Some("v"));
        let two = read_request(&mut r, &mut pending).unwrap();
        assert_eq!(two.path, "/b");
        assert_eq!(read_request(&mut r, &mut pending), Err(ReadError::Gone));
    }

    #[test]
    fn streamed_bodies() {
        let wire = b"PUT /a HTTP/1.1\r\nContent-Length: 5\r\n\r\nhelloPUT /b HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2\r\nde\r\n0\r\n\r\nGET /c HTTP/1.1\r\n\r\n";
        let mut r = &wire[..];
        let mut pending = Vec::new();
        for want in [&b"hello"[..], b"abcde"] {
            let req = read_head(&mut r, &mut pending).unwrap();
            let mut s = BodyStream::new(&mut r, &mut pending, &req).unwrap();
            let mut got = Vec::new();
            let mut one = [0u8; 2];
            loop {
                let n = s.read(&mut one).unwrap();
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&one[..n]);
            }
            assert!(s.finished());
            assert_eq!(got, want);
        }
        let req = read_head(&mut r, &mut pending).unwrap();
        assert_eq!(req.path, "/c");
        // A body left unread is not finished; a short one fails.
        let wire = b"PUT /a HTTP/1.1\r\nContent-Length: 9\r\n\r\nshort";
        let mut r = &wire[..];
        let req = read_head(&mut r, &mut pending).unwrap();
        let mut s = BodyStream::new(&mut r, &mut pending, &req).unwrap();
        assert!(!s.finished());
        let mut all = Vec::new();
        assert!(s.read_to_end(&mut all).is_err());
    }

    #[test]
    fn chunked_body() {
        let wire = b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x\r\nde\r\n0\r\n\r\n";
        let mut pending = Vec::new();
        assert_eq!(
            read_request(&mut &wire[..], &mut pending).unwrap().body,
            b"abcde"
        );
    }

    #[test]
    fn limits_and_keep_alive() {
        let mut big = b"GET / HTTP/1.1\r\nX: ".to_vec();
        big.extend(std::iter::repeat_n(b'a', HEAD_MAX + 10));
        assert_eq!(
            read_request(&mut &big[..], &mut Vec::new()),
            Err(ReadError::HeadTooLarge)
        );
        let huge = format!(
            "POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            BODY_MAX + 1
        );
        assert_eq!(
            read_request(&mut huge.as_bytes(), &mut Vec::new()),
            Err(ReadError::BodyTooLarge)
        );
        assert_eq!(parse_head(b"get / HTTP/1.1"), Err(ReadError::Malformed));
        let mut r = Request::new("GET", "/x%20y?q=1#frag");
        assert_eq!((r.path.as_str(), r.query.as_str()), ("/x y", "q=1"));
        assert!(r.keep_alive());
        r.version = "HTTP/1.0".into();
        assert!(!r.keep_alive());
    }
}
