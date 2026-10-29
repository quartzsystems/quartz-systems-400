// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PEM: reading a bundle's blocks and writing them.

use crate::error::{CertError, Result};

/// One PEM block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The label between `BEGIN` and the dashes.
    pub label: String,
    /// The decoded body.
    pub der: Vec<u8>,
}

/// Every block of `text`, in order; text outside blocks is ignored (the
/// Mozilla bundle has comments and titles between its certificates).
pub fn blocks(text: &str) -> Result<Vec<Block>> {
    let mut out = Vec::new();
    let mut lines = text.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let Some(label) = line
            .strip_prefix("-----BEGIN ")
            .and_then(|l| l.strip_suffix("-----"))
        else {
            continue;
        };
        let end = format!("-----END {label}-----");
        let mut body = String::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if l == end {
                closed = true;
                break;
            }
            // Skip RFC 1421 headers (`Proc-Type:`), never used here.
            if l.contains(':') {
                continue;
            }
            body.push_str(l);
        }
        if !closed {
            return Err(CertError::Malformed(format!(
                "PEM block {label} is not closed"
            )));
        }
        let der = base64_decode(&body)
            .ok_or_else(|| CertError::Malformed(format!("PEM block {label}: bad base64")))?;
        out.push(Block {
            label: label.to_string(),
            der,
        });
    }
    Ok(out)
}

/// A block as PEM text with 64-column lines and a trailing newline.
pub fn encode(label: &str, der: &[u8]) -> String {
    let b64 = base64_encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).expect("ascii"));
        out.push('\n');
    }
    out.push_str("-----END ");
    out.push_str(label);
    out.push_str("-----\n");
    out
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode standard base64, ignoring whitespace. `None` on bad input.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0u32;
    let mut pad = 0;
    for c in text.bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            pad += 1;
            continue;
        }
        if pad > 0 {
            return None;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((acc >> bits) & 0xFF) as u8);
        }
    }
    if pad > 2 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for n in 0..20 {
            let data: Vec<u8> = (0..n)
                .map(|i: u32| u8::try_from(i * 37 % 251).unwrap())
                .collect();
            let e = base64_encode(&data);
            assert_eq!(base64_decode(&e).unwrap(), data);
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_decode("TW E=\n").unwrap(), b"Ma");
        assert!(base64_decode("T*").is_none());
    }

    #[test]
    fn blocks_skip_text_between_them() {
        let text = format!(
            "Title\n====\n# comment\n{}junk\n{}",
            encode("CERTIFICATE", b"abc"),
            encode("PRIVATE KEY", b"xyz")
        );
        let b = blocks(&text).unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].label, "CERTIFICATE");
        assert_eq!(b[0].der, b"abc");
        assert_eq!(b[1].der, b"xyz");
        assert!(blocks("-----BEGIN X-----\nYWJj\n").is_err());
    }
}
