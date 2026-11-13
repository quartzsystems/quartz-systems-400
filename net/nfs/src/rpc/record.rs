// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record marking for RPC over TCP (RFC 5531 section 11): fragments of
//! a 4-byte header (the top bit marks the last, 31 bits of length) are
//! joined into a record of at most a limit; a larger record is an
//! error and the connection is closed.

use std::io::{self, Read};

/// The largest record: 1 MiB of data and room for the largest header.
pub const MAX_RECORD: usize = (1 << 20) + 16 * 1024;

const LAST: u32 = 0x8000_0000;

/// A record over the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a record of more than {0} bytes")]
pub struct TooLarge(pub usize);

/// `msg` as one record: fragments of at most 2 GiB, the last marked.
pub fn frame(msg: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(msg.len() + 4);
    let mut chunks = msg.chunks(0x7fff_ffff).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&LAST.to_be_bytes());
    }
    while let Some(c) = chunks.next() {
        // A chunk is at most 31 bits long.
        let mut h = u32::try_from(c.len()).unwrap_or(0x7fff_ffff);
        if chunks.peek().is_none() {
            h |= LAST;
        }
        out.extend_from_slice(&h.to_be_bytes());
        out.extend_from_slice(c);
    }
    out
}

/// Records from bytes that arrive in pieces.
#[derive(Debug, Clone)]
pub struct RecordBuf {
    buf: Vec<u8>,
    pos: usize,
    record: Vec<u8>,
    max: usize,
}

impl RecordBuf {
    /// A buffer joining records of at most `max` bytes.
    pub fn new(max: usize) -> Self {
        Self {
            buf: Vec::new(),
            pos: 0,
            record: Vec::new(),
            max,
        }
    }

    /// Bytes that arrived.
    pub fn push(&mut self, b: &[u8]) {
        if self.pos > 0 && self.pos * 2 >= self.buf.len() {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        self.buf.extend_from_slice(b);
    }

    /// Bytes held that are not yet part of a record taken.
    pub fn pending(&self) -> usize {
        self.buf.len() - self.pos + self.record.len()
    }

    /// The next whole record, if one has arrived.
    pub fn next_record(&mut self) -> Result<Option<Vec<u8>>, TooLarge> {
        loop {
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let Some(h) = rest.get(..4) else {
                return Ok(None);
            };
            let h = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
            let len = (h & !LAST) as usize;
            if self.record.len().saturating_add(len) > self.max {
                return Err(TooLarge(self.max));
            }
            let Some(frag) = rest.get(4..4 + len) else {
                return Ok(None);
            };
            self.record.extend_from_slice(frag);
            self.pos += 4 + len;
            if h & LAST != 0 {
                return Ok(Some(std::mem::take(&mut self.record)));
            }
        }
    }
}

/// One record from `r`; `None` when `r` ends before a record starts.
pub fn read_record<R: Read>(r: &mut R, max: usize) -> io::Result<Option<Vec<u8>>> {
    let mut record = Vec::new();
    let mut first = true;
    loop {
        let mut h = [0u8; 4];
        if first {
            // A clean end between records.
            match r.read(&mut h[..1])? {
                0 => return Ok(None),
                _ => r.read_exact(&mut h[1..])?,
            }
            first = false;
        } else {
            r.read_exact(&mut h)?;
        }
        let h = u32::from_be_bytes(h);
        let len = (h & !LAST) as usize;
        if record.len().saturating_add(len) > max {
            return Err(io::Error::new(io::ErrorKind::InvalidData, TooLarge(max)));
        }
        // Grows as the bytes arrive; the length is not allocated up front.
        let got = r.by_ref().take(len as u64).read_to_end(&mut record)?;
        if got != len {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        if h & LAST != 0 {
            return Ok(Some(record));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_fragment() {
        let f = frame(b"abcd");
        assert_eq!(f, b"\x80\0\0\x04abcd");
        let mut rb = RecordBuf::new(MAX_RECORD);
        rb.push(&f[..3]);
        assert_eq!(rb.next_record(), Ok(None));
        rb.push(&f[3..]);
        assert_eq!(rb.next_record(), Ok(Some(b"abcd".to_vec())));
        assert_eq!(
            read_record(&mut &f[..], MAX_RECORD).unwrap(),
            Some(b"abcd".to_vec())
        );
        assert_eq!(frame(b""), b"\x80\0\0\0");
    }

    #[test]
    fn fragments_join_and_pipeline() {
        let mut s = b"\0\0\0\x02ab\0\0\0\0\x80\0\0\x01c".to_vec();
        s.extend_from_slice(&frame(b"next"));
        let mut rb = RecordBuf::new(16);
        rb.push(&s);
        assert_eq!(rb.next_record(), Ok(Some(b"abc".to_vec())));
        assert_eq!(rb.next_record(), Ok(Some(b"next".to_vec())));
        assert_eq!(rb.next_record(), Ok(None));
        let mut r = &s[..];
        assert_eq!(read_record(&mut r, 16).unwrap(), Some(b"abc".to_vec()));
        assert_eq!(read_record(&mut r, 16).unwrap(), Some(b"next".to_vec()));
        assert_eq!(read_record(&mut r, 16).unwrap(), None);
    }

    #[test]
    fn oversize_is_refused_before_it_arrives() {
        // A 2 GiB fragment announced in four bytes.
        let h = [0xff, 0xff, 0xff, 0xff];
        let mut rb = RecordBuf::new(MAX_RECORD);
        rb.push(&h);
        assert_eq!(rb.next_record(), Err(TooLarge(MAX_RECORD)));
        let e = read_record(&mut &h[..], MAX_RECORD).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        // Fragments that add up past the limit.
        let s = b"\0\0\0\x03abc\0\0\0\x03def";
        let mut rb = RecordBuf::new(5);
        rb.push(s);
        assert_eq!(rb.next_record(), Err(TooLarge(5)));
        assert!(read_record(&mut &s[..], 5).is_err());
    }

    #[test]
    fn truncated_streams() {
        assert!(read_record(&mut &b"\x80\0"[..], 16).is_err());
        assert!(read_record(&mut &b"\x80\0\0\x05ab"[..], 16).is_err());
        assert!(read_record(&mut &b"\0\0\0\x01a"[..], 16).is_err());
    }
}
