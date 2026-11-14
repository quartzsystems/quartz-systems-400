// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDAP messages cut from a byte stream: each is one BER element
//! (`ldap::ber::frame_len`), at most the instance's limit long.

use ldap::ber::{BerError, frame_len};

/// The bytes read and not yet a whole message.
#[derive(Debug)]
pub struct Framer {
    buf: Vec<u8>,
    max: usize,
}

impl Framer {
    /// A framer taking messages of at most `max` bytes of contents.
    #[must_use]
    pub fn new(max: usize) -> Self {
        Self {
            buf: Vec::new(),
            max,
        }
    }

    /// Bytes read.
    pub fn push(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    /// The next whole message, if one has come.
    ///
    /// # Errors
    /// A bad header or a message over the limit: the stream cannot be
    /// framed any more.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, BerError> {
        match frame_len(&self.buf, self.max)? {
            None => Ok(None),
            Some(n) => Ok(Some(self.buf.drain(..n).collect())),
        }
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ldap::msg::{LdapMessage, ProtocolOp};

    #[test]
    fn frames_across_reads() {
        let a = LdapMessage::new(1, ProtocolOp::DelRequest("cn=a".into())).encode();
        let b = LdapMessage::new(2, ProtocolOp::UnbindRequest).encode();
        let mut all = a.clone();
        all.extend_from_slice(&b);
        let mut f = Framer::new(1 << 20);
        for byte in &all[..a.len() - 1] {
            f.push(&[*byte]);
            assert_eq!(f.next_frame().unwrap(), None);
        }
        f.push(&all[a.len() - 1..]);
        assert_eq!(f.next_frame().unwrap(), Some(a));
        assert_eq!(f.next_frame().unwrap(), Some(b));
        assert!(f.is_empty());
    }

    #[test]
    fn over_the_limit() {
        let mut f = Framer::new(16);
        // A SEQUENCE claiming 4 MiB.
        f.push(&[0x30, 0x83, 0x40, 0x00, 0x00]);
        assert!(f.next_frame().is_err());
        let mut f = Framer::new(1 << 20);
        f.push(&[0x30, 0x80]);
        assert!(f.next_frame().is_err(), "indefinite length");
    }
}
