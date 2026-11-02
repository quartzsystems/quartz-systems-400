// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Frames from a byte stream that arrives in pieces: a `u32`
//! little-endian length, then that many bytes, at most
//! [`MAX_FRAME`]. A longer length is refused from its prefix alone.

use qap_proto::{Envelope, MAX_FRAME, ProtoError};

/// Bytes read and not yet a whole frame.
#[derive(Debug, Default)]
pub struct FrameReader {
    buf: Vec<u8>,
}

impl FrameReader {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add bytes read.
    pub fn push(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// The next whole envelope, `None` until one is in; an error ends
    /// the stream (the frame cannot be skipped).
    pub fn next_frame(&mut self) -> Result<Option<Envelope>, ProtoError> {
        let Some(head) = self.buf.get(..4) else {
            return Ok(None);
        };
        let n = u32::from_le_bytes(head.try_into().expect("four bytes")) as usize;
        if n > MAX_FRAME {
            return Err(ProtoError::TooLong { len: n });
        }
        if self.buf.len() < 4 + n {
            return Ok(None);
        }
        let env = Envelope::from_frame(&self.buf[4..4 + n]);
        self.buf.drain(..4 + n);
        env.map(Some)
    }

    /// Whether part of a frame is waiting.
    #[must_use]
    pub fn partial(&self) -> bool {
        !self.buf.is_empty()
    }
}

/// `env` as a length-prefixed frame.
pub fn frame(env: &Envelope) -> Result<Vec<u8>, ProtoError> {
    let b = env.to_frame()?;
    let mut out = Vec::with_capacity(4 + b.len());
    out.extend_from_slice(&u32::try_from(b.len()).expect("bounded").to_le_bytes());
    out.extend_from_slice(&b);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qap_proto::Op;

    #[test]
    fn pieces_and_limits() {
        let a = Envelope::request(1, Op::Ping, &());
        let b = Envelope::cancel(1);
        let mut bytes = frame(&a).unwrap();
        bytes.extend(frame(&b).unwrap());
        let mut r = FrameReader::new();
        for chunk in bytes.chunks(3) {
            r.push(chunk);
        }
        assert_eq!(r.next_frame().unwrap(), Some(a));
        assert_eq!(r.next_frame().unwrap(), Some(b));
        assert_eq!(r.next_frame().unwrap(), None);
        assert!(!r.partial());
        let mut r = FrameReader::new();
        r.push(&u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes());
        assert!(matches!(r.next_frame(), Err(ProtoError::TooLong { .. })));
        let mut r = FrameReader::new();
        r.push(&[2, 0, 0, 0, 9, 9]);
        assert!(r.next_frame().is_err());
    }
}
