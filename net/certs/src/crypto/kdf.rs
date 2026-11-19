// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key derivation: Argon2id for store passwords here; PBKDF2 and the
//! PKCS#12 KDF are `x509kit`'s.

use zeroize::Zeroizing;

use crate::error::{CertError, Result};

pub use x509kit::crypto::kdf::*;

/// Argon2id costs kept in each store's header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Argon2Params {
    /// Memory in KiB.
    pub m_cost: u32,
    /// Passes.
    pub t_cost: u32,
    /// Lanes.
    pub p_cost: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Self {
            m_cost: 19 * 1024,
            t_cost: 2,
            p_cost: 1,
        }
    }
}

/// The 32-byte store key from a password and salt.
pub fn store_key(password: &str, salt: &[u8], p: Argon2Params) -> Result<Zeroizing<[u8; 32]>> {
    let params = argon2::Params::new(p.m_cost, p.t_cost, p.p_cost, Some(32))
        .map_err(|e| CertError::Corrupt(format!("argon2 parameters: {e}")))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    a.hash_password_into(password.as_bytes(), salt, out.as_mut())
        .map_err(|e| CertError::Crypto(format!("argon2: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_key_depends_on_password_and_salt() {
        let p = Argon2Params {
            m_cost: 64,
            t_cost: 1,
            p_cost: 1,
        };
        let a = store_key("a", &[1u8; 16], p).unwrap();
        let b = store_key("b", &[1u8; 16], p).unwrap();
        let c = store_key("a", &[2u8; 16], p).unwrap();
        assert_ne!(*a, *b);
        assert_ne!(*a, *c);
        assert_eq!(*a, *store_key("a", &[1u8; 16], p).unwrap());
    }
}
