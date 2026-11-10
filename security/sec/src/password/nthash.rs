// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NT hash `NetServer` checks `NTLMv2` with (MD4 of the password's
//! UTF-16LE form, `ntlm::nt_hash`). It is password-equivalent, so it is
//! kept only sealed: AES-GCM with the LIC partition's stash key, the
//! profile name bound in as additional data. The storage service
//! installs the sealer at start ([`install`]); without one no NT hash
//! is made. Stored as a [`PwdForm::NtHash`] item: the key's generation,
//! `$`, the sealed bytes in hex.
//!
//! Which passwords get one (`docs/NETSERVER.md` section 5): at
//! `QPWDLVL` 0 and 1 the password is one case, so the hashes of its upper
//! and lower case forms are kept (a client may send either); at 2 to 4
//! the password as typed.

use std::sync::{Arc, PoisonError, RwLock};

use crate::profile::payload::{PwdForm, PwdHash, UsrPrf};

/// Seals and unseals an NT hash. The service's holds the LIC stash key.
pub trait NtSealer: Send + Sync {
    /// The key's generation, stored beside what it sealed.
    fn generation(&self) -> u32;
    /// `plain` sealed with `aad` authenticated; `None` when it cannot.
    fn seal(&self, aad: &[u8], plain: &[u8]) -> Option<Vec<u8>>;
    /// Back; `None` when the key, the data or `aad` do not match.
    fn unseal(&self, aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>>;
}

static SEALER: RwLock<Option<Arc<dyn NtSealer>>> = RwLock::new(None);

/// Install the process's sealer (`None` removes it).
pub fn install(s: Option<Arc<dyn NtSealer>>) {
    *SEALER.write().unwrap_or_else(PoisonError::into_inner) = s;
}

/// The installed sealer.
#[must_use]
pub fn sealer() -> Option<Arc<dyn NtSealer>> {
    SEALER
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The additional data: the profile name.
fn aad(user: &str) -> Vec<u8> {
    user.trim().to_ascii_uppercase().into_bytes()
}

/// The password texts that get an NT hash at `pwdlvl`.
#[must_use]
pub fn variants(password: &str, pwdlvl: u8) -> Vec<String> {
    if pwdlvl <= 1 {
        let up = password.to_uppercase();
        let low = password.to_lowercase();
        if up == low { vec![up] } else { vec![up, low] }
    } else {
        vec![password.to_string()]
    }
}

/// The sealed NT hashes of a new password of `user` at `pwdlvl`; none
/// without a sealer.
#[must_use]
pub fn hashes_with(
    sealer: Option<&dyn NtSealer>,
    user: &str,
    password: &str,
    pwdlvl: u8,
) -> Vec<PwdHash> {
    let Some(s) = sealer else {
        return Vec::new();
    };
    variants(password, pwdlvl)
        .iter()
        .filter_map(|v| {
            let ct = s.seal(&aad(user), &ntlm::nt_hash(v))?;
            Some(PwdHash {
                form: PwdForm::NtHash,
                phc: format!("{}${}", s.generation(), super::hex(&ct)),
            })
        })
        .collect()
}

/// [`hashes_with`] the installed sealer.
#[must_use]
pub fn hashes(user: &str, password: &str, pwdlvl: u8) -> Vec<PwdHash> {
    hashes_with(sealer().as_deref(), user, password, pwdlvl)
}

/// Whether the profile keeps an NT hash.
#[must_use]
pub fn has(p: &UsrPrf) -> bool {
    p.hashes.iter().any(|h| h.form == PwdForm::NtHash)
}

/// The profile's NT hashes, unsealed. One sealed by another key (another
/// system's, restored here) or bound to another name does not open.
#[must_use]
pub fn open_with(sealer: Option<&dyn NtSealer>, p: &UsrPrf) -> Vec<[u8; 16]> {
    let Some(s) = sealer else {
        return Vec::new();
    };
    p.hashes
        .iter()
        .filter(|h| h.form == PwdForm::NtHash)
        .filter_map(|h| {
            let (generation, hex) = h.phc.split_once('$')?;
            if generation.parse::<u32>().ok()? != s.generation() {
                return None;
            }
            let plain = s.unseal(&aad(&p.name), &super::unhex(hex))?;
            <[u8; 16]>::try_from(plain.as_slice()).ok()
        })
        .collect()
}

/// A sealer for tests and host tools: a fixed key, HMAC-MD5 for the
/// tag and RC4 for the data. Not for a running system.
pub struct TestSealer;

impl TestSealer {
    const KEY: [u8; 16] = [0x5a; 16];

    fn stream(nonce: &[u8]) -> Vec<u8> {
        ntlm::digest::hmac_md5(&Self::KEY, &[b"stream", nonce]).to_vec()
    }

    fn tag(aad: &[u8], body: &[u8]) -> [u8; 16] {
        ntlm::digest::hmac_md5(&Self::KEY, &[b"tag", aad, body])
    }
}

impl NtSealer for TestSealer {
    fn generation(&self) -> u32 {
        1
    }

    fn seal(&self, aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
        let nonce = super::fresh_salt();
        let mut out = nonce.to_vec();
        out.extend(ntlm::cipher::rc4(&Self::stream(&nonce), plain));
        let tag = Self::tag(aad, &out);
        out.extend(tag);
        Some(out)
    }

    fn unseal(&self, aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 32 {
            return None;
        }
        let (body, tag) = sealed.split_at(sealed.len() - 16);
        if !ntlm::ct_eq(&Self::tag(aad, body), tag) {
            return None;
        }
        let (nonce, ct) = body.split_at(16);
        Some(ntlm::cipher::rc4(&Self::stream(nonce), ct))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_sealing() {
        let s = TestSealer;
        assert_eq!(variants("Secret1", 0), vec!["SECRET1", "secret1"]);
        assert_eq!(variants("123", 1), vec!["123"]);
        assert_eq!(variants("Secret1", 2), vec!["Secret1"]);
        assert_eq!(variants("Secret1", 4), vec!["Secret1"]);
        let mut p = UsrPrf::new("BOB", 40);
        p.hashes = hashes_with(Some(&s), "BOB", "Secret1", 0);
        assert_eq!(p.hashes.len(), 2);
        assert!(has(&p));
        let open = open_with(Some(&s), &p);
        assert_eq!(
            open,
            vec![ntlm::nt_hash("SECRET1"), ntlm::nt_hash("secret1")]
        );
        assert!(!p.hashes[0].phc.contains(&super::super::hex(&open[0])));
        // Bound to the name: another profile's copy does not open.
        let mut q = p.clone();
        q.name = "ALICE".into();
        assert!(open_with(Some(&s), &q).is_empty());
        assert!(open_with(None, &p).is_empty());
        assert!(hashes_with(None, "BOB", "x", 3).is_empty());
    }
}
