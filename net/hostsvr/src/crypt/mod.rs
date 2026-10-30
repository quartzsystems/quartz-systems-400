// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password substitutes for the host servers, by password level
//! (`QPWDLVL`): DES for 0 and 1, SHA-1 for 2 and 3, SHA-512 for 4. The
//! client and server exchange seeds (`0x7001` or sign-on exchange
//! attributes) and the client proves the password with a substitute
//! computed from a token of the user and password. A password change
//! sends the new password protected with the old token and the old one
//! protected with the new token, each `XORed` with substitutes at
//! sequence numbers 2 and up.

pub mod sha512;

use crate::Error;
use crate::common::auth;

/// The encryption a password level uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// DES, 8-byte substitutes (levels 0 and 1).
    Des,
    /// SHA-1, 20-byte substitutes (levels 2 and 3).
    Sha1,
    /// SHA-512, 64-byte substitutes (level 4).
    Sha512,
}

impl Level {
    /// The encryption of a `QPWDLVL` value.
    pub fn from_qpwdlvl(level: u8) -> Self {
        match level {
            0 | 1 => Self::Des,
            2 | 3 => Self::Sha1,
            _ => Self::Sha512,
        }
    }

    /// The substitute's length.
    pub fn substitute_len(self) -> usize {
        match self {
            Self::Des => 8,
            Self::Sha1 => 20,
            Self::Sha512 => 64,
        }
    }

    /// The authentication scheme byte that announces it.
    pub fn auth_scheme(self) -> u8 {
        match self {
            Self::Des => auth::DES,
            Self::Sha1 => auth::SHA1,
            Self::Sha512 => auth::SHA512,
        }
    }
}

/// The two seeds of a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seeds {
    /// The client's seed.
    pub client: [u8; 8],
    /// The server's seed.
    pub server: [u8; 8],
}

/// Sequence number `n` as the eight bytes the algorithms use.
pub fn sequence(n: u64) -> [u8; 8] {
    n.to_be_bytes()
}

/// A password as the DES form wants it: a leading digit gets a `Q`,
/// upper case EBCDIC, at most ten characters, blank padded.
pub fn des_password(password: &str) -> Result<[u8; 10], Error> {
    let p = password.trim_end();
    let p = if p.starts_with(|c: char| c.is_ascii_digit()) {
        format!("Q{p}")
    } else {
        p.to_string()
    };
    if p.chars().count() > 10 {
        return Err(Error::Invalid(
            "a DES password has at most ten characters".into(),
        ));
    }
    Ok(pwdsub::ebcdic_name(&p))
}

fn sha1_token(user: &str, password: &str) -> [u8; 20] {
    pwdsub::sha_token(&pwdsub::utf16_name(user), &pwdsub::utf16_password(password))
}

fn substitute_with(
    level: Level,
    user: &str,
    password: &str,
    seeds: &Seeds,
    seq: [u8; 8],
) -> Result<Vec<u8>, Error> {
    Ok(match level {
        Level::Des => {
            let u = pwdsub::ebcdic_name(user);
            let token = pwdsub::des_token(&u, &des_password(password)?);
            pwdsub::des_substitute_seq(&token, &u, &seeds.server, &seeds.client, seq).to_vec()
        }
        Level::Sha1 => {
            let token = sha1_token(user, password);
            pwdsub::sha_substitute_seq(
                &token,
                &pwdsub::utf16_name(user),
                &seeds.server,
                &seeds.client,
                seq,
            )
            .to_vec()
        }
        Level::Sha512 => {
            let token = sha512::token(user, password.trim_end());
            sha512::substitute(&token, user, &seeds.server, &seeds.client, seq).to_vec()
        }
    })
}

/// The substitute a client sends for `password` (`0x1105`).
pub fn substitute(
    level: Level,
    user: &str,
    password: &str,
    seeds: &Seeds,
) -> Result<Vec<u8>, Error> {
    substitute_with(level, user, password, seeds, sequence(1))
}

/// What a change password request carries (`0x1105`, `0x110C`,
/// `0x110D`, and for the SHA forms `0x111C`/`0x111D`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordChange {
    /// The substitute of the old password.
    pub substitute: Vec<u8>,
    /// The old password protected with the new password's token.
    pub old_protected: Vec<u8>,
    /// The new password protected with the old password's token.
    pub new_protected: Vec<u8>,
    /// The old password's length in bytes (SHA forms).
    pub old_len: u32,
    /// The new password's length in bytes (SHA forms).
    pub new_len: u32,
}

