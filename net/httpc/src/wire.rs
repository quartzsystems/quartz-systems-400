// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bytes of a request and a response.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read, Write};

use crate::{Error, Result};

/// Most bytes of a status line and headers.
const HEAD_MAX: usize = 64 * 1024;
/// Most bytes of a body: a CLOB or BLOB's 2 GB.
const BODY_MAX: u64 = 2_147_483_647;

/// Write a request.
pub fn write_request(w: &mut impl Write, method: &str, target: &str, host: &str, headers: &[(String, String)], body: &[u8]) -> Result<()> {
    let mut head = format!("{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    let has = |n: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    if !has("User-Agent") {
        head.push_str("User-Agent: QS400-SQL\r\n");
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
    Ok(String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string())
}

fn too_long() -> Error {
    Error::Protocol("the body is too long".into())
}

/// A response: status, headers, body.
pub type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// Read a response to a request of `method`.
pub fn read_response(r: impl Read, method: &str) -> Result<Answer> {
    let mut r = BufReader::new(r);
    let mut total = 0;
    let (status, headers) = loop {
        let first = line(&mut r, &mut total)?;
        let status: u16 = first
            .split_whitespace()
            .nth(1)
            .filter(|_| first.starts_with("HTTP/"))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| Error::Protocol(format!("status line {first:?}")))?;
        let mut headers = Vec::new();
        loop {
            let l = line(&mut r, &mut total)?;
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        // 1xx answers come before the real one.
        if !(100..200).contains(&status) {
            break (status, headers);
        }
    };
    let get = |n: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.clone());
    let mut body = Vec::new();
    if method == "HEAD" || status == 204 || status == 304 {
        return Ok((status, headers, body));
    }
    if get("Transfer-Encoding").is_some_and(|t| t.to_ascii_lowercase().contains("chunked")) {
        loop {
            let size = line(&mut r, &mut 0)?;
            let n = u64::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16)
                .map_err(|_| Error::Protocol(format!("chunk size {size:?}")))?;
            if n == 0 {
                while !line(&mut r, &mut 0)?.is_empty() {}
                break;
            }
            let start = body.len();
            let end = u64::try_from(start).map_err(|_| too_long())? + n;
            if end > BODY_MAX {
                return Err(too_long());
            }
            body.resize(usize::try_from(end).map_err(|_| too_long())?, 0);
            r.read_exact(&mut body[start..])?;
            line(&mut r, &mut 0)?;
        }
    } else if let Some(len) = get("Content-Length") {
        let n: u64 = len.parse().map_err(|_| Error::Protocol(format!("Content-Length {len:?}")))?;
        if n > BODY_MAX {
            return Err(too_long());
        }
        body.resize(usize::try_from(n).map_err(|_| too_long())?, 0);
        r.read_exact(&mut body)?;
    } else {
        r.take(BODY_MAX).read_to_end(&mut body)?;
    }
    Ok((status, headers, body))
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
}
