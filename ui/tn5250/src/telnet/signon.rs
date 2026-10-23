// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The password substitutes of RFC 4777: a client proves it knows the
//! password without sending it. `IBMRSEED` carries the server's random
//! seed to the client and the client's seed back; `IBMSUBSPW` carries
//! the substitute. Two encodings exist: DES for password levels 0 and
//! 1 (the password upper-cased in EBCDIC, at most 10 characters) and
//! SHA-1 for levels 2 and 3 (the password as typed, in UTF-16). Both
//! sides compute the same bytes, so the server compares.

use des::Des;
use des::cipher::{BlockCipherEncrypt, KeyInit};
use sha1::{Digest, Sha1};

/// The sequence number both encodings start at.
const SEQUENCE: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

/// A user name as the DES encoding wants it: upper case, EBCDIC, ten
/// bytes padded with blanks.
pub fn ebcdic_name(name: &str) -> [u8; 10] {
    let mut out = [0x40u8; 10];
    for (i, ch) in name
        .trim()
        .to_ascii_uppercase()
        .chars()
        .take(10)
        .enumerate()
    {
        out[i] = ccsid::byte_of(ch).unwrap_or(0x40);
    }
    out
}

fn des_encrypt(key: [u8; 8], data: [u8; 8]) -> [u8; 8] {
    let cipher = Des::new_from_slice(&key).expect("8-byte key");
    let mut block = des::cipher::Block::<Des>::from(data);
    cipher.encrypt_block(&mut block);
    block.into()
}

fn ebcdic_len(s: &[u8]) -> usize {
    s.iter().take_while(|&&b| b != 0x40 && b != 0).count()
}

