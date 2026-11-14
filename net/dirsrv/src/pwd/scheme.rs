// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password storage schemes in `userPassword` (docs/DIRSRV.md §11):
//! `{SSHA512}`, `{SSHA256}`, `{SSHA}`, `{SHA}` (verify only), `{CRYPT}`
//! (`$6$`, `$5$`, `$1$` verify only) and clear text.

use ldap::ldif::{base64_decode, base64_encode};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use super::crypt;

/// A scheme new passwords are stored with (`ibm-slapdPwEncryption`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scheme {
    /// `{SSHA512}`: salted SHA-512, QS/400's default.
    #[default]
    Ssha512,
    /// `{SSHA256}`: salted SHA-256.
    Ssha256,
    /// `{SSHA}`: salted SHA-1.
    Ssha,
    /// `{SHA}`: unsalted SHA-1 (IBM's `sha`).
    Sha,
    /// `{CRYPT}$6$`: SHA-512 crypt (IBM's `crypt`).
    Crypt,
    /// Clear text (`none`).
    Clear,
}

/// Why an `ibm-slapdPwEncryption` value is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("password encryption {0} is not supported")]
pub struct SchemeError(pub String);

/// The salt bytes a salted scheme uses.
pub const SALT_LEN: usize = 8;

impl Scheme {
    /// The scheme `ibm-slapdPwEncryption` names. `aes128`, `aes192`,
    /// `aes256` and `md5` are refused (§14).
    ///
    /// # Errors
    /// [`SchemeError`] for anything else.
    pub fn from_config(s: &str) -> Result<Self, SchemeError> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "ssha512" => Self::Ssha512,
            "ssha256" => Self::Ssha256,
            "ssha" => Self::Ssha,
            "sha" => Self::Sha,
            "crypt" => Self::Crypt,
            "none" => Self::Clear,
            _ => return Err(SchemeError(s.into())),
        })
    }

    /// The configuration keyword.
    #[must_use]
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Ssha512 => "ssha512",
            Self::Ssha256 => "ssha256",
            Self::Ssha => "ssha",
            Self::Sha => "sha",
            Self::Crypt => "crypt",
            Self::Clear => "none",
        }
    }

    /// `pw` stored under this scheme; `salt` is random bytes (at least
    /// [`SALT_LEN`]; unsalted schemes ignore it).
    #[must_use]
    pub fn hash(self, pw: &[u8], salt: &[u8]) -> Vec<u8> {
        let salt = &salt[..salt.len().min(16)];
        match self {
            Self::Ssha512 => tagged("SSHA512", &salted::<Sha512>(pw, salt)),
            Self::Ssha256 => tagged("SSHA256", &salted::<Sha256>(pw, salt)),
            Self::Ssha => tagged("SSHA", &salted::<Sha1>(pw, salt)),
            Self::Sha => tagged("SHA", &Sha1::digest(pw)),
            Self::Crypt => {
                format!("{{CRYPT}}{}", crypt::sha_crypt(6, pw, &crypt::salt_chars(salt), None))
                    .into_bytes()
            }
            Self::Clear => pw.to_vec(),
        }
    }
}

fn salted<D: Digest>(pw: &[u8], salt: &[u8]) -> Vec<u8> {
    let mut h = D::new();
    h.update(pw);
    h.update(salt);
    let mut out = h.finalize().to_vec();
    out.extend_from_slice(salt);
    out
}

fn tagged(tag: &str, raw: &[u8]) -> Vec<u8> {
    format!("{{{tag}}}{}", base64_encode(raw)).into_bytes()
}

/// A stored value's scheme tag (`SSHA512`, `CRYPT`, ...), upper-cased,
/// when it starts with `{TAG}`.
#[must_use]
pub fn tag_of(stored: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(stored).ok()?;
    let rest = s.strip_prefix('{')?;
    let end = rest.find('}')?;
    let tag = &rest[..end];
    (!tag.is_empty() && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .then(|| tag.to_ascii_uppercase())
}

/// Whether a value is already in a scheme QS/400 reads (so a write keeps
/// it as given rather than hashing it again).
#[must_use]
pub fn is_hashed(stored: &[u8]) -> bool {
    matches!(
        tag_of(stored).as_deref(),
        Some("SSHA512" | "SSHA256" | "SSHA" | "SHA" | "CRYPT")
    )
}

fn check_salted<D: Digest>(raw: &[u8], pw: &[u8]) -> bool {
    let n = <D as Digest>::output_size();
    if raw.len() <= n {
        return false;
    }
    let (digest, salt) = raw.split_at(n);
    let mut h = D::new();
    h.update(pw);
    h.update(salt);
    ntlm::ct_eq(&h.finalize(), digest)
}

/// Whether `pw` is the password `stored` holds, whatever its scheme. A
/// value without a known tag is clear text.
#[must_use]
pub fn verify(stored: &[u8], pw: &[u8]) -> bool {
    let Some(tag) = tag_of(stored) else {
        return ntlm::ct_eq(stored, pw);
    };
    let body = &stored[tag.len() + 2..];
    match tag.as_str() {
        "CRYPT" => std::str::from_utf8(body).is_ok_and(|b| crypt::verify(b, pw)),
        "SHA" | "SSHA" | "SSHA256" | "SSHA512" => {
            let Some(raw) = base64_decode(body) else {
                return false;
            };
            match tag.as_str() {
                "SHA" => ntlm::ct_eq(&Sha1::digest(pw), &raw),
                "SSHA" => check_salted::<Sha1>(&raw, pw),
                "SSHA256" => check_salted::<Sha256>(&raw, pw),
                _ => check_salted::<Sha512>(&raw, pw),
            }
        }
        _ => ntlm::ct_eq(stored, pw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let salt = [1, 2, 3, 4, 5, 6, 7, 8];
        for s in [
            Scheme::Ssha512,
            Scheme::Ssha256,
            Scheme::Ssha,
            Scheme::Sha,
            Scheme::Crypt,
            Scheme::Clear,
        ] {
            let h = s.hash(b"secret", &salt);
            assert!(verify(&h, b"secret"), "{s:?}");
            assert!(!verify(&h, b"Secret"), "{s:?}");
            assert_eq!(is_hashed(&h), s != Scheme::Clear);
            assert_eq!(Scheme::from_config(s.keyword()), Ok(s));
        }
        assert!(Scheme::from_config("aes256").is_err());
        assert!(Scheme::from_config("md5").is_err());
    }

    #[test]
    fn known_values() {
        // {SHA} of "secret" (RFC 3112 style, base64 of SHA-1).
        assert!(verify(b"{SHA}5en6G6MezRroT3XKqkdPOmY/BfQ=", b"secret"));
        assert!(verify(
            b"{crypt}$1$saltstri$YMyguxXMBpd2TEZ.vS/3q1",
            b"Hello world!"
        ));
        assert!(Scheme::Ssha512.hash(b"x", &[0; 8]).starts_with(b"{SSHA512}"));
    }
}
