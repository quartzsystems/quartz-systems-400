// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Responses: pages, redirects, downloads and streams.

use std::fmt::Write as _;
use std::io::{self, Read, Write};

use super::date;

/// A response body.
pub enum Body {
    /// Bytes known up front, sent with `Content-Length`.
    Bytes(Vec<u8>),
    /// A stream sent chunked (a complete log file).
    Stream(Box<dyn Read + Send>),
    /// `101 Switching Protocols`: what then runs on the connection
    /// ([`crate::ws`]).
    Upgrade(crate::ws::Upgrade),
}

impl std::fmt::Debug for Body {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Body::Bytes(b) => write!(f, "Bytes({})", b.len()),
            Body::Stream(_) => f.write_str("Stream"),
            Body::Upgrade(_) => f.write_str("Upgrade"),
        }
    }
}

/// A response.
#[derive(Debug)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// Headers besides `Date`, `Server`, `Content-Length` and `Connection`.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Body,
}

impl Response {
    /// A response with status `status` and a body of `content_type`.
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            headers: vec![("Content-Type".into(), content_type.into())],
            body: Body::Bytes(body),
        }
    }

    /// An HTML page.
    pub fn html(status: u16, page: String) -> Self {
        let mut r = Self::new(status, "text/html; charset=utf-8", page.into_bytes());
        r.headers.push(("Cache-Control".into(), "no-store".into()));
        r
    }

    /// `303 See Other` to `to`: POST, redirect, GET.
    pub fn redirect(to: &str) -> Self {
        let mut r = Self::new(303, "text/plain; charset=utf-8", Vec::new());
        r.headers.push(("Location".into(), to.into()));
        r
    }

    /// A file the browser saves as `filename`.
    pub fn download(filename: &str, content_type: &str, data: Vec<u8>) -> Self {
        let mut r = Self::new(200, content_type, data);
        r.headers.push(("Content-Disposition".into(), format!("attachment; filename=\"{filename}\"")));
        r
    }

    /// A stream the browser saves as `filename`.
    pub fn stream(filename: &str, content_type: &str, from: Box<dyn Read + Send>) -> Self {
        Self {
            status: 200,
            headers: vec![
                ("Content-Type".into(), content_type.into()),
                ("Content-Disposition".into(), format!("attachment; filename=\"{filename}\"")),
            ],
            body: Body::Stream(from),
        }
    }

    /// Add a header.
    #[must_use]
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// The value of header `name` (case-blind).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The body, for a byte body.
    pub fn bytes(&self) -> &[u8] {
        match &self.body {
            Body::Bytes(b) => b,
            Body::Stream(_) | Body::Upgrade(_) => &[],
        }
    }

    /// The body as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(self.bytes()).into_owned()
    }

    /// Write the response; `head_only` leaves the body out, `keep`
    /// keeps the connection.
    pub fn write(self, w: &mut dyn Write, head_only: bool, keep: bool) -> io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nDate: {}\r\nServer: QS400/{}\r\n",
            self.status,
            reason(self.status),
            date::now(),
            release::RELEASE
        );
        for (k, v) in &self.headers {
            let _ = write!(head, "{k}: {v}\r\n");
        }
        let _ = write!(head, "Connection: {}\r\n", if keep { "keep-alive" } else { "close" });
        match self.body {
            Body::Upgrade(_) => {
                head.push_str("\r\n");
                w.write_all(head.as_bytes())?;
            }
            Body::Bytes(b) => {
                let _ = write!(head, "Content-Length: {}\r\n\r\n", b.len());
                w.write_all(head.as_bytes())?;
                if !head_only {
                    w.write_all(&b)?;
                }
            }
            Body::Stream(mut r) => {
                head.push_str("Transfer-Encoding: chunked\r\n\r\n");
                w.write_all(head.as_bytes())?;
                if !head_only {
                    let mut buf = vec![0u8; 32 * 1024];
                    loop {
                        let n = r.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        write!(w, "{n:x}\r\n")?;
                        w.write_all(&buf[..n])?;
                        w.write_all(b"\r\n")?;
                    }
                    w.write_all(b"0\r\n\r\n")?;
                }
            }
        }
        w.flush()
    }
}

/// The reason phrase of `status`.
pub fn reason(status: u16) -> &'static str {
    match status {
        101 => "Switching Protocols",
        200 => "OK",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        410 => "Gone",
        413 => "Content Too Large",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        505 => "HTTP Version Not Supported",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_bytes_and_streams() {
        let mut out = Vec::new();
        Response::redirect("/x").write(&mut out, false, true).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.starts_with("HTTP/1.1 303 See Other\r\n"));
        assert!(s.contains("Location: /x\r\nConnection: keep-alive\r\nContent-Length: 0\r\n\r\n"));
        let mut out = Vec::new();
        Response::stream("a.log", "text/plain", Box::new(&b"hello"[..])).write(&mut out, false, false).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.ends_with("Transfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"));
        assert!(s.contains("attachment; filename=\"a.log\""));
    }
}
