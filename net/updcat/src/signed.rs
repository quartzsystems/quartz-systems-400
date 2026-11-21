// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signed text: a body, then one line `# signature <128 hex digits>`,
//! the ed25519 signature of every byte of the body. Catalogs and group
//! manifests are signed this way.

use std::fmt::Write as _;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// `body` signed with `key`: the body, a newline, the signature line.
#[must_use]
pub fn sign(body: &str, key: &SigningKey) -> String {
    let sig = key.sign(body.as_bytes()).to_bytes();
    let hex = sig.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    });
    format!("{body}\n# signature {hex}\n")
}

/// Split signed text and check it: the body and whether one of
/// `trusted` (ed25519 public keys) signed it.
#[must_use]
pub fn verify<'a>(text: &'a str, trusted: &[[u8; 32]]) -> (&'a str, bool) {
    let (body, sig) = match text.rfind("\n# signature ") {
        Some(at) => (&text[..at], Some(text[at + 13..].trim())),
        None => (text, None),
    };
    let verified = sig
        .and_then(|h| {
            let bytes: Vec<u8> = (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(h.get(i..i + 2)?, 16).ok())
                .collect::<Option<_>>()?;
            <[u8; 64]>::try_from(bytes).ok()
        })
        .is_some_and(|s| {
            trusted.iter().any(|k| {
                VerifyingKey::from_bytes(k).is_ok_and(|vk| {
                    vk.verify(body.as_bytes(), &Signature::from_bytes(&s))
                        .is_ok()
                })
            })
        });
    (body, verified)
}

/// A TOML basic string for `s`, as the publisher writes it.
#[must_use]
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_text_round_trip() {
        let key = SigningKey::from_bytes(&[3; 32]);
        let text = sign("a = 1\n", &key);
        assert!(text.starts_with("a = 1\n\n# signature "));
        let public = key.verifying_key().to_bytes();
        assert_eq!(verify(&text, &[public]), ("a = 1\n", true));
        assert!(!verify(&text, &[[9; 32]]).1);
        assert!(!verify(&text.replace("a = 1", "a = 2"), &[public]).1);
        assert!(!verify("a = 1\n", &[public]).1);
        assert_eq!(quote("x\"\\\n\u{1}"), "\"x\\\"\\\\\\n\\u0001\"");
    }
}