/// XOR `bytes` with substitutes at the following sequence numbers,
/// `block` bytes at a time; the last block is padded with the
/// substitute's own bytes. Applying it twice gives the bytes back, so
/// a server unprotects with the same call.
fn protect(bytes: &[u8], block: usize, seq: &mut u64, sub: &dyn Fn([u8; 8]) -> Vec<u8>) -> Vec<u8> {
    let total = bytes.len().div_ceil(block).max(1) * block;
    let mut out = vec![0u8; total];
    for start in (0..total).step_by(block) {
        *seq += 1;
        let s = sub(sequence(*seq));
        for i in 0..block {
            out[start + i] = s[i] ^ bytes.get(start + i).copied().unwrap_or(0);
        }
    }
    out
}

fn des_protect(
    password: &[u8; 10],
    token: [u8; 8],
    user: &[u8; 10],
    seeds: &Seeds,
    seq: &mut u64,
) -> Vec<u8> {
    let mut sub = || {
        *seq += 1;
        pwdsub::des_substitute_seq(&token, user, &seeds.server, &seeds.client, sequence(*seq))
    };
    let mut out: Vec<u8> = sub()
        .iter()
        .zip(&password[..8])
        .map(|(a, b)| a ^ b)
        .collect();
    if password[8] != 0x40 || password[9] != 0x40 {
        let mut second = [0x40u8; 8];
        second[0] = password[8];
        second[1] = password[9];
        out.extend(sub().iter().zip(second).map(|(a, b)| a ^ b));
    }
    out
}

/// Everything a change password request needs.
pub fn change_password(
    level: Level,
    user: &str,
    old: &str,
    new: &str,
    seeds: &Seeds,
) -> Result<PasswordChange, Error> {
    let substitute = substitute(level, user, old, seeds)?;
    let mut seq = 1u64;
    match level {
        Level::Des => {
            let u = pwdsub::ebcdic_name(user);
            let (o, n) = (des_password(old)?, des_password(new)?);
            let (old_token, new_token) = (pwdsub::des_token(&u, &o), pwdsub::des_token(&u, &n));
            let new_protected = des_protect(&n, old_token, &u, seeds, &mut seq);
            let old_protected = des_protect(&o, new_token, &u, seeds, &mut seq);
            Ok(PasswordChange {
                substitute,
                old_protected,
                new_protected,
                old_len: 0,
                new_len: 0,
            })
        }
        Level::Sha1 => {
            let u16name = pwdsub::utf16_name(user);
            let u16name = &u16name;
            let (o, n) = (pwdsub::utf16_password(old), pwdsub::utf16_password(new));
            let (ot, nt) = (sha1_token(user, old), sha1_token(user, new));
            let sub = |t: [u8; 20]| {
                move |s: [u8; 8]| {
                    pwdsub::sha_substitute_seq(&t, u16name, &seeds.server, &seeds.client, s)
                        .to_vec()
                }
            };
            let new_protected = protect(&n, 20, &mut seq, &sub(ot));
            let old_protected = protect(&o, 20, &mut seq, &sub(nt));
            Ok(PasswordChange {
                substitute,
                old_protected,
                new_protected,
                old_len: len32(&o),
                new_len: len32(&n),
            })
        }
        Level::Sha512 => {
            let (o, n) = (pwdsub::utf16_password(old), pwdsub::utf16_password(new));
            let (ot, nt) = (
                sha512::token(user, old.trim_end()),
                sha512::token(user, new.trim_end()),
            );
            let sub = |t: [u8; 64]| {
                move |s: [u8; 8]| {
                    sha512::substitute(&t, user, &seeds.server, &seeds.client, s).to_vec()
                }
            };
            let new_protected = protect(&n, 64, &mut seq, &sub(ot));
            let old_protected = protect(&o, 64, &mut seq, &sub(nt));
            Ok(PasswordChange {
                substitute,
                old_protected,
                new_protected,
                old_len: len32(&o),
                new_len: len32(&n),
            })
        }
    }
}

