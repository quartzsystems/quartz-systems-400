// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bytes of a request and a response.

use std::fmt::Write as _;
use std::io::{self, BufRead, Read, Write};

use crate::{Error, Result};

/// Most bytes of a status line and headers.
const HEAD_MAX: usize = 64 * 1024;
/// Most bytes of a body read whole: a CLOB or BLOB's 2 GB.
const BODY_MAX: u64 = 2_147_483_647;

/// Write a request; `agent` is the `User-Agent` sent when `headers`
/// have none (none when empty).
pub fn write_request(
    w: &mut impl Write,
    method: &str,
    target: &str,
    host: &str,
    agent: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<()> {
    let mut head = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    let has = |n: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    if !has("User-Agent") && !agent.is_empty() {
        if agent.contains(['\r', '\n']) {
            return Err(Error::Protocol("the agent holds a line end".into()));
        }
        let _ = write!(head, "User-Agent: {agent}\r\n");
    }
    if !has("Accept") {
        head.push_str("Accept: */*\r\n");
    }
    for (k, v) in headers {
        if k.contains(['\r', '\n']) || v.contains(['\r', '\n']) {
            return Err(Error::Protocol(format!("header {k} holds a line end")));
        }
        let _ = write!(head, "{k}: {v}\r\n");
    }
    if !body.is_empty() || matches!(method, "POST" | "PUT" | "PATCH") {
        let _ = write!(head, "Content-Length: {}\r\n", body.len());
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()?;
    Ok(())
}

fn line(r: &mut impl BufRead, total: &mut usize) -> Result<String> {
    let mut buf = Vec::new();
    let n = r.read_until(b'\n', &mut buf)?;
    *total += n;
    if *total > HEAD_MAX {
        return Err(Error::Protocol("the headers are too long".into()));
    }
    if n == 0 {
        return Err(Error::Protocol("the connection closed".into()));
    }
    Ok(String::from_utf8_lossy(&buf)
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

fn too_long() -> Error {
    Error::Protocol("the body is too long".into())
}

/// A status and its headers.
pub type Head = (u16, Vec<(String, String)>);

/// Read the status line and headers; 1xx answers are skipped.
pub fn read_head(r: &mut impl BufRead) -> Result<Head> {
    let mut total = 0;
    loop {
        let first = line(r, &mut total)?;
        let status: u16 = first
            .split_whitespace()
            .nth(1)
            .filter(|_| first.starts_with("HTTP/"))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::Protocol(format!("status line {first:?}")))?;
        let mut headers = Vec::new();
        loop {
            let l = line(r, &mut total)?;
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        // 1xx answers come before the real one.
        if !(100..200).contains(&status) {
            return Ok((status, headers));
        }
    }
}

/// A header's value.
pub fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// How a body is framed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    /// No body.
    Empty,
    /// `Content-Length` bytes.
    Length(u64),
    /// Chunked.
    Chunked,
    /// To the close.
    Close,
}

/// The framing of the body after `head`, for a request of `method`.
pub fn frame(method: &str, (status, headers): &Head) -> Result<Frame> {
    if method == "HEAD" || *status == 204 || *status == 304 {
        return Ok(Frame::Empty);
    }
    if header(headers, "Transfer-Encoding")
        .is_some_and(|t| t.to_ascii_lowercase().contains("chunked"))
    {
        return Ok(Frame::Chunked);
    }
    match header(headers, "Content-Length") {
        Some(len) => len
            .parse()
            .map(Frame::Length)
            .map_err(|_| Error::Protocol(format!("Content-Length {len:?}"))),
        None => Ok(Frame::Close),
    }
}

/// A protocol error carried through `io::Read`.
#[derive(Debug)]
struct Proto(String);

impl std::fmt::Display for Proto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Proto {}

/// An I/O error from a [`Body`] as the client's error.
pub fn from_io(e: io::Error) -> Error {
    if e.get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<Proto>)
    {
        let inner = e.into_inner().expect("checked above");
        return Error::Protocol(inner.to_string());
    }
    Error::Io(e)
}

/// A body as a reader: its framing taken off. A body cut short is an
/// `UnexpectedEof` error.
pub struct Body<'a, R: BufRead> {
    r: &'a mut R,
    frame: Frame,
    /// Bytes left in the body (`Length`) or the chunk (`Chunked`).
    left: u64,
    /// A chunk was read, so its line end comes before the next size.
    after_chunk: bool,
    done: bool,
}

impl<'a, R: BufRead> Body<'a, R> {
    /// The body framed as `frame` on `r`.
    pub fn new(r: &'a mut R, frame: Frame) -> Self {
        let left = match frame {
            Frame::Length(n) => n,
            _ => 0,
        };
        Self {
            r,
            frame,
            left,
            after_chunk: false,
            done: matches!(frame, Frame::Empty | Frame::Length(0)),
        }
    }

    /// The next chunk's size; 0 ends the body (its trailers read).
    fn next_chunk(&mut self) -> Result<u64> {
        if self.after_chunk {
            line(self.r, &mut 0)?;
        }
        let size = line(self.r, &mut 0)?;
        let n = u64::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| Error::Protocol(format!("chunk size {size:?}")))?;
        if n == 0 {
            while !line(self.r, &mut 0)?.is_empty() {}
        }
        self.after_chunk = true;
        Ok(n)
    }
}

