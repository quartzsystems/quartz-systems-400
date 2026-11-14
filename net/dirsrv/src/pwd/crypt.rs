// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `crypt(3)` hashes as `{CRYPT}` holds them: SHA-512 (`$6$`), SHA-256
//! (`$5$`) after Drepper's specification, MD5 (`$1$`, verify only).

use std::fmt::Write as _;

use sha2::{Digest, Sha256, Sha512};

const ALPHABET: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The default and limits of `rounds=` (Drepper's).
const ROUNDS_DEFAULT: u32 = 5000;
const ROUNDS_MIN: u32 = 1000;
const ROUNDS_MAX: u32 = 999_999_999;

/// Three bytes as `n` crypt base-64 characters, low bits first.
fn b64(out: &mut String, b2: u8, b1: u8, b0: u8, n: usize) {
    let mut w = (u32::from(b2) << 16) | (u32::from(b1) << 8) | u32::from(b0);
    for _ in 0..n {
        out.push(char::from(ALPHABET[(w & 0x3f) as usize]));
        w >>= 6;
    }
}

/// Salt characters from random bytes.
#[must_use]
pub fn salt_chars(random: &[u8]) -> String {
    random
        .iter()
        .map(|b| char::from(ALPHABET[usize::from(b & 0x3f)]))
        .collect()
}

fn repeat_to(src: &[u8], len: usize) -> Vec<u8> {
    src.iter().copied().cycle().take(len).collect()
}

/// The digest function a SHA-crypt variant runs.
trait Hash {
    fn hash(parts: &[&[u8]]) -> Vec<u8>;
}

struct S256;
struct S512;

impl Hash for S256 {
    fn hash(parts: &[&[u8]]) -> Vec<u8> {
        let mut h = Sha256::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().to_vec()
    }
}

impl Hash for S512 {
    fn hash(parts: &[&[u8]]) -> Vec<u8> {
        let mut h = Sha512::new();
        for p in parts {
            h.update(p);
        }
        h.finalize().to_vec()
    }
}

fn sha_crypt_raw<H: Hash>(pw: &[u8], salt: &[u8], rounds: u32) -> Vec<u8> {
    let b = H::hash(&[pw, salt, pw]);
    let n = b.len();
    let mut a_in: Vec<u8> = Vec::new();
    a_in.extend_from_slice(pw);
    a_in.extend_from_slice(salt);
    a_in.extend_from_slice(&repeat_to(&b, pw.len()));
    let mut len = pw.len();
    while len > 0 {
        if len & 1 == 1 {
            a_in.extend_from_slice(&b);
        } else {
            a_in.extend_from_slice(pw);
        }
        len >>= 1;
    }
    let mut a = H::hash(&[&a_in]);
    let dp_in: Vec<u8> = pw.repeat(pw.len());
    let p = repeat_to(&H::hash(&[&dp_in]), pw.len());
    let ds_in: Vec<u8> = salt.repeat(16 + usize::from(a[0]));
    let s = repeat_to(&H::hash(&[&ds_in]), salt.len());
    for i in 0..rounds {
        let mut parts: Vec<&[u8]> = Vec::with_capacity(4);
        let prev = a.clone();
        parts.push(if i & 1 == 1 { &p } else { &prev });
        if i % 3 != 0 {
            parts.push(&s);
        }
        if i % 7 != 0 {
            parts.push(&p);
        }
        parts.push(if i & 1 == 1 { &prev } else { &p });
        a = H::hash(&parts);
    }
    debug_assert_eq!(a.len(), n);
    a
}

/// SHA-256 output order (Drepper's table).
const ORDER_256: [(usize, usize, usize); 10] = [
    (0, 10, 20),
    (21, 1, 11),
    (12, 22, 2),
    (3, 13, 23),
    (24, 4, 14),
    (15, 25, 5),
    (6, 16, 26),
    (27, 7, 17),
    (18, 28, 8),
    (9, 19, 29),
];

/// SHA-512 output order: each group of three rotates one place.
fn order_512() -> Vec<(usize, usize, usize)> {
    (0..21)
        .map(|k| {
            let t = [k, k + 21, k + 42];
            match k % 3 {
                0 => (t[0], t[1], t[2]),
                1 => (t[1], t[2], t[0]),
                _ => (t[2], t[0], t[1]),
            }
        })
        .collect()
}

/// `$5$` or `$6$` with `salt` (at most 16 characters used) and `rounds`
/// (`None` for the default, written without `rounds=`).
#[must_use]
pub fn sha_crypt(variant: u8, pw: &[u8], salt: &str, rounds: Option<u32>) -> String {
    let salt = &salt.as_bytes()[..salt.len().min(16)];
    let r = rounds.map_or(ROUNDS_DEFAULT, |r| r.clamp(ROUNDS_MIN, ROUNDS_MAX));
    let mut out = format!("${variant}$");
    if rounds.is_some() {
        let _ = write!(out, "rounds={r}$");
    }
    out.push_str(&String::from_utf8_lossy(salt));
    out.push('$');
    if variant == 5 {
        let d = sha_crypt_raw::<S256>(pw, salt, r);
        for (x, y, z) in ORDER_256 {
            b64(&mut out, d[x], d[y], d[z], 4);
        }
        b64(&mut out, 0, d[31], d[30], 3);
    } else {
        let d = sha_crypt_raw::<S512>(pw, salt, r);
        for (x, y, z) in order_512() {
            b64(&mut out, d[x], d[y], d[z], 4);
        }
        b64(&mut out, 0, 0, d[63], 2);
    }
    out
}

/// `$1$`: the MD5-based crypt.
#[must_use]
pub fn md5_crypt(pw: &[u8], salt: &str) -> String {
    let salt = &salt.as_bytes()[..salt.len().min(8)];
    let md5 = ntlm::digest::md5;
    let mut fin = md5(&[pw, salt, pw].concat());
    let mut ctx: Vec<u8> = [pw, b"$1$", salt].concat();
    let mut pl = pw.len();
    while pl > 0 {
        ctx.extend_from_slice(&fin[..pl.min(16)]);
        pl = pl.saturating_sub(16);
    }
    let mut i = pw.len();
    while i > 0 {
        ctx.push(if i & 1 == 1 {
            0
        } else {
            pw.first().copied().unwrap_or(0)
        });
        i >>= 1;
    }
    fin = md5(&ctx);
    for i in 0..1000 {
        let mut c: Vec<u8> = Vec::new();
        if i & 1 == 1 {
            c.extend_from_slice(pw);
        } else {
            c.extend_from_slice(&fin);
        }
        if i % 3 != 0 {
            c.extend_from_slice(salt);
        }
        if i % 7 != 0 {
            c.extend_from_slice(pw);
        }
        if i & 1 == 1 {
            c.extend_from_slice(&fin);
        } else {
            c.extend_from_slice(pw);
        }
        fin = md5(&c);
    }
    let mut out = format!("$1${}$", String::from_utf8_lossy(salt));
    for (a, b, c) in [(0, 6, 12), (1, 7, 13), (2, 8, 14), (3, 9, 15), (4, 10, 5)] {
        b64(&mut out, fin[a], fin[b], fin[c], 4);
    }
    b64(&mut out, 0, 0, fin[11], 2);
    out
}

/// Whether `pw` hashes to `stored` (`$6$...`, `$5$...`, `$1$...`).
/// Other forms (traditional DES) never match.
#[must_use]
pub fn verify(stored: &str, pw: &[u8]) -> bool {
    let computed = if let Some(rest) = stored.strip_prefix("$1$") {
        let salt = rest.split('$').next().unwrap_or("");
        md5_crypt(pw, salt)
    } else if let Some(v) = stored
        .strip_prefix("$5$")
        .map(|r| (5u8, r))
        .or_else(|| stored.strip_prefix("$6$").map(|r| (6u8, r)))
    {
        let (variant, rest) = v;
        let mut parts = rest.split('$');
        let first = parts.next().unwrap_or("");
        let (rounds, salt) = match first.strip_prefix("rounds=") {
            Some(n) => match n.parse::<u32>() {
                Ok(n) => (Some(n), parts.next().unwrap_or("")),
                Err(_) => return false,
            },
            None => (None, first),
        };
        sha_crypt(variant, pw, salt, rounds)
    } else {
        return false;
    };
    ntlm::ct_eq(computed.as_bytes(), stored.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Checked against `openssl passwd -6/-5/-1`.
    #[test]
    fn known_answers() {
        assert_eq!(
            sha_crypt(6, b"Hello world!", "saltstring", None),
            "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
        );
        assert_eq!(
            sha_crypt(5, b"Hello world!", "saltstring", None),
            "$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5"
        );
        assert_eq!(
            md5_crypt(b"Hello world!", "saltstri"),
            "$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1"
        );
        assert!(verify(
            "$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1",
            b"Hello world!"
        ));
        assert!(!verify(
            "$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1",
            b"Hello world"
        ));
        let r = sha_crypt(5, b"pw", "abc", Some(1000));
        assert!(r.starts_with("$5$rounds=1000$abc$"));
        assert!(verify(&r, b"pw"));
        assert!(!verify("abDEFghij", b"pw"));
    }
}