/// The new password of a change request, as the server recovers it
/// with the old password it knows: the protected bytes `XORed` again
/// with the old token's substitutes from sequence number 2. `new_len`
/// is the byte length the SHA forms send (`0x111D`).
pub fn recover_new_password(
    level: Level,
    user: &str,
    old: &str,
    seeds: &Seeds,
    new_protected: &[u8],
    new_len: u32,
) -> Result<String, Error> {
    let mut seq = 1u64;
    match level {
        Level::Des => {
            let u = pwdsub::ebcdic_name(user);
            let token = pwdsub::des_token(&u, &des_password(old)?);
            let mut sub = || {
                seq += 1;
                pwdsub::des_substitute_seq(&token, &u, &seeds.server, &seeds.client, sequence(seq))
            };
            let mut pw: Vec<u8> = sub()
                .iter()
                .zip(new_protected.iter().take(8))
                .map(|(a, b)| a ^ b)
                .collect();
            if new_protected.len() >= 16 {
                let s = sub();
                pw.extend(s.iter().zip(&new_protected[8..10]).map(|(a, b)| a ^ b));
            }
            Ok(ccsid::from_ebcdic(&pw).trim_end().to_string())
        }
        Level::Sha1 | Level::Sha512 => {
            let plain = if level == Level::Sha1 {
                let t = sha1_token(user, old);
                let un = pwdsub::utf16_name(user);
                protect(new_protected, 20, &mut seq, &|s| {
                    pwdsub::sha_substitute_seq(&t, &un, &seeds.server, &seeds.client, s).to_vec()
                })
            } else {
                let t = sha512::token(user, old.trim_end());
                protect(new_protected, 64, &mut seq, &|s| {
                    sha512::substitute(&t, user, &seeds.server, &seeds.client, s).to_vec()
                })
            };
            let n = (new_len as usize).min(plain.len());
            let units: Vec<u16> = plain[..n]
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                .collect();
            String::from_utf16(&units).map_err(|_| Error::Invalid("the new password is not UTF-16".into()))
        }
    }
}

fn len32(b: &[u8]) -> u32 {
    u32::try_from(b.len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEEDS: Seeds = Seeds {
        client: [1, 2, 3, 4, 5, 6, 7, 8],
        server: [8, 7, 6, 5, 4, 3, 2, 1],
    };

    #[test]
    fn lengths_by_level() {
        for (lvl, n) in [(0, 8), (2, 20), (4, 64)] {
            let l = Level::from_qpwdlvl(lvl);
            assert_eq!(substitute(l, "user", "secret", &SEEDS).unwrap().len(), n);
        }
    }

    #[test]
    fn des_matches_pwdsub_and_q_rule() {
        let s = substitute(Level::Des, "USER1", "SECRET", &SEEDS).unwrap();
        assert_eq!(
            s,
            pwdsub::des_password("USER1", "SECRET", &SEEDS.server, &SEEDS.client).to_vec()
        );
        assert_eq!(des_password("123").unwrap(), pwdsub::ebcdic_name("Q123"));
        assert!(des_password("ABCDEFGHIJK").is_err());
    }

    #[test]
    fn protection_reverses() {
        for level in [Level::Sha1, Level::Sha512] {
            let c = change_password(level, "USER1", "oldpass", "newpass1", &SEEDS).unwrap();
            assert_eq!(c.new_len, 16);
            // A server holding the old token unprotects the new password.
            let block = level.substitute_len();
            assert_eq!(c.new_protected.len(), block);
            let ot: Vec<u8> = match level {
                Level::Sha1 => sha1_token("USER1", "oldpass").to_vec(),
                _ => sha512::token("USER1", "oldpass").to_vec(),
            };
            let sub = |s: [u8; 8]| match level {
                Level::Sha1 => pwdsub::sha_substitute_seq(
                    &ot.clone().try_into().unwrap(),
                    &pwdsub::utf16_name("USER1"),
                    &SEEDS.server,
                    &SEEDS.client,
                    s,
                )
                .to_vec(),
                _ => sha512::substitute(
                    &ot.clone().try_into().unwrap(),
                    "USER1",
                    &SEEDS.server,
                    &SEEDS.client,
                    s,
                )
                .to_vec(),
            };
            let mut seq = 1;
            let plain = protect(&c.new_protected, block, &mut seq, &sub);
            assert_eq!(&plain[..16], pwdsub::utf16_password("newpass1").as_slice());
        }
    }

    #[test]
    fn des_change_has_eight_or_sixteen() {
        let c = change_password(Level::Des, "USER1", "OLD", "NEWPASSWD9", &SEEDS).unwrap();
        assert_eq!(c.old_protected.len(), 8);
        assert_eq!(c.new_protected.len(), 16);
    }
}
