// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Passwords: the forms `QPWDLVL` accepts, Argon2id hashes of them with a
//! salt per hash, and checking one at sign-on. Beside the hashes a
//! profile keeps the RFC 4777 tokens of its password (DES of the short
//! form, SHA-1 of the long form) so a TN5250E client can sign on with a
//! substitute; `docs/SECURITY.md` says what is stored.

pub mod policy;
pub mod rules;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};

use crate::profile::payload::{PwdForm, PwdHash, UsrPrf};

/// Longest short-form password (`QPWDLVL` 0 and 1).
pub const SHORT_MAX: usize = 10;
/// Longest long-form password (`QPWDLVL` 2 and 3).
pub const LONG_MAX: usize = 128;

/// The form a password level signs on with.
#[must_use]
pub fn form_for(pwdlvl: u8) -> PwdForm {
    if pwdlvl <= 1 {
        PwdForm::Short
    } else {
        PwdForm::Long
    }
}

/// Whether `p` can be a short-form password: one to ten characters of
/// `A-Z 0-9 $ @ # _`, not starting with a digit, after folding to upper case.
#[must_use]
pub fn short_ok(p: &str) -> bool {
    let u = p.to_ascii_uppercase();
    !u.is_empty()
        && u.chars().count() <= SHORT_MAX
        && !u.starts_with(|c: char| c.is_ascii_digit())
        && u.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "$@#_".contains(c))
}

/// The password as the form stores it: short folds to upper case.
#[must_use]
pub fn canonical(p: &str, form: PwdForm) -> String {
    match form {
        PwdForm::Short | PwdForm::DesToken => p.to_ascii_uppercase(),
        PwdForm::Long | PwdForm::ShaToken => p.to_string(),
    }
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

/// The RFC 4777 tokens of a new password: the DES token when the
/// password has a short form, the SHA-1 token of the form the level
/// signs on with.
#[must_use]
pub fn tokens_for(p: &str, pwdlvl: u8) -> Vec<PwdHash> {
    let mut v = Vec::new();
    if short_ok(p) {
        let name = pwdsub::ebcdic_name(p);
        // The token is of the password alone: the user folds in at
        // sign-on through the substitute.
        v.push(PwdHash {
            form: PwdForm::DesToken,
            phc: hex(&name),
        });
    }
    let long = canonical(p, form_for(pwdlvl));
    v.push(PwdHash {
        form: PwdForm::ShaToken,
        phc: hex(&pwdsub::utf16_password(&long)),
    });
    v
}

/// Whether an RFC 4777 substitute proves the profile's password: the
/// twenty-byte SHA-1 form at levels 2 and 3, the eight-byte DES form at
/// 0 and 1.
#[must_use]
pub fn verify_substitute(
    prf: &UsrPrf,
    user: &str,
    s: &crate::signon::Substitute,
    pwdlvl: u8,
) -> bool {
    if prf.pwd_none {
        return false;
    }
    match s.substitute.len() {
        20 if pwdlvl >= 2 => {
            let Some(h) = prf.hash(PwdForm::ShaToken) else {
                return false;
            };
            let u = pwdsub::utf16_name(user);
            let token = pwdsub::sha_token(&u, &unhex(&h.phc));
            pwdsub::sha_substitute(&token, &u, &s.server_seed, &s.client_seed)[..]
                == s.substitute[..]
        }
        8 if pwdlvl <= 1 => {
            let Some(h) = prf.hash(PwdForm::DesToken) else {
                return false;
            };
            let u = pwdsub::ebcdic_name(user);
            let mut pw = [0x40u8; 10];
            let stored = unhex(&h.phc);
            pw[..stored.len().min(10)].copy_from_slice(&stored[..stored.len().min(10)]);
            let token = pwdsub::des_token(&u, &pw);
            pwdsub::des_substitute(&token, &u, &s.server_seed, &s.client_seed)[..]
                == s.substitute[..]
        }
        _ => false,
    }
}

/// Sixteen salt bytes from the kernel, or from the clock where there is
/// no `/dev/urandom` (host tests).
#[must_use]
pub fn fresh_salt() -> [u8; 16] {
    use std::io::Read;
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom")
        && f.read_exact(&mut b).is_ok()
    {
        return b;
    }
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = N.fetch_add(1, Ordering::Relaxed);
    b[..8].copy_from_slice(&u64::try_from(t % (1 << 64)).unwrap_or(0).to_le_bytes());
    b[8..].copy_from_slice(&n.to_le_bytes());
    b
}

/// The Argon2id PHC string of `p` in `form`.
#[must_use]
pub fn hash(p: &str, form: PwdForm, salt: [u8; 16]) -> PwdHash {
    let salt = SaltString::encode_b64(&salt).expect("16 bytes fit");
    let phc = Argon2::default()
        .hash_password(canonical(p, form).as_bytes(), &salt)
        .expect("argon2 with default parameters")
        .to_string();
    PwdHash { form, phc }
}

/// Whether `p` matches a stored hash.
#[must_use]
pub fn verify(h: &PwdHash, p: &str) -> bool {
    PasswordHash::new(&h.phc).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(canonical(p, h.form).as_bytes(), &parsed)
            .is_ok()
    })
}

