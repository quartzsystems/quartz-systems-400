// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Encodings: the SSH data types, message numbers, base64 and PEM, and
//! the bits of DER the keys need.

pub mod codec;
pub mod der;
pub mod msg;

pub use codec::{Reader, WireError, WireResult, Writer};

use base64ct::{Base64, Encoding};

/// Standard base64 with padding.
#[must_use]
pub fn b64(data: &[u8]) -> String {
    Base64::encode_string(data)
}

/// Decode standard base64 (whitespace removed first).
#[must_use]
pub fn unb64(text: &str) -> Option<Vec<u8>> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    Base64::decode_vec(&clean).ok()
}

/// A PEM block with `label`, 64 columns.
#[must_use]
pub fn pem(label: &str, der: &[u8]) -> String {
    let body = b64(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for chunk in body.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        out.push('\n');
    }
    out.push_str("-----END ");
    out.push_str(label);
    out.push_str("-----\n");
    out
}

/// The label and contents of the first PEM block in `text`.
#[must_use]
pub fn unpem(text: &str) -> Option<(String, Vec<u8>)> {
    let start = text.find("-----BEGIN ")?;
    let rest = &text[start + 11..];
    let label_end = rest.find("-----")?;
    let label = rest[..label_end].to_string();
    let body_start = label_end + 5;
    let end_marker = format!("-----END {label}-----");
    let end = rest.find(&end_marker)?;
    let body = &rest[body_start..end];
    Some((label, unb64(body)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_round_trip() {
        let data: Vec<u8> = (0..=255).collect();
        let text = pem("PRIVATE KEY", &data);
        assert!(text.lines().all(|l| l.len() <= 64));
        assert_eq!(unpem(&text), Some(("PRIVATE KEY".into(), data)));
    }
}
