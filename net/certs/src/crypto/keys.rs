// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Private keys: RSA 2048/3072/4096 and ECDSA P-256/P-384, the key types
//! IBM's DCM issues. Stored as PKCS#8 DER.

use std::fmt;

use der::asn1::ObjectIdentifier;
use graviola::hashing::{Sha256, Sha384};
use graviola::signing::ecdsa::{P256, P384, SigningKey as EcKey};
use graviola::signing::rsa::{KeySize, SigningKey as RsaKey};
use serde::{Deserialize, Serialize};
use spki::AlgorithmIdentifierOwned;
use zeroize::Zeroizing;

use crate::error::{CertError, Result};

/// sha256WithRSAEncryption.
pub const SHA256_WITH_RSA: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.11");
/// ecdsa-with-SHA256.
pub const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
/// ecdsa-with-SHA384.
pub const ECDSA_WITH_SHA384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.3");

/// The key types a certificate can be made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyAlgorithm {
    /// RSA, 2048-bit modulus.
    Rsa2048,
    /// RSA, 3072-bit modulus.
    Rsa3072,
    /// RSA, 4096-bit modulus.
    Rsa4096,
    /// ECDSA on P-256.
    EcdsaP256,
    /// ECDSA on P-384.
    EcdsaP384,
}

impl KeyAlgorithm {
    /// Every algorithm, in the order commands list them.
    pub const ALL: [KeyAlgorithm; 5] = [
        KeyAlgorithm::Rsa2048,
        KeyAlgorithm::Rsa3072,
        KeyAlgorithm::Rsa4096,
        KeyAlgorithm::EcdsaP256,
        KeyAlgorithm::EcdsaP384,
    ];

    /// Whether this is an RSA key.
    pub fn is_rsa(self) -> bool {
        matches!(
            self,
            KeyAlgorithm::Rsa2048 | KeyAlgorithm::Rsa3072 | KeyAlgorithm::Rsa4096
        )
    }

    /// Key size in bits.
    pub fn bits(self) -> u32 {
        match self {
            KeyAlgorithm::Rsa2048 => 2048,
            KeyAlgorithm::Rsa3072 => 3072,
            KeyAlgorithm::Rsa4096 => 4096,
            KeyAlgorithm::EcdsaP256 => 256,
            KeyAlgorithm::EcdsaP384 => 384,
        }
    }

    /// The name commands show and accept: `*RSA2048`, `*ECDSAP256`, ...
    pub fn name(self) -> &'static str {
        match self {
            KeyAlgorithm::Rsa2048 => "*RSA2048",
            KeyAlgorithm::Rsa3072 => "*RSA3072",
            KeyAlgorithm::Rsa4096 => "*RSA4096",
            KeyAlgorithm::EcdsaP256 => "*ECDSAP256",
            KeyAlgorithm::EcdsaP384 => "*ECDSAP384",
        }
    }

    /// Parse [`KeyAlgorithm::name`], case-insensitive.
    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(s.trim()))
    }
}

/// A private key.
pub enum PrivateKey {
    /// RSA (boxed: graviola's key holds its precomputed tables).
    Rsa(Box<RsaKey>),
    /// ECDSA P-256.
    P256(EcKey<P256>),
    /// ECDSA P-384.
    P384(EcKey<P384>),
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PrivateKey({:?})", self.algorithm())
    }
}

impl PrivateKey {
    /// A new random key.
    pub fn generate(alg: KeyAlgorithm) -> Result<Self> {
        Ok(match alg {
            KeyAlgorithm::Rsa2048 => PrivateKey::Rsa(Box::new(RsaKey::generate(KeySize::Rsa2048)?)),
            KeyAlgorithm::Rsa3072 => PrivateKey::Rsa(Box::new(RsaKey::generate(KeySize::Rsa3072)?)),
            KeyAlgorithm::Rsa4096 => PrivateKey::Rsa(Box::new(RsaKey::generate(KeySize::Rsa4096)?)),
            KeyAlgorithm::EcdsaP256 => PrivateKey::P256(EcKey {
                private_key: graviola::key_agreement::p256::StaticPrivateKey::new_random()?,
            }),
            KeyAlgorithm::EcdsaP384 => PrivateKey::P384(EcKey {
                private_key: graviola::key_agreement::p384::StaticPrivateKey::new_random()?,
            }),
        })
    }

    /// Load PKCS#8 DER (RSA or EC on P-256/P-384).
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self> {
        if let Ok(k) = RsaKey::from_pkcs8_der(der) {
            return Ok(PrivateKey::Rsa(Box::new(k)));
        }
        if let Ok(k) = EcKey::<P256>::from_pkcs8_der(der) {
            return Ok(PrivateKey::P256(k));
        }
        if let Ok(k) = EcKey::<P384>::from_pkcs8_der(der) {
            return Ok(PrivateKey::P384(k));
        }
        Err(CertError::Unsupported(
            "private key: only RSA and ECDSA P-256/P-384 keys are supported".into(),
        ))
    }

