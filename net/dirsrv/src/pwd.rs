// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password storage schemes and password policy (docs/DIRSRV.md §11).
//!
//! [`ssha512`] is the scheme the configuration's `ibm-slapdAdminPW` is
//! kept in; [`verify`] checks a clear password against a stored value
//! of the schemes built here.

use sha2::{Digest, Sha512};

/// The `{SSHA512}` prefix.
pub const SSHA512: &str = "{SSHA512}";

/// Salt bytes [`ssha512`] takes from [`fresh_salt`].
pub const SALT_LEN: usize = 16;

/// `{SSHA512}` and the base64 of SHA-512(password, salt) followed by
/// the salt.
#[must_use]
pub fn ssha512(password: &[u8], salt: &[u8]) -> String {
    let mut h = Sha512::new();
    h.update(password);
    h.update(salt);
    let mut out: Vec<u8> = h.finalize().to_vec();
    out.extend_from_slice(salt);
    format!("{SSHA512}{}", ldap::ldif::base64_encode(&out))
}

/// [`ssha512`] with a fresh salt.
#[must_use]
pub fn hash(password: &[u8]) -> String {
    ssha512(password, &fresh_salt())
}

/// Whether `password` is the one `stored` holds; a scheme not built
/// here never matches.
#[must_use]
pub fn verify(password: &[u8], stored: &str) -> bool {
    let Some(b64) = stored
        .get(..SSHA512.len())
        .filter(|p| p.eq_ignore_ascii_case(SSHA512))
        .map(|_| &stored[SSHA512.len()..])
    else {
        return false;
    };
    let Some(raw) = ldap::ldif::base64_decode(b64.as_bytes()) else {
        return false;
    };
    if raw.len() <= 64 {
        return false;
    }
    let salt = &raw[64..];
    let want = ssha512(password, salt);
    // Compared whole, every byte, whatever the first difference.
    let a = &want.as_bytes()[SSHA512.len()..];
    let b = b64.as_bytes();
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Salt from the kernel, or from the clock where there is no
/// `/dev/urandom` (host tests).
#[must_use]
pub fn fresh_salt() -> [u8; SALT_LEN] {
    use std::io::Read;
    let mut s = [0u8; SALT_LEN];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom")
        && f.read_exact(&mut s).is_ok()
    {
        return s;
    }
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let h = Sha512::digest(t.to_le_bytes());
    s.copy_from_slice(&h[..SALT_LEN]);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssha512_round_trip() {
        let h = ssha512(b"secret", &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(h.starts_with("{SSHA512}"));
        // 64 digest bytes and 8 salt bytes, base64.
        assert_eq!(h.len() - SSHA512.len(), 96);
        assert!(verify(b"secret", &h));
        assert!(!verify(b"Secret", &h));
        let fresh = hash(b"pw");
        assert!(verify(b"pw", &fresh));
        assert!(!verify(b"pw", "{SHA}abc"));
        assert!(!verify(b"pw", "{SSHA512}!!"));
    }
}
