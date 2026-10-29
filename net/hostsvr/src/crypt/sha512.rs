// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password level 4: a 64-byte token from PBKDF2 with HMAC SHA-512
//! (10022 iterations, a SHA-256 salt of the user and the password's last
//! four characters) and a SHA-512 substitute over the token, the seeds,
//! the user and the sequence number.

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};

/// PBKDF2 iterations.
pub const ITERATIONS: u32 = 10022;

/// A user ID as level 4 wants it: upper case, ten characters blank
/// padded, UTF-16 big endian (20 bytes).
pub fn user_utf16(user: &str) -> Vec<u8> {
    format!("{:<10}", user.trim().to_uppercase())
        .chars()
        .take(10)
        .flat_map(|c| u16::try_from(u32::from(c)).unwrap_or(0x3F).to_be_bytes())
        .collect()
}

/// The salt: SHA-256 of the user (10 characters) and the password's last
/// four characters, blank padded, in UTF-16.
pub fn salt(user: &str, password: &[u16]) -> [u8; 32] {
    let mut chars: Vec<u16> = format!("{:<10}", user.trim().to_uppercase())
        .chars()
        .take(10)
        .map(|c| u16::try_from(u32::from(c)).unwrap_or(0x3F))
        .collect();
    let start = password.len().saturating_sub(4);
    chars.extend_from_slice(&password[start..]);
    while chars.len() < 14 {
        chars.push(0x20);
    }
    let bytes: Vec<u8> = chars.iter().flat_map(|c| c.to_be_bytes()).collect();
    Sha256::digest(&bytes).into()
}

/// PBKDF2-HMAC-SHA512 for one 64-byte block.
fn pbkdf2(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 64] {
    let mac = Hmac::<Sha512>::new_from_slice(password).expect("any key length");
    let mut m = mac.clone();
    m.update(salt);
    m.update(&1u32.to_be_bytes());
    let mut u: [u8; 64] = m.finalize().into_bytes().into();
    let mut out = u;
    for _ in 1..iterations {
        let mut m = mac.clone();
        m.update(&u);
        u = m.finalize().into_bytes().into();
        for (o, x) in out.iter_mut().zip(u) {
            *o ^= x;
        }
    }
    out
}

/// The level 4 password token. The password is used as typed (UTF-16,
/// trailing blanks removed by the caller).
pub fn token(user: &str, password: &str) -> [u8; 64] {
    let units: Vec<u16> = password.encode_utf16().collect();
    let bytes: Vec<u8> = units.iter().flat_map(|c| c.to_be_bytes()).collect();
    pbkdf2(&bytes, &salt(user, &units), ITERATIONS)
}

/// The level 4 substitute at a sequence number.
pub fn substitute(
    token: &[u8; 64],
    user: &str,
    server_seed: &[u8; 8],
    client_seed: &[u8; 8],
    sequence: [u8; 8],
) -> [u8; 64] {
    let mut h = Sha512::new();
    h.update(token);
    h.update(server_seed);
    h.update(client_seed);
    h.update(user_utf16(user));
    h.update(sequence);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_matches_rfc_style_vector() {
        // PBKDF2-HMAC-SHA512("password", "salt", 1), the widely published vector.
        let k = pbkdf2(b"password", b"salt", 1);
        assert_eq!(&k[..8], &[0x86, 0x7f, 0x70, 0xcf, 0x1a, 0xde, 0x02, 0xcf]);
        let k2 = pbkdf2(b"password", b"salt", 2);
        assert_eq!(&k2[..8], &[0xe1, 0xd9, 0xc1, 0x6a, 0xa6, 0x81, 0x70, 0x8a]);
    }

    #[test]
    fn salt_pads_short_passwords() {
        let a = salt("u", &"ab".encode_utf16().collect::<Vec<_>>());
        let b = salt("U", &"ab".encode_utf16().collect::<Vec<_>>());
        assert_eq!(a, b);
    }
}