    /// Load a PKCS#1 RSA key (`RSA PRIVATE KEY`).
    pub fn from_pkcs1_der(der: &[u8]) -> Result<Self> {
        Ok(PrivateKey::Rsa(Box::new(RsaKey::from_pkcs1_der(der)?)))
    }

    /// Load a SEC1 EC key (`EC PRIVATE KEY`).
    pub fn from_sec1_der(der: &[u8]) -> Result<Self> {
        if let Ok(k) = EcKey::<P256>::from_sec1_der(der) {
            return Ok(PrivateKey::P256(k));
        }
        Ok(PrivateKey::P384(EcKey::<P384>::from_sec1_der(der)?))
    }

    /// The key as PKCS#8 DER.
    pub fn to_pkcs8_der(&self) -> Result<Zeroizing<Vec<u8>>> {
        let mut buf = Zeroizing::new(vec![0u8; 8192]);
        let len = match self {
            PrivateKey::Rsa(k) => k.to_pkcs8_der(&mut buf)?.len(),
            PrivateKey::P256(k) => k.to_pkcs8_der(&mut buf)?.len(),
            PrivateKey::P384(k) => k.to_pkcs8_der(&mut buf)?.len(),
        };
        Ok(Zeroizing::new(buf[..len].to_vec()))
    }

    /// The public key as `SubjectPublicKeyInfo` DER.
    pub fn spki_der(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 2048];
        let len = match self {
            PrivateKey::Rsa(k) => k.public_key().to_spki_der(&mut buf)?.len(),
            PrivateKey::P256(k) => k.to_spki_der(&mut buf)?.len(),
            PrivateKey::P384(k) => k.to_spki_der(&mut buf)?.len(),
        };
        buf.truncate(len);
        Ok(buf)
    }

    /// The key's algorithm.
    pub fn algorithm(&self) -> KeyAlgorithm {
        match self {
            PrivateKey::Rsa(k) => match k.modulus_len_bytes() * 8 {
                0..=2048 => KeyAlgorithm::Rsa2048,
                2049..=3072 => KeyAlgorithm::Rsa3072,
                _ => KeyAlgorithm::Rsa4096,
            },
            PrivateKey::P256(_) => KeyAlgorithm::EcdsaP256,
            PrivateKey::P384(_) => KeyAlgorithm::EcdsaP384,
        }
    }

    /// The signature algorithm this key signs certificates with.
    pub fn signature_algorithm(&self) -> AlgorithmIdentifierOwned {
        match self {
            PrivateKey::Rsa(_) => AlgorithmIdentifierOwned {
                oid: SHA256_WITH_RSA,
                parameters: Some(der::asn1::Null.into()),
            },
            PrivateKey::P256(_) => AlgorithmIdentifierOwned {
                oid: ECDSA_WITH_SHA256,
                parameters: None,
            },
            PrivateKey::P384(_) => AlgorithmIdentifierOwned {
                oid: ECDSA_WITH_SHA384,
                parameters: None,
            },
        }
    }

    /// Sign `msg` with [`PrivateKey::signature_algorithm`]; ECDSA
    /// signatures are DER.
    pub fn sign(&self, msg: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 1024];
        let len = match self {
            PrivateKey::Rsa(k) => k.sign_pkcs1_sha256(&mut buf, msg)?.len(),
            PrivateKey::P256(k) => k.sign_asn1::<Sha256>(&[msg], &mut buf)?.len(),
            PrivateKey::P384(k) => k.sign_asn1::<Sha384>(&[msg], &mut buf)?.len(),
        };
        buf.truncate(len);
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ec_keys_round_trip_through_pkcs8() {
        for alg in [KeyAlgorithm::EcdsaP256, KeyAlgorithm::EcdsaP384] {
            let k = PrivateKey::generate(alg).unwrap();
            let der = k.to_pkcs8_der().unwrap();
            let back = PrivateKey::from_pkcs8_der(&der).unwrap();
            assert_eq!(back.algorithm(), alg);
            assert_eq!(back.spki_der().unwrap(), k.spki_der().unwrap());
            assert!(!k.sign(b"x").unwrap().is_empty());
        }
    }

    #[test]
    fn rsa_2048_round_trips() {
        let k = PrivateKey::generate(KeyAlgorithm::Rsa2048).unwrap();
        assert_eq!(k.algorithm(), KeyAlgorithm::Rsa2048);
        let back = PrivateKey::from_pkcs8_der(&k.to_pkcs8_der().unwrap()).unwrap();
        assert_eq!(back.spki_der().unwrap(), k.spki_der().unwrap());
        assert_eq!(k.sign(b"x").unwrap().len(), 256);
    }

    #[test]
    fn names() {
        for a in KeyAlgorithm::ALL {
            assert_eq!(KeyAlgorithm::from_name(&a.name().to_lowercase()), Some(a));
        }
        assert!(KeyAlgorithm::from_name("*DSA").is_none());
    }
}
