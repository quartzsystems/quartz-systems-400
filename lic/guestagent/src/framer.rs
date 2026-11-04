// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Requests out of the channel's byte stream. The protocol sends JSON
//! objects one after another, with or without whitespace between; a
//! `0xFF` byte from the host throws away whatever is half read (what
//! `guest-sync-delimited` relies on to resynchronise).

/// The longest request kept; past it the partial request is dropped.
pub const MAX_REQUEST: usize = 64 * 1024;

/// What came out of the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// One whole top-level JSON object, as text.
    Object(String),
    /// Bytes that are not an object (a stray value, a request too long):
    /// answered with an error.
    Junk,
}

/// Splits a byte stream into top-level JSON objects.
#[derive(Debug, Default)]
pub struct Framer {
    buf: Vec<u8>,
    depth: u32,
    in_string: bool,
    escaped: bool,
    /// Inside a run of bytes that are not an object.
    junk: bool,
}

impl Framer {
    /// An empty framer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        self.buf.clear();
        self.depth = 0;
        self.in_string = false;
        self.escaped = false;
    }

    /// Feed bytes; every piece they complete, in order.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Piece> {
        let mut out = Vec::new();
        for &b in bytes {
            if b == 0xFF {
                self.reset();
                self.junk = false;
                continue;
            }
            if self.depth == 0 {
                if b == b'{' {
                    self.buf.push(b);
                    self.depth = 1;
                    self.junk = false;
                } else if !b.is_ascii_whitespace() && !self.junk {
                    self.junk = true;
                    out.push(Piece::Junk);
                }
                continue;
            }
            self.buf.push(b);
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if b == b'\\' {
                    self.escaped = true;
                } else if b == b'"' {
                    self.in_string = false;
                }
            } else {
                match b {
                    b'"' => self.in_string = true,
                    b'{' | b'[' => self.depth += 1,
                    b'}' | b']' => {
                        self.depth -= 1;
                        if self.depth == 0 {
                            let text = String::from_utf8_lossy(&self.buf).into_owned();
                            self.reset();
                            out.push(Piece::Object(text));
                        }
                    }
                    _ => {}
                }
            }
            if self.buf.len() > MAX_REQUEST {
                self.reset();
                self.junk = true;
                out.push(Piece::Junk);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn objects(p: Vec<Piece>) -> Vec<String> {
        p.into_iter()
            .filter_map(|p| match p {
                Piece::Object(s) => Some(s),
                Piece::Junk => None,
            })
            .collect()
    }

    #[test]
    fn objects_in_pieces() {
        let mut f = Framer::new();
        assert!(f.push(br#"{"execute":"gue"#).is_empty());
        let got = objects(f.push(b"st-ping\"}\n{\"execute\":\"guest-info\"}"));
        assert_eq!(got, [r#"{"execute":"guest-ping"}"#, r#"{"execute":"guest-info"}"#]);
    }

    #[test]
    fn braces_in_strings_and_nesting() {
        let mut f = Framer::new();
        let got = objects(f.push(br#"{"a":"}{\"","b":{"c":[1,{"d":2}]}}"#));
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("}}"));
    }

    #[test]
    fn ff_resyncs() {
        let mut f = Framer::new();
        assert!(f.push(br#"{"execute":"half"#).is_empty());
        let got = objects(f.push(b"\xff{\"execute\":\"guest-sync-delimited\"}"));
        assert_eq!(got, [r#"{"execute":"guest-sync-delimited"}"#]);
    }

    #[test]
    fn junk_between_objects() {
        let mut f = Framer::new();
        let p = f.push(b"12 {\"x\":1}");
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], Piece::Junk);
        assert_eq!(objects(p), [r#"{"x":1}"#]);
    }

    #[test]
    fn too_long_is_dropped() {
        let mut f = Framer::new();
        let mut big = b"{\"a\":\"".to_vec();
        big.resize(MAX_REQUEST + 10, b'x');
        assert_eq!(f.push(&big), [Piece::Junk]);
        assert_eq!(objects(f.push(b"\"}{\"ok\":1}")).len(), 1);
    }
}
