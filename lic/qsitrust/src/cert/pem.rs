// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PEM: base64 between a `BEGIN` and an `END` line.

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `der` as a PEM document of kind `label`, in lines of 64.
#[must_use]
pub fn to_pem(label: &str, der: &[u8]) -> String {
    let mut body = String::new();
    for (i, chunk) in der.chunks(3).enumerate() {
        if i > 0 && i % 16 == 0 {
            body.push('\n');
        }
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (j, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if j <= chunk.len() {
                body.push(char::from(TABLE[(n >> shift & 0x3f) as usize]));
            } else {
                body.push('=');
            }
        }
    }
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n")
}

/// The DER of the first document of kind `label` in `text`.
#[must_use]
pub fn from_pem(label: &str, text: &str) -> Option<Vec<u8>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = text.find(&begin)? + begin.len();
    let stop = start + text[start..].find(&end)?;
    unbase64(&text[start..stop])
}

fn unbase64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(u32::from(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let clean: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let body: Vec<u8> = clean.iter().copied().take_while(|b| *b != b'=').collect();
    if body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    for chunk in body.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for n in 0..70 {
            let der: Vec<u8> = (0..n).map(|i| u8::try_from(i * 7 % 256).unwrap()).collect();
            let doc = to_pem("CERTIFICATE", &der);
            assert_eq!(from_pem("CERTIFICATE", &doc), Some(der));
        }
        assert_eq!(
            to_pem("X", b"Man"),
            "-----BEGIN X-----\nTWFu\n-----END X-----\n"
        );
        assert!(from_pem("CERTIFICATE", "-----BEGIN X-----\nTWFu\n-----END X-----\n").is_none());
        assert!(from_pem("X", "-----BEGIN X-----\nT$Fu\n-----END X-----\n").is_none());
    }
}
