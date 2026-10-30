// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM trace file: what the recording proxy saw on each connection
//! to port 446, DSS by DSS, as a hex dump that diffs well.
//!
//! ```text
//! # QS/400 DDM trace, format 1
//! server ddm 446
//! scenario sql-catalog
//!
//! conn 1
//! > 1 +3ms dss 120 rqs 1 1041 EXCSAT
//!   0000  00 78 d0 41 00 01 00 72  10 41 00 ...   |..}.......|
//! < 2 +9ms dss 96 rpy 1 1443 EXCSATRD
//! note +12ms text
//! end 1 +30ms
//! ```
//!
//! `>` goes to the server, `<` back. A chunk is a `dss` (cut at the
//! DSS length, segments included) or `raw` (bytes that do not frame).
//! After the length come the type, the correlation ID and the first
//! object's code point and name, for reading only. The text column is
//! EBCDIC and ignored when reading.

pub mod mask;

use std::fmt::Write as _;

use crate::wire::{Dss, DssType, dss_len};
use crate::{Dir, Error};

/// The first line of every trace.
pub const MAGIC: &str = "# QS/400 DDM trace, format 1";

/// Bytes seen in one direction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Sequence number within the connection.
    pub seq: u32,
    /// Direction.
    pub dir: Dir,
    /// Milliseconds since accept.
    pub at_ms: u64,
    /// Whether this is one whole DSS (else raw bytes).
    pub framed: bool,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// One line of a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Bytes.
    Chunk(Chunk),
    /// A remark by the proxy.
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
    /// The server's name (`ddm`).
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
    #[allow(clippy::too_many_lines)] // one arm per line kind
    pub fn parse(text: &str) -> Result<Self, Error> {
        let mut t = Self::default();
        let mut lines = text.lines().enumerate();
        match lines.next() {
            Some((_, l)) if l.trim_end() == MAGIC => {}
            _ => {
                return Err(Error::Trace {
                    line: 1,
                    what: "not a DDM trace".into(),
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
                    let seq = words
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| bad("chunk seq"))?;
                    let at_ms = ms(words.next()).ok_or_else(|| bad("chunk time"))?;
                    let framed = match words.next() {
                        Some("dss") => true,
                        Some("raw") => false,
                        _ => return Err(bad("chunk kind")),
                    };
                    let len: usize = words
                        .next()
                        .and_then(|v| v.parse().ok())
                        .ok_or_else(|| bad("chunk length"))?;
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
                        dir: if first == ">" {
                            Dir::Request
                        } else {
                            Dir::Reply
                        },
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

fn ms(w: Option<&str>) -> Option<u64> {
    w?.strip_prefix('+')?.strip_suffix("ms")?.parse().ok()
}

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

/// The reading aid after a DSS's length: type, correlation, code point.
fn describe(b: &[u8]) -> String {
    let Ok((d, _)) = Dss::decode(b) else {
        return String::new();
    };
    let ty = match d.ty {
        DssType::Request => "rqs",
        DssType::Reply => "rpy",
        DssType::Object => "obj",
        DssType::Encrypted => "cmn",
        DssType::RequestNoReply => "rqsnr",
        DssType::Other(_) => "dss?",
    };
    let mut s = format!(" {ty} {}", d.corr);
    if let Some(code) = d.code() {
        let _ = write!(s, " {code:04x}");
        if let Some(n) = crate::cp::name(code) {
            let _ = write!(s, " {n}");
        }
    }
    s
}

fn write_chunk(s: &mut String, ch: &Chunk) {
    let kind = if ch.framed { "dss" } else { "raw" };
    let _ = write!(
        s,
        "{} {} +{}ms {kind} {}",
        ch.dir.marker(),
        ch.seq,
        ch.at_ms,
        ch.bytes.len()
    );
    if ch.framed {
        s.push_str(&describe(&ch.bytes));
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

/// Cut a byte stream into chunks: whole DSSes, and whatever does not
/// frame as raw. Returns the chunks and the bytes used (the rest waits
/// for more).
pub fn cut(buf: &[u8]) -> (Vec<(bool, Vec<u8>)>, usize) {
    let mut out = Vec::new();
    let mut at = 0;
    while at < buf.len() {
        let rest = &buf[at..];
        match dss_len(rest) {
            Ok(Some(n)) => {
                out.push((true, rest[..n].to_vec()));
                at += n;
            }
            Ok(None) => break,
            Err(_) => {
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
    use crate::cp;
    use crate::wire::Object;

    #[test]
    fn round_trip() {
        let d = Dss::request(1, Object::coll(cp::cmd::EXCSAT, vec![Object::chars(cp::param::EXTNAM, "X")]));
        let t = Trace {
            server: "ddm".into(),
            port: 446,
            scenario: "test".into(),
            connections: vec![Connection {
                id: 1,
                events: vec![
                    Event::Chunk(Chunk {
                        seq: 1,
                        dir: Dir::Request,
                        at_ms: 3,
                        framed: true,
                        bytes: d.encode(),
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
                        bytes: vec![1, 2, 3],
                    }),
                ],
                end_ms: Some(30),
            }],
        };
        let text = t.write();
        assert!(text.contains("rqs 1 1041 EXCSAT"));
        assert_eq!(Trace::parse(&text).unwrap(), t);
    }

    #[test]
    fn cuts_whole_dss() {
        let mut b = Dss::request(1, Object::coll(cp::cmd::RDBCMM, vec![])).encode();
        let one = b.len();
        b.extend([0, 20, 0xD0]);
        let (c, used) = cut(&b);
        assert_eq!(c.len(), 1);
        assert_eq!(used, one);
    }
}
