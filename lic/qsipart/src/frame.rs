// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Frames on a port's byte stream. The header is eight bytes, all
//! little-endian: the magic `QI`, the message kind (`u16`), the body's
//! length (`u16`), and a check (`u16`: the complement of kind XOR
//! length). A body is at most [`MAX_BODY`] bytes, so a whole frame fits
//! one write to a virtio-console port, which the kernel passes on whole.
//!
//! A port has no message boundaries of its own and its peer may write
//! anything, so [`Reader`] finds frames again after junk: a header with
//! the wrong magic, a failed check or a length over the limit costs one
//! byte and the search goes on.

use crate::Error;

/// The header's first two bytes.
pub const MAGIC: [u8; 2] = *b"QI";
/// The header's length.
pub const HEADER: usize = 8;
/// The longest body: a frame is at most 32 KiB.
pub const MAX_BODY: usize = 32 * 1024 - HEADER;
/// The most bytes a reader keeps while it waits for a frame's end.
const MAX_BUFFER: usize = 4 * (HEADER + MAX_BODY);

/// One frame: a message kind and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The kind (`service::kind`, `console::kind`).
    pub kind: u16,
    /// The encoded message.
    pub body: Vec<u8>,
}

fn check(kind: u16, len: u16) -> u16 {
    !(kind ^ len)
}

/// A frame's bytes.
///
/// # Errors
/// The body is longer than [`MAX_BODY`].
pub fn encode(kind: u16, body: &[u8]) -> Result<Vec<u8>, Error> {
    let len = u16::try_from(body.len())
        .ok()
        .filter(|n| usize::from(*n) <= MAX_BODY)
        .ok_or(Error::TooLong { len: body.len() })?;
    let mut out = Vec::with_capacity(HEADER + body.len());
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&check(kind, len).to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// Frames out of a byte stream, in whatever pieces it arrives.
#[derive(Debug, Default)]
pub struct Reader {
    buf: Vec<u8>,
    skipped: u64,
}

impl Reader {
    /// An empty reader.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bytes from the port.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > MAX_BUFFER {
            // Nothing waited for can be this long: keep the newest bytes.
            let cut = self.buf.len() - (HEADER + MAX_BODY);
            self.skipped += cut as u64;
            self.buf.drain(..cut);
        }
    }

    /// The next whole frame, passing over junk; `None` until one is in.
    pub fn next_frame(&mut self) -> Option<Frame> {
        loop {
            let Some(at) = self.buf.windows(2).position(|w| w == MAGIC) else {
                // Keep a last `Q`: it may start the next magic.
                let keep = usize::from(self.buf.last() == Some(&MAGIC[0]));
                let drop = self.buf.len() - keep;
                self.skipped += drop as u64;
                self.buf.drain(..drop);
                return None;
            };
            if at > 0 {
                self.skipped += at as u64;
                self.buf.drain(..at);
            }
            if self.buf.len() < HEADER {
                return None;
            }
            let kind = u16::from_le_bytes([self.buf[2], self.buf[3]]);
            let len = u16::from_le_bytes([self.buf[4], self.buf[5]]);
            let sum = u16::from_le_bytes([self.buf[6], self.buf[7]]);
            if sum != check(kind, len) || usize::from(len) > MAX_BODY {
                self.skipped += 1;
                self.buf.drain(..1);
                continue;
            }
            let end = HEADER + usize::from(len);
            if self.buf.len() < end {
                return None;
            }
            let body = self.buf[HEADER..end].to_vec();
            self.buf.drain(..end);
            return Some(Frame { kind, body });
        }
    }

    /// Every whole frame in.
    pub fn frames(&mut self) -> Vec<Frame> {
        std::iter::from_fn(|| self.next_frame()).collect()
    }

    /// Bytes passed over as junk so far.
    #[must_use]
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    /// Forget what is half read: the peer went away.
    pub fn reset(&mut self) {
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_in_pieces_and_after_junk() {
        let a = encode(0x0102, b"hello").unwrap();
        let b = encode(0x0001, &[]).unwrap();
        assert_eq!(&a[..2], b"QI");
        assert_eq!(a.len(), HEADER + 5);
        let mut stream = b"junkQ".to_vec();
        stream.extend_from_slice(&a);
        stream.extend_from_slice(b"QIxxxxxx");
        stream.extend_from_slice(&b);
        let mut r = Reader::new();
        let mut got = Vec::new();
        for piece in stream.chunks(3) {
            r.push(piece);
            got.extend(r.frames());
        }
        assert_eq!(
            got,
            vec![
                Frame {
                    kind: 0x0102,
                    body: b"hello".to_vec()
                },
                Frame {
                    kind: 1,
                    body: Vec::new()
                },
            ]
        );
        assert_eq!(r.skipped(), 5 + 8);
    }

    #[test]
    fn limits() {
        assert!(encode(1, &vec![0; MAX_BODY]).is_ok());
        assert!(matches!(
            encode(1, &vec![0; MAX_BODY + 1]),
            Err(Error::TooLong { .. })
        ));
        // A header claiming more than the limit is junk, even with a
        // good check.
        let len = u16::try_from(MAX_BODY + 1).unwrap();
        let mut bad = MAGIC.to_vec();
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(&len.to_le_bytes());
        bad.extend_from_slice(&check(1, len).to_le_bytes());
        let mut r = Reader::new();
        r.push(&bad);
        r.push(&encode(2, b"x").unwrap());
        assert_eq!(r.frames().len(), 1);
        // A flood without a frame stays bounded.
        let mut r = Reader::new();
        for _ in 0..100 {
            r.push(&[b'Q'; 4096]);
        }
        assert!(r.buf.len() <= MAX_BUFFER);
        assert!(r.next_frame().is_none());
        assert!(r.buf.len() <= 1);
    }
}