/// The hashes to keep for a new password at `pwdlvl`: level 0 and 1
/// keep both forms (the long one of the folded password, so a move to
/// level 2 keeps working); level 2 keeps the long form and the short one
/// when the password fits it; level 3 keeps only the long form.
#[must_use]
pub fn hashes_for(p: &str, pwdlvl: u8) -> Vec<PwdHash> {
    let mut v = hashes_only(p, pwdlvl);
    v.extend(tokens_for(p, pwdlvl));
    v
}

fn hashes_only(p: &str, pwdlvl: u8) -> Vec<PwdHash> {
    match pwdlvl {
        0 | 1 => vec![
            hash(p, PwdForm::Short, fresh_salt()),
            hash(&p.to_ascii_uppercase(), PwdForm::Long, fresh_salt()),
        ],
        2 => {
            let mut v = vec![hash(p, PwdForm::Long, fresh_salt())];
            if short_ok(p) {
                v.push(hash(p, PwdForm::Short, fresh_salt()));
            }
            v
        }
        _ => vec![hash(p, PwdForm::Long, fresh_salt())],
    }
}

/// Whether `p` is the profile's current password at `pwdlvl`.
#[must_use]
pub fn matches(prf: &UsrPrf, p: &str, pwdlvl: u8) -> bool {
    !prf.pwd_none && prf.hash(form_for(pwdlvl)).is_some_and(|h| verify(h, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_and_levels() {
        assert!(short_ok("qsecofr"));
        assert!(!short_ok("1abc"));
        assert!(!short_ok("ELEVENCHARS"));
        assert!(!short_ok("a-b"));
        let mut p = UsrPrf::new("BOB", 40);
        p.pwd_none = false;
        p.hashes = hashes_for("Secret1", 0);
        assert!(matches(&p, "SECRET1", 0));
        assert!(matches(&p, "secret1", 1));
        assert!(matches(&p, "SECRET1", 2));
        assert!(!matches(&p, "Secret1", 2));
        p.hashes = hashes_for("Secret1", 2);
        assert!(matches(&p, "Secret1", 2));
        assert!(!matches(&p, "SECRET1", 3));
        assert!(matches(&p, "secret1", 0));
        p.hashes = hashes_for("Long pass phrase!", 3);
        assert_eq!(p.hashes.iter().filter(|h| !h.form.is_token()).count(), 1);
        assert!(p.hash(PwdForm::ShaToken).is_some());
        assert!(p.hash(PwdForm::DesToken).is_none());
        assert!(!matches(&p, "x", 0));
        p.pwd_none = true;
        assert!(!matches(&p, "Long pass phrase!", 3));
    }

    #[test]
    fn substitutes_prove_the_password() {
        let (server, client) = ([1u8; 8], [2u8; 8]);
        let mut p = UsrPrf::new("BOB", 40);
        p.pwd_none = false;
        p.hashes = hashes_for("Secret1", 2);
        let sha = crate::signon::Substitute {
            server_seed: server,
            client_seed: client,
            substitute: pwdsub::sha_password("bob", "Secret1", &server, &client).to_vec(),
        };
        assert!(verify_substitute(&p, "BOB", &sha, 2));
        assert!(
            !verify_substitute(&p, "BOB", &sha, 0),
            "SHA-1 is not for level 0"
        );
        let wrong = crate::signon::Substitute {
            substitute: pwdsub::sha_password("bob", "Secret2", &server, &client).to_vec(),
            ..sha.clone()
        };
        assert!(!verify_substitute(&p, "BOB", &wrong, 2));
        p.hashes = hashes_for("Secret1", 0);
        let des = crate::signon::Substitute {
            server_seed: server,
            client_seed: client,
            substitute: pwdsub::des_password("bob", "secret1", &server, &client).to_vec(),
        };
        assert!(verify_substitute(&p, "BOB", &des, 0));
        assert!(
            !verify_substitute(&p, "BOB", &des, 2),
            "DES is not for level 2"
        );
        let other_user = pwdsub::des_password("ALICE", "secret1", &server, &client).to_vec();
        assert!(!verify_substitute(
            &p,
            "BOB",
            &crate::signon::Substitute {
                substitute: other_user,
                ..des
            },
            0
        ));
    }
}
