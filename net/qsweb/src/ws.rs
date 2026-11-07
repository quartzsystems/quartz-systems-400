// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! WebSocket (RFC 6455) over an HTTP/1.1 connection: the upgrade
//! handshake and the frames, server side. Only Access for Web's 5250
//! session page uses it; every other page stays plain HTML.

use std::io::{self, Read, Write};
use std::time::Duration;

use sha1::{Digest, Sha1};

use crate::http::{Body, Request, Response};

/// The GUID RFC 6455 appends to the client's key.
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
/// The largest message taken from a client.
const MAX_MESSAGE: usize = 1 << 20;

/// A connection an upgraded protocol runs over.
pub trait Duplex: Read + Write {}
impl<T: Read + Write> Duplex for T {}

/// Sets the connection's read timeout (`None`: block).
pub type SetTimeout<'a> = &'a dyn Fn(Option<Duration>);

/// What runs once the connection is upgraded.
pub type Upgrade = Box<dyn FnOnce(&mut dyn Duplex, SetTimeout<'_>) + Send>;

/// Whether `req` asks for a WebSocket.
pub fn is_upgrade(req: &Request) -> bool {
    let has = |h: &str, v: &str| {
        req.header(h)
            .is_some_and(|x| x.split(',').any(|p| p.trim().eq_ignore_ascii_case(v)))
    };
    req.method == "GET"
        && has("upgrade", "websocket")
        && has("connection", "upgrade")
        && req.header("sec-websocket-key").is_some()
}

/// `Sec-WebSocket-Accept` for a client's key.
pub fn accept_key(key: &str) -> String {
    let mut h = Sha1::new();
    h.update(key.trim().as_bytes());
    h.update(GUID.as_bytes());
    base64(&h.finalize())
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// The `101 Switching Protocols` answer to `req`, with `run` to take
/// the connection; `400` when `req` is not a WebSocket handshake.
pub fn upgrade(req: &Request, run: impl FnOnce(&mut Socket<'_>) + Send + 'static) -> Response {
    if !is_upgrade(req) || req.header("sec-websocket-version").map(str::trim) != Some("13") {
        return crate::http::plain(400, "This address speaks WebSocket version 13 only.")
            .with("Sec-WebSocket-Version", "13");
    }
    let accept = accept_key(req.header("sec-websocket-key").unwrap_or(""));
    Response {
        status: 101,
        headers: vec![
            ("Upgrade".into(), "websocket".into()),
            ("Sec-WebSocket-Accept".into(), accept),
        ],
        body: Body::Upgrade(Box::new(move |conn, timeout| {
            let mut s = Socket {
                conn,
                timeout,
                buf: Vec::new(),
                parts: Vec::new(),
                text: false,
            };
            run(&mut s);
        })),
    }
}

/// A message from the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// A text message.
    Text(String),
    /// A binary message.
    Binary(Vec<u8>),
}

/// The server's side of a WebSocket.
pub struct Socket<'a> {
    conn: &'a mut dyn Duplex,
    timeout: SetTimeout<'a>,
    buf: Vec<u8>,
    parts: Vec<u8>,
    text: bool,
}

impl Socket<'_> {
    /// The next message, waiting at most `wait`: `Ok(None)` when none
    /// came; an error when the client closed or broke the protocol.
    pub fn recv(&mut self, wait: Duration) -> io::Result<Option<Message>> {
        loop {
            if let Some(m) = self.frame()? {
                return Ok(Some(m));
            }
            (self.timeout)(Some(wait.max(Duration::from_millis(1))));
            let mut chunk = [0u8; 8192];
            match self.conn.read(&mut chunk) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// A whole message out of the buffer, answering pings and closes.
    fn frame(&mut self) -> io::Result<Option<Message>> {
        loop {
            let b = &self.buf;
            if b.len() < 2 {
                return Ok(None);
            }
            let fin = b[0] & 0x80 != 0;
            let op = b[0] & 0x0F;
            if b[1] & 0x80 == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a client frame must be masked",
                ));
            }
            let (len, mut at) = match b[1] & 0x7F {
                126 if b.len() >= 4 => (usize::from(u16::from_be_bytes([b[2], b[3]])), 4),
                127 if b.len() >= 10 => (
                    usize::try_from(u64::from_be_bytes(b[2..10].try_into().unwrap_or_default()))
                        .unwrap_or(usize::MAX),
                    10,
                ),
                126 | 127 => return Ok(None),
                n => (usize::from(n), 2),
            };
            if len > MAX_MESSAGE || self.parts.len() + len > MAX_MESSAGE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "message too large",
                ));
            }
            if b.len() < at + 4 + len {
                return Ok(None);
            }
            let mask = [b[at], b[at + 1], b[at + 2], b[at + 3]];
            at += 4;
            let data: Vec<u8> = b[at..at + len]
                .iter()
                .enumerate()
                .map(|(i, x)| x ^ mask[i % 4])
                .collect();
            self.buf.drain(..at + len);
            match op {
                0x8 => {
                    let _ = self.write_frame(0x8, &data);
                    return Err(io::ErrorKind::ConnectionAborted.into());
                }
                0x9 => self.write_frame(0xA, &data)?,
                0xA => {}
                0x0..=0x2 => {
                    if op != 0 {
                        self.text = op == 1;
                        self.parts.clear();
                    }
                    self.parts.extend_from_slice(&data);
                    if fin {
                        let whole = std::mem::take(&mut self.parts);
                        return Ok(Some(if self.text {
                            Message::Text(String::from_utf8(whole).map_err(|_| {
                                io::Error::new(io::ErrorKind::InvalidData, "text is not UTF-8")
                            })?)
                        } else {
                            Message::Binary(whole)
                        }));
                    }
                }
                _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown opcode")),
            }
        }
    }

    fn write_frame(&mut self, op: u8, data: &[u8]) -> io::Result<()> {
        let mut f = vec![0x80 | op];
        match data.len() {
            n if n < 126 => f.push(u8::try_from(n).unwrap_or(0)),
            n if n <= 0xFFFF => {
                f.push(126);
                f.extend_from_slice(&u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes());
            }
            n => {
                f.push(127);
                f.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        f.extend_from_slice(data);
        (self.timeout)(None);
        self.conn.write_all(&f)?;
        self.conn.flush()
    }

    /// Send a text message.
    pub fn send(&mut self, text: &str) -> io::Result<()> {
        self.write_frame(0x1, text.as_bytes())
    }

    /// Close the socket.
    pub fn close(&mut self) {
        let _ = self.write_frame(0x8, &1000u16.to_be_bytes());
    }
}

/// A client frame of `text`, masked, for tests.
pub fn client_frame(text: &str) -> Vec<u8> {
    let mask = [0x37, 0xfa, 0x21, 0x3d];
    let mut f = vec![0x81];
    let n = text.len();
    if n < 126 {
        f.push(0x80 | u8::try_from(n).unwrap_or(0));
    } else {
        f.push(0x80 | 0x7E);
        f.extend_from_slice(&u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes());
    }
    f.extend_from_slice(&mask);
    f.extend(text.bytes().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    f
}

/// The text messages in the server's frames `b`, for tests.
pub fn server_texts(mut b: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    while b.len() >= 2 {
        let op = b[0] & 0x0F;
        let (len, at) = match b[1] & 0x7F {
            126 => (usize::from(u16::from_be_bytes([b[2], b[3]])), 4),
            127 => (
                usize::try_from(u64::from_be_bytes(b[2..10].try_into().unwrap_or_default()))
                    .unwrap_or(0),
                10,
            ),
            n => (usize::from(n), 2),
        };
        if b.len() < at + len {
            break;
        }
        if op == 1 {
            out.push(String::from_utf8_lossy(&b[at..at + len]).into_owned());
        }
        b = &b[at + len..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_example_key() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert_eq!(base64(b"ab"), "YWI=");
    }

    struct Pipe {
        input: io::Cursor<Vec<u8>>,
        output: Vec<u8>,
    }
    impl Read for Pipe {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let n = self.input.read(b)?;
            if n == 0 {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(n)
            }
        }
    }
    impl Write for Pipe {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.output.write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn frames_both_ways() {
        let mut input = client_frame("hello");
        input.extend(client_frame(&"x".repeat(300)));
        let mut p = Pipe {
            input: io::Cursor::new(input),
            output: Vec::new(),
        };
        let none = |_: Option<Duration>| {};
        let mut s = Socket {
            conn: &mut p,
            timeout: &none,
            buf: Vec::new(),
            parts: Vec::new(),
            text: false,
        };
        assert_eq!(
            s.recv(Duration::from_millis(1)).unwrap(),
            Some(Message::Text("hello".into()))
        );
        assert_eq!(
            s.recv(Duration::from_millis(1)).unwrap(),
            Some(Message::Text("x".repeat(300)))
        );
        assert_eq!(s.recv(Duration::from_millis(1)).unwrap(), None);
        s.send("screen").unwrap();
        assert_eq!(server_texts(&p.output), ["screen"]);
    }
}