/// The user ID folded to eight bytes: names of nine or ten characters
/// spread their last two over the first eight.
fn folded_user(user: &[u8; 10]) -> [u8; 8] {
    let mut w = *user;
    if ebcdic_len(user) > 8 {
        w[0] ^= w[8] & 0xC0;
        w[1] ^= (w[8] & 0x30) << 2;
        w[2] ^= (w[8] & 0x0C) << 4;
        w[3] ^= (w[8] & 0x03) << 6;
        w[4] ^= w[9] & 0xC0;
        w[5] ^= (w[9] & 0x30) << 2;
        w[6] ^= (w[9] & 0x0C) << 4;
        w[7] ^= (w[9] & 0x03) << 6;
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(&w[..8]);
    out
}

fn xor_55_shift(b: &mut [u8; 8]) {
    for x in b.iter_mut() {
        *x ^= 0x55;
    }
    for i in 0..7 {
        b[i] = (b[i] << 1) | ((b[i + 1] & 0x80) >> 7);
    }
    b[7] <<= 1;
}

/// The DES password token of a user and password (both EBCDIC, the
/// password upper case and blank padded to ten).
pub fn des_token(user: &[u8; 10], password: &[u8; 10]) -> [u8; 8] {
    let id = folded_user(user);
    let len = ebcdic_len(password);
    if len > 8 {
        let mut first = [0x40u8; 8];
        first.copy_from_slice(&password[..8]);
        let mut second = [0x40u8; 8];
        second[..len - 8].copy_from_slice(&password[8..len]);
        xor_55_shift(&mut first);
        xor_55_shift(&mut second);
        let a = des_encrypt(first, id);
        let b = des_encrypt(second, id);
        let mut out = [0u8; 8];
        for i in 0..8 {
            out[i] = a[i] ^ b[i];
        }
        out
    } else {
        let mut key = [0x40u8; 8];
        key[..len].copy_from_slice(&password[..len]);
        xor_55_shift(&mut key);
        des_encrypt(key, id)
    }
}

fn add8(a: [u8; 8], b: [u8; 8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    let mut carry = 0u16;
    for i in (0..8).rev() {
        let t = u16::from(a[i]) + u16::from(b[i]) + carry;
        carry = t >> 8;
        out[i] = (t & 0xFF) as u8;
    }
    out
}

fn xor8(a: [u8; 8], b: [u8; 8]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for i in 0..8 {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// The DES substitute (eight bytes) for a sign-on: the server's seed
/// and the client's, the user (EBCDIC, ten bytes) and the token.
pub fn des_substitute(
    token: &[u8; 8],
    user: &[u8; 10],
    server_seed: &[u8; 8],
    client_seed: &[u8; 8],
) -> [u8; 8] {
    let rdrseq = add8(SEQUENCE, *server_seed);
    let mut enc = des_encrypt(*token, rdrseq);
    enc = des_encrypt(*token, xor8(enc, *client_seed));
    let mut id8 = [0u8; 8];
    id8.copy_from_slice(&user[..8]);
    let next = xor8(xor8(id8, rdrseq), enc);
    enc = des_encrypt(*token, next);
    let mut tail = [0x40u8; 8];
    tail[0] = user[8];
    tail[1] = user[9];
    let next = xor8(xor8(rdrseq, tail), enc);
    enc = des_encrypt(*token, next);
    des_encrypt(*token, xor8(SEQUENCE, enc))
}

/// The DES substitute straight from the names: user and password as
/// typed.
pub fn des_password(
    user: &str,
    password: &str,
    server_seed: &[u8; 8],
    client_seed: &[u8; 8],
) -> [u8; 8] {
    let u = ebcdic_name(user);
    let p = ebcdic_name(password);
    let token = des_token(&u, &p);
    des_substitute(&token, &u, server_seed, client_seed)
}

/// A user name as the SHA-1 encoding wants it: upper case, ten
/// characters blank padded, UTF-16 big endian.
pub fn utf16_name(name: &str) -> Vec<u8> {
    let padded = format!("{:<10}", name.trim().to_ascii_uppercase());
    padded
        .chars()
        .take(10)
        .flat_map(|c| (c as u16).to_be_bytes())
        .collect()
}

/// A password as the SHA-1 encoding wants it: as typed, trailing
/// blanks removed, UTF-16 big endian.
pub fn utf16_password(password: &str) -> Vec<u8> {
    password
        .trim_end()
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect()
}

/// The SHA-1 token: the digest of the user (UTF-16) and the password
/// (UTF-16).
pub fn sha_token(user_utf16: &[u8], password_utf16: &[u8]) -> [u8; 20] {
    let mut h = Sha1::new();
    h.update(user_utf16);
    h.update(password_utf16);
    h.finalize().into()
}

/// The SHA-1 substitute (twenty bytes).
pub fn sha_substitute(
    token: &[u8; 20],
    user_utf16: &[u8],
    server_seed: &[u8; 8],
    client_seed: &[u8; 8],
) -> [u8; 20] {
    let mut h = Sha1::new();
    h.update(token);
    h.update(server_seed);
    h.update(client_seed);
    h.update(user_utf16);
    h.update(SEQUENCE);
    h.finalize().into()
}

/// The SHA-1 substitute straight from the names.
pub fn sha_password(
    user: &str,
    password: &str,
    server_seed: &[u8; 8],
    client_seed: &[u8; 8],
) -> [u8; 20] {
    let u = utf16_name(user);
    let token = sha_token(&u, &utf16_password(password));
    sha_substitute(&token, &u, server_seed, client_seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn des_is_deterministic_and_seed_sensitive() {
        let a = des_password("QSECOFR", "secret", &[1; 8], &[2; 8]);
        let b = des_password("QSECOFR", "secret", &[1; 8], &[2; 8]);
        let c = des_password("QSECOFR", "secret", &[1; 8], &[3; 8]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, des_password("QSECOFR", "secreu", &[1; 8], &[2; 8]));
    }

    #[test]
    fn long_names_fold() {
        assert_ne!(
            des_token(&ebcdic_name("ABCDEFGHIJ"), &ebcdic_name("X")),
            des_token(&ebcdic_name("ABCDEFGH"), &ebcdic_name("X"))
        );
    }

    #[test]
    fn sha_has_twenty_bytes() {
        let s = sha_password("CWELLMAN", "pw", &[0; 8], &[0; 8]);
        assert_eq!(s.len(), 20);
        assert_ne!(s, sha_password("CWELLMAN", "pW", &[0; 8], &[0; 8]));
    }
}
