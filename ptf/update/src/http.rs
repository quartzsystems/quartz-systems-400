// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The HTTP/1.1 the protocol needs: one request per connection, bodies
//! sized by `Content-Length`.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};

use crate::{Error, Result};

/// Most bytes of a request or status line and its headers.
const HEAD_MAX: usize = 16 * 1024;
/// Most bytes of a body (a LIC PTF carries two kernel images).
pub const BODY_MAX: u64 = 1 << 30;

/// A request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    /// `GET` or `POST`.
    pub method: String,
    /// The request target.
    pub target: String,
    /// Headers, names as sent.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: Vec<u8>,
}

/// A response.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Headers.
    pub headers: Vec<(String, String)>,
    /// Body.
    pub body: Vec<u8>,
}

impl Response {
    /// A response with a body and its type.
    #[must_use]
    pub fn new(status: u16, kind: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), kind.into())],
            body,
        }
    }

    /// A text response.
    #[must_use]
    pub fn text(status: u16, text: &str) -> Self {
        Self::new(status, "text/plain; charset=utf-8", text.as_bytes().to_vec())
    }

    /// A header's value.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find(&self.headers, name)
    }
}

impl Request {
    /// A header's value.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find(&self.headers, name)
    }
}

fn find<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    }
}

/// Write a request; `host` is the `Host` header.
pub fn write_request(w: &mut impl Write, req: &Request, host: &str) -> Result<()> {
    let mut head = format!("{} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n", req.method, req.target);
    for (k, v) in &req.headers {
        let _ = write!(head, "{k}: {v}\r\n");
    }
    let _ = write!(head, "Content-Length: {}\r\n\r\n", req.body.len());
    w.write_all(head.as_bytes())?;
    w.write_all(&req.body)?;
    w.flush()?;
    Ok(())
}

/// Write a response.
pub fn write_response(w: &mut impl Write, res: &Response) -> Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\nConnection: close\r\n", res.status, reason(res.status));
    for (k, v) in &res.headers {
        let _ = write!(head, "{k}: {v}\r\n");
    }
    let _ = write!(head, "Content-Length: {}\r\n\r\n", res.body.len());
    w.write_all(head.as_bytes())?;
    w.write_all(&res.body)?;
    w.flush()?;
    Ok(())
}

/// The first line and the headers.
fn read_head(r: &mut impl BufRead) -> Result<(String, Vec<(String, String)>)> {
    let mut first = String::new();
    let mut total = 0;
    let mut headers = Vec::new();
    loop {
        let mut line = Vec::new();
        let n = r.by_ref().take(HEAD_MAX as u64).read_until(b'\n', &mut line)?;
        if n == 0 {
            return Err(Error::Protocol("connection closed in the head".into()));
        }
        total += n;
        if total > HEAD_MAX {
            return Err(Error::Protocol("head too long".into()));
        }
        let line = String::from_utf8_lossy(&line).trim_end().to_string();
        if first.is_empty() {
            if line.is_empty() {
                continue;
            }
            first = line;
            continue;
        }
        if line.is_empty() {
            return Ok((first, headers));
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| Error::Protocol(format!("header {line:?}")))?;
        headers.push((k.trim().to_string(), v.trim().to_string()));
    }
}

fn read_body(r: &mut impl Read, headers: &[(String, String)], to_end: bool) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    match find(headers, "Content-Length") {
        Some(n) => {
            let n: u64 = n
                .parse()
                .map_err(|_| Error::Protocol(format!("Content-Length {n:?}")))?;
            if n > BODY_MAX {
                return Err(Error::Protocol(format!("body of {n} bytes")));
            }
            r.take(n).read_to_end(&mut body)?;
            if body.len() as u64 != n {
                return Err(Error::Protocol("connection closed in the body".into()));
            }
        }
        None if to_end => {
            r.take(BODY_MAX).read_to_end(&mut body)?;
        }
        None => {}
    }
    Ok(body)
}

/// Read a request.
pub fn read_request(r: impl Read) -> Result<Request> {
    let mut r = BufReader::new(r);
    let (first, headers) = read_head(&mut r)?;
    let mut f = first.split_whitespace();
    let (Some(method), Some(target)) = (f.next(), f.next()) else {
        return Err(Error::Protocol(format!("request line {first:?}")));
    };
    let body = read_body(&mut r, &headers, false)?;
    Ok(Request {
        method: method.into(),
        target: target.into(),
        headers,
        body,
    })
}

/// Read a response; `head_only` for the answer to `CONNECT`.
pub fn read_response(r: impl Read, head_only: bool) -> Result<Response> {
    let mut r = BufReader::new(r);
    let (first, headers) = read_head(&mut r)?;
    let status = first
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::Protocol(format!("status line {first:?}")))?;
    let body = if head_only {
        Vec::new()
    } else {
        read_body(&mut r, &headers, true)?
    };
    Ok(Response { status, headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_responses_round_trip() {
        let req = Request {
            method: "POST".into(),
            target: "/v1/order".into(),
            headers: vec![("X-QS400-System".into(), "QS400".into())],
            body: b"QS400ORDER 1\n".to_vec(),
        };
        let mut out = Vec::new();
        write_request(&mut out, &req, "h:1").unwrap();
        let back = read_request(&out[..]).unwrap();
        assert_eq!(back.method, "POST");
        assert_eq!(back.header("x-qs400-system"), Some("QS400"));
        assert_eq!(back.body, req.body);
        let mut out = Vec::new();
        write_response(&mut out, &Response::text(404, "QS00999 not found")).unwrap();
        let res = read_response(&out[..], false).unwrap();
        assert_eq!(res.status, 404);
        assert_eq!(res.body, b"QS00999 not found");
    }
}