fn to_io(e: Error) -> io::Error {
    match e {
        Error::Io(e) => e,
        Error::Protocol(s) => io::Error::new(io::ErrorKind::InvalidData, Proto(s)),
        e => io::Error::other(e.to_string()),
    }
}

impl<R: BufRead> Read for Body<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        if self.frame == Frame::Close {
            return self.r.read(buf);
        }
        if self.left == 0 {
            // Only a chunked body gets here with bytes still to come.
            let n = self.next_chunk().map_err(to_io)?;
            if n == 0 {
                self.done = true;
                return Ok(0);
            }
            self.left = n;
        }
        let want = usize::try_from(self.left)
            .unwrap_or(usize::MAX)
            .min(buf.len());
        let n = self.r.read(&mut buf[..want])?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        self.left -= n as u64;
        if self.left == 0 && self.frame != Frame::Chunked {
            self.done = true;
        }
        Ok(n)
    }
}

/// Read a whole body: at most 2 GB, a body to the close cut there.
pub fn read_body(r: &mut impl BufRead, method: &str, head: &Head) -> Result<Vec<u8>> {
    let frame = frame(method, head)?;
    if let Frame::Length(n) = frame
        && n > BODY_MAX
    {
        return Err(too_long());
    }
    let mut body = Vec::new();
    Body::new(r, frame)
        .take(BODY_MAX + 1)
        .read_to_end(&mut body)
        .map_err(from_io)?;
    if body.len() as u64 > BODY_MAX {
        if frame != Frame::Close {
            return Err(too_long());
        }
        body.truncate(usize::try_from(BODY_MAX).map_err(|_| too_long())?);
    }
    Ok(body)
}

/// A response: status, headers, body.
#[cfg(test)]
pub type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// Read a whole response to a request of `method`.
#[cfg(test)]
pub fn read_response(r: impl Read, method: &str) -> Result<Answer> {
    let mut r = std::io::BufReader::new(r);
    let head = read_head(&mut r)?;
    let body = read_body(&mut r, method, &head)?;
    Ok((head.0, head.1, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_and_sized_bodies() {
        let t = b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=y\r\nde\r\n0\r\n\r\n";
        let (s, _, b) = read_response(&t[..], "GET").unwrap();
        assert_eq!((s, b.as_slice()), (200, &b"abcde"[..]));
        let t = b"HTTP/1.0 404 Not Found\r\nContent-Length: 4\r\n\r\ngone and more";
        let (s, h, b) = read_response(&t[..], "GET").unwrap();
        assert_eq!((s, h.len(), b.as_slice()), (404, 1, &b"gone"[..]));
        assert!(read_response(&b"SSH-2.0\r\n"[..], "GET").is_err());
    }

    #[test]
    fn cut_and_bad_bodies() {
        let t = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nshort";
        assert!(
            matches!(read_response(&t[..], "GET"), Err(Error::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof)
        );
        let t = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n";
        assert!(
            matches!(read_response(&t[..], "GET"), Err(Error::Protocol(s)) if s.contains("chunk size"))
        );
        let t = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nab";
        assert!(read_response(&t[..], "GET").is_err());
        let t = b"HTTP/1.1 200 OK\r\n\r\nto the close";
        assert_eq!(read_response(&t[..], "GET").unwrap().2, b"to the close");
        let t = b"HTTP/1.1 204 No Content\r\nContent-Length: 3\r\n\r\nabc";
        assert!(read_response(&t[..], "GET").unwrap().2.is_empty());
    }
}
