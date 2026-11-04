// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTP/1.1 `GET` of one small file over a connection the caller makes
//! (TCP, or TLS over it for `https`): `Connection: close`, a body by
//! `Content-Length`, chunks or the end of the connection, up to three
//! redirects.

use std::io::{self, Read, Write};

use super::{MAX_BYTES, Url};

/// A connection: anything to read and write.
pub trait Conn: Read + Write {}
impl<T: Read + Write> Conn for T {}

/// Makes a connection to a URL's host and port (TLS for `https`).
pub type Connect<'a> = &'a dyn Fn(&Url) -> io::Result<Box<dyn Conn>>;

/// The body of `url`, following redirects.
pub fn get(url: &Url, connect: Connect<'_>) -> Result<Vec<u8>, String> {
    let mut at = url.clone();
    for _ in 0..4 {
        let mut c = connect(&at).map_err(|e| format!("{}:{}: {e}", at.host, at.port))?;
        let host = if at.port == if at.scheme == "https" { 443 } else { 80 } {
            at.host.clone()
        } else {
            format!("{}:{}", at.host, at.port)
        };
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: QS400-install\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            at.path
        );
        c.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        c.flush().map_err(|e| e.to_string())?;
        let mut raw = Vec::new();
        // A TLS peer that closes without close_notify still sent the body.
        let _ = c.take((MAX_BYTES + 64 * 1024) as u64).read_to_end(&mut raw);
        let r = parse(&raw)?;
        match r.status {
            200 => return Ok(r.body),
            301 | 302 | 303 | 307 | 308 => {
                let loc = r.location.ok_or_else(|| format!("HTTP {} without a Location", r.status))?;
                at = at.join(&loc)?;
            }
            s => return Err(format!("HTTP {s} for {}", at.path)),
        }
    }
    Err("too many redirects".into())
}

/// A response.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// `Location`, when there was one.
    pub location: Option<String>,
    /// The body.
    pub body: Vec<u8>,
}

/// A whole response as read.
pub fn parse(raw: &[u8]) -> Result<Response, String> {
    let end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("the response has no end of headers")?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("not an HTTP status line: {status_line}"))?;
    let mut length = None;
    let mut chunked = false;
    let mut location = None;
    for l in lines {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = v.parse::<usize>().ok(),
            "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
            "location" => location = Some(v.to_string()),
            _ => {}
        }
    }
    let rest = &raw[end + 4..];
    let body = if chunked {
        dechunk(rest)?
    } else if let Some(n) = length {
        if rest.len() < n {
            return Err(format!("the body ended after {} of {n} bytes", rest.len()));
        }
        rest[..n].to_vec()
    } else {
        rest.to_vec()
    };
    if body.len() > MAX_BYTES {
        return Err("the response file is larger than 1 MiB".into());
    }
    Ok(Response { status, location, body })
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").ok_or("a chunk without a size line")?;
        let size_text = String::from_utf8_lossy(&b[..eol]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| format!("chunk size '{size_text}'"))?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size + 2 {
            return Err("a chunk ended early".into());
        }
        out.extend_from_slice(&b[..size]);
        b = &b[size + 2..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn bodies() {
        let r = parse(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello!").unwrap();
        assert_eq!((r.status, r.body.as_slice()), (200, &b"hello"[..]));
        let r = parse(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x\r\nde\r\n0\r\n\r\n").unwrap();
        assert_eq!(r.body, b"abcde");
        let r = parse(b"HTTP/1.0 302 Found\r\nLocation: /b\r\n\r\n").unwrap();
        assert_eq!(r.location.as_deref(), Some("/b"));
        assert!(parse(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc").is_err());
        assert!(parse(b"garbage").is_err());
    }

    #[test]
    fn get_over_loopback_with_a_redirect() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let t = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for answer in [
                "HTTP/1.1 302 Found\r\nLocation: /qs400/INSTALL.RSP\r\nContent-Length: 0\r\n\r\n".to_string(),
                "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\nlic-option = 2".to_string(),
            ] {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                paths.push(req.lines().next().unwrap_or_default().to_string());
                assert!(req.contains(&format!("Host: 127.0.0.1:{port}")), "{req}");
                s.write_all(answer.as_bytes()).unwrap();
            }
            paths
        });
        let url = Url::parse(&format!("http://127.0.0.1:{port}/rsp")).unwrap();
        let tcp = |u: &Url| -> io::Result<Box<dyn Conn>> { Ok(Box::new(TcpStream::connect((u.host.as_str(), u.port))?)) };
        let body = get(&url, &tcp).unwrap();
        assert_eq!(body, b"lic-option = 2");
        assert_eq!(t.join().unwrap(), ["GET /rsp HTTP/1.1", "GET /qs400/INSTALL.RSP HTTP/1.1"]);
    }
}
