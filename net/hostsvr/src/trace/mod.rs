// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The trace file: what a recording proxy saw on each connection to one
//! server, frame by frame, as a hex dump that diffs well.
//!
//! ```text
//! # QS/400 host server trace, format 1
//! server central 8470
//! scenario cmdcall
//!
//! conn 1 +0ms
//! > 1 +3ms frame 28 e000/7001
//!   0000  00 00 00 1c 00 03 e0 00  00 00 00 00 00 00 00 00  |................|
//!   0010  00 08 70 01 ...
//! < 2 +9ms frame 32 e000/f001
//! note +12ms text
//! end 1 +30ms
//! ```
//!
//! `>` goes to the server, `<` back to the client. A chunk is a `frame`
//! (cut at the header's length) or `raw` (bytes that are not framed:
//! the port mapper, a partial frame at close). Times are milliseconds
//! from the connection's accept. The text column is EBCDIC and is
//! ignored when reading.

pub mod mask;

use std::fmt::Write as _;

use crate::Error;
use crate::frame::header::be16;

/// The first line of every trace.
pub const MAGIC: &str = "# QS/400 host server trace, format 1";

pub use crate::Dir;

/// Bytes seen in one direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Sequence number within the connection.
    pub seq: u32,
    /// Direction.
    pub dir: Dir,
    /// Milliseconds since accept.
    pub at_ms: u64,
    /// Whether this is one whole frame (else raw bytes).
    pub framed: bool,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// One line of a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Bytes.
    Chunk(Chunk),
    /// A remark by the proxy (an upstream failure, say).
    Note {
        /// Milliseconds since accept.
        at_ms: u64,
        /// The remark.
        text: String,
    },
}

/// One accepted connection.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Connection {
    /// Its number within the trace.
    pub id: u32,
    /// What happened, in order.
    pub events: Vec<Event>,
    /// Milliseconds from accept to close, when it closed.
    pub end_ms: Option<u64>,
}

impl Connection {
    /// The chunks.
    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.events.iter().filter_map(|e| match e {
            Event::Chunk(c) => Some(c),
            Event::Note { .. } => None,
        })
    }
}

/// A trace file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Trace {
    /// The server's name (`central`, `signon`, ...).
    pub server: String,
    /// Its port.
    pub port: u16,
    /// What the client was doing.
    pub scenario: String,
    /// The connections in accept order.
    pub connections: Vec<Connection>,
}

impl Trace {
    /// The text.
    pub fn write(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{MAGIC}");
        let _ = writeln!(s, "server {} {}", self.server, self.port);
        let _ = writeln!(s, "scenario {}", self.scenario);
        for c in &self.connections {
            let _ = writeln!(s);
            let _ = writeln!(s, "conn {}", c.id);
            for e in &c.events {
                match e {
                    Event::Chunk(ch) => write_chunk(&mut s, ch),
                    Event::Note { at_ms, text } => {
                        let _ = writeln!(s, "note +{at_ms}ms {text}");
                    }
                }
            }
            if let Some(end) = c.end_ms {
                let _ = writeln!(s, "end {} +{end}ms", c.id);
            }
        }
        s
    }

    /// Read the text back.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut t = Self::default();
        let mut lines = text.lines().enumerate().peekable();
        match lines.next() {
            Some((_, l)) if l.trim_end() == MAGIC => {}
            _ => {
                return Err(Error::Trace {
                    line: 1,
                    what: "not a host server trace".into(),
                });
            }
        }
        while let Some((n, line)) = lines.next() {
            let n = n + 1;
            let bad = |what: &str| Error::Trace {
                line: n,
                what: what.into(),
            };
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut words = line.split_whitespace();
            let first = words.next().unwrap_or_default();
            match first {
                "server" => {
                    t.server = words.next().ok_or_else(|| bad("server name"))?.into();
                    t.port = words
                        .next()
                        .and_then(|p| p.parse().ok())
                        .ok_or_else(|| bad("server port"))?;
                }
                "scenario" => t.scenario = words.collect::<Vec<_>>().join(" "),
                "conn" => {
                    let id = words
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| bad("conn id"))?;
                    t.connections.push(Connection {
                        id,
                        ..Connection::default()
                    });
                }
                "end" => {
                    let c = t
                        .connections
                        .last_mut()
                        .ok_or_else(|| bad("end outside a conn"))?;
                    words.next();
                    c.end_ms = Some(ms(words.next()).ok_or_else(|| bad("end time"))?);
                }
                "note" => {
                    let c = t
                        .connections
                        .last_mut()
                        .ok_or_else(|| bad("note outside a conn"))?;
                    let at_ms = ms(words.next()).ok_or_else(|| bad("note time"))?;
                    let text = line.splitn(3, ' ').nth(2).unwrap_or_default().to_string();
                    c.events.push(Event::Note { at_ms, text });
                }
                ">" | "<" => {
                    let (seq, at_ms, framed, len) = chunk_head(&mut words).map_err(bad)?;
                    let dir = if first == ">" {
                        Dir::Request
                    } else {
                        Dir::Reply
                    };
                    let mut bytes = Vec::with_capacity(len);
                    while bytes.len() < len {
                        let Some((_, hl)) = lines.next() else {
                            return Err(bad("chunk ends early"));
                        };
                        hex_line(hl, &mut bytes).ok_or_else(|| bad("hex line"))?;
                    }
                    if bytes.len() != len {
                        return Err(bad("chunk length"));
                    }
                    let c = t
                        .connections
                        .last_mut()
                        .ok_or_else(|| bad("chunk outside a conn"))?;
                    c.events.push(Event::Chunk(Chunk {
                        seq,
                        dir,
                        at_ms,
                        framed,
                        bytes,
                    }));
                }
                _ => return Err(bad("unknown line")),
            }
        }
        Ok(t)
    }
}

