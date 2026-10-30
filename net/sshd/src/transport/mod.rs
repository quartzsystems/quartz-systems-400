// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The transport layer: identification, algorithm negotiation, key
//! exchange, key derivation and packet protection.

pub mod algo;
pub mod kex;
pub mod packet;

use std::io::BufRead;

use crate::error::{SshError, SshResult};
use algo::Negotiated;
use packet::DirKeys;

/// This server's identification string, without CR LF.
#[must_use]
pub fn server_ident() -> String {
    format!("SSH-2.0-QS400_{}", release::RELEASE.full())
}

/// Read the peer's identification line (RFC 4253 4.2), skipping any
/// lines before it; the line without CR LF.
pub fn read_ident(r: &mut dyn BufRead) -> SshResult<String> {
    for _ in 0..32 {
        let mut line = Vec::new();
        let n = std::io::Read::take(&mut *r, 256).read_until(b'\n', &mut line)?;
        if n == 0 {
            return Err(SshError::Closed);
        }
        if !line.ends_with(b"\n") {
            return Err(SshError::Protocol("identification line too long".into()));
        }
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        let text = String::from_utf8_lossy(&line).into_owned();
        if text.starts_with("SSH-2.0-") || text.starts_with("SSH-1.99-") {
            return Ok(text);
        }
        if text.starts_with("SSH-") {
            return Err(SshError::Protocol(format!("protocol version not supported: {text}")));
        }
    }
    Err(SshError::Protocol("no identification line".into()))
}

/// The keys both directions use after a key exchange: (client to
/// server, server to client).
#[must_use]
pub fn derive_keys(n: &Negotiated, k: &[u8], h: &[u8], session_id: &[u8]) -> (DirKeys, DirKeys) {
    let d = |letter, len| kex::derive(k, h, session_id, letter, len);
    let cs = DirKeys::new(d(b'C', algo::key_len(&n.cipher_cs)), &d(b'A', 12));
    let sc = DirKeys::new(d(b'D', algo::key_len(&n.cipher_sc)), &d(b'B', 12));
    (cs, sc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident_skips_banner_lines() {
        let mut r = &b"hello\r\nSSH-2.0-Client_1 extra\r\nrest"[..];
        assert_eq!(read_ident(&mut r).unwrap(), "SSH-2.0-Client_1 extra");
        assert_eq!(r, b"rest");
        assert!(read_ident(&mut &b"SSH-1.5-old\r\n"[..]).is_err());
        assert!(server_ident().starts_with("SSH-2.0-QS400_V"));
    }
}
