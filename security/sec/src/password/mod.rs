// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Passwords: the forms `QPWDLVL` accepts, Argon2id hashes of them with a
//! salt per hash, and checking one at sign-on. Nothing here is an IBM
//! hash format; `docs/SECURITY.md` says what is stored.

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
        PwdForm::Short => p.to_ascii_uppercase(),
        PwdForm::Long => p.to_string(),
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
        assert_eq!(p.hashes.len(), 1);
        assert!(!matches(&p, "x", 0));
        p.pwd_none = true;
        assert!(!matches(&p, "Long pass phrase!", 3));
    }
}