/// The words after a chunk marker: sequence, time, kind, length.
fn chunk_head<'a>(
    words: &mut impl Iterator<Item = &'a str>,
) -> Result<(u32, u64, bool, usize), &'static str> {
    let seq = words
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or("chunk seq")?;
    let at_ms = ms(words.next()).ok_or("chunk time")?;
    let framed = match words.next() {
        Some("frame") => true,
        Some("raw") => false,
        _ => return Err("chunk kind"),
    };
    let len = words
        .next()
        .and_then(|v| v.parse().ok())
        .ok_or("chunk length")?;
    Ok((seq, at_ms, framed, len))
}

fn ms(w: Option<&str>) -> Option<u64> {
    w?.strip_prefix('+')?.strip_suffix("ms")?.parse().ok()
}

/// Append one hex line's bytes; `None` when it is not one.
fn hex_line(line: &str, out: &mut Vec<u8>) -> Option<()> {
    let mut words = line.split_whitespace();
    let offset = words.next()?;
    if offset.len() < 4 || !offset.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    for w in words {
        if w.starts_with('|') {
            break;
        }
        if w.len() != 2 {
            return None;
        }
        out.push(u8::from_str_radix(w, 16).ok()?);
    }
    Some(())
}

fn write_chunk(s: &mut String, ch: &Chunk) {
    let kind = if ch.framed { "frame" } else { "raw" };
    let _ = write!(
        s,
        "{} {} +{}ms {kind} {}",
        ch.dir.marker(),
        ch.seq,
        ch.at_ms,
        ch.bytes.len()
    );
    if ch.framed && ch.bytes.len() >= 20 {
        let _ = write!(s, " {:04x}/{:04x}", be16(&ch.bytes, 6), be16(&ch.bytes, 18));
    }
    s.push('\n');
    for (i, row) in ch.bytes.chunks(16).enumerate() {
        let _ = write!(s, "  {:04x} ", i * 16);
        for (j, b) in row.iter().enumerate() {
            if j == 8 {
                s.push(' ');
            }
            let _ = write!(s, " {b:02x}");
        }
        for j in row.len()..16 {
            s.push_str(if j == 8 { "    " } else { "   " });
        }
        s.push_str("  |");
        for &b in row {
            let c = ccsid::char_of(b);
            s.push(if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '.'
            });
        }
        s.push_str("|\n");
    }
}

/// Cut a byte stream into chunks: whole frames by the header length,
/// whatever does not frame as raw. Returns the chunks and the bytes
/// held back for a frame not yet complete.
pub fn cut_frames(buf: &[u8]) -> (Vec<(bool, Vec<u8>)>, usize) {
    let mut out = Vec::new();
    let mut at = 0;
    while at < buf.len() {
        let rest = &buf[at..];
        match crate::frame::frame_len(rest) {
            Some(n) if n <= rest.len() => {
                out.push((true, rest[..n].to_vec()));
                at += n;
            }
            Some(_) => break,
            None if rest.len() < crate::frame::HEADER_LEN => break,
            None => {
                out.push((false, rest.to_vec()));
                at = buf.len();
            }
        }
    }
    (out, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let t = Trace {
            server: "central".into(),
            port: 8470,
            scenario: "test".into(),
            connections: vec![Connection {
                id: 1,
                events: vec![
                    Event::Chunk(Chunk {
                        seq: 1,
                        dir: Dir::Request,
                        at_ms: 3,
                        framed: true,
                        bytes: (0u8..=40).collect(),
                    }),
                    Event::Note {
                        at_ms: 5,
                        text: "a note | with bars".into(),
                    },
                    Event::Chunk(Chunk {
                        seq: 2,
                        dir: Dir::Reply,
                        at_ms: 9,
                        framed: false,
                        bytes: vec![0x4f, 0x40, 0x6a],
                    }),
                ],
                end_ms: Some(30),
            }],
        };
        let text = t.write();
        assert_eq!(Trace::parse(&text).unwrap(), t);
    }

    #[test]
    fn cuts() {
        let mut b = vec![0, 0, 0, 20];
        b.extend([0; 16]);
        b.extend([0, 0, 0, 22]);
        let (c, used) = cut_frames(&b);
        assert_eq!(c.len(), 1);
        assert_eq!(used, 20);
    }
}
