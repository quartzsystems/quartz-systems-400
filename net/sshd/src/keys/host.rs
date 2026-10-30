// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Private keys: the server's host keys and a client's identity. Made
//! here, kept as PKCS#8 PEM; read from PKCS#8, PKCS#1, SEC 1 and
//! unencrypted OpenSSH files (Ed25519 and ECDSA).

use graviola::hashing::Sha256;
use graviola::key_agreement::p256::StaticPrivateKey as P256Key;
use graviola::signing::ecdsa::{P256, SigningKey as EcSign};
use graviola::signing::eddsa::Ed25519SigningKey;
use graviola::signing::rsa::{KeySize, SigningKey as RsaSign};

use super::public::{ECDSA_P256, ED25519, PublicKey, RSA_SHA256, RSA_SHA512};
use crate::wire::{Reader, Writer, der, pem, unpem};

/// The three host key types, in preference order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    /// Ed25519.
    Ed25519,
    /// ECDSA over P-256.
    EcdsaP256,
    /// RSA, 3072 bits when made here.
    Rsa,
}

impl KeyKind {
    /// Every kind.
    pub const ALL: [Self; 3] = [Self::Ed25519, Self::EcdsaP256, Self::Rsa];

    /// The file name OpenSSH gives this host key.
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Ed25519 => "ssh_host_ed25519_key",
            Self::EcdsaP256 => "ssh_host_ecdsa_key",
            Self::Rsa => "ssh_host_rsa_key",
        }
    }
}

/// A private key.
pub enum PrivateKey {
    /// Ed25519.
    Ed25519(Ed25519SigningKey),
    /// ECDSA P-256.
    EcdsaP256(EcSign<P256>),
    /// RSA.
    Rsa(Box<RsaSign>),
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrivateKey({})", self.public().fingerprint())
    }
}

/// A key could not be made or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    /// The random source or the key arithmetic failed.
    #[error("the key could not be generated")]
    Generate,
    /// The text is not a key this server reads.
    #[error("the key file is not in a format the server reads")]
    Format,
}

impl PrivateKey {
    /// A new key.
    pub fn generate(kind: KeyKind) -> Result<Self, KeyError> {
        Ok(match kind {
            KeyKind::Ed25519 => {
                Self::Ed25519(Ed25519SigningKey::generate().map_err(|_| KeyError::Generate)?)
            }
            KeyKind::EcdsaP256 => Self::EcdsaP256(EcSign {
                private_key: P256Key::new_random().map_err(|_| KeyError::Generate)?,
            }),
            KeyKind::Rsa => {
                Self::Rsa(Box::new(
                    RsaSign::generate(KeySize::Rsa3072).map_err(|_| KeyError::Generate)?,
                ))
            }
        })
    }

    /// Its kind.
    #[must_use]
    pub fn kind(&self) -> KeyKind {
        match self {
            Self::Ed25519(_) => KeyKind::Ed25519,
            Self::EcdsaP256(_) => KeyKind::EcdsaP256,
            Self::Rsa(_) => KeyKind::Rsa,
        }
    }

    /// The public half.
    #[must_use]
    pub fn public(&self) -> PublicKey {
        match self {
            Self::Ed25519(k) => PublicKey::Ed25519(k.public_key().as_bytes()),
            Self::EcdsaP256(k) => PublicKey::EcdsaP256(k.private_key.public_key_uncompressed().to_vec()),
            Self::Rsa(k) => {
                let mut buf = vec![0u8; 2048];
                let (n, e) = k
                    .public_key()
                    .to_pkcs1_der(&mut buf)
                    .ok()
                    .and_then(der::rsa_public_parts)
                    .unwrap_or_default();
                PublicKey::Rsa { n, e }
            }
        }
    }

    /// The signature algorithms this key makes, preferred first.
    #[must_use]
    pub fn sig_algs(&self) -> &'static [&'static str] {
        match self {
            Self::Ed25519(_) => &[ED25519],
            Self::EcdsaP256(_) => &[ECDSA_P256],
            Self::Rsa(_) => &[RSA_SHA512, RSA_SHA256],
        }
    }

    /// Sign `data` with `alg`: the signature blob (`string alg, string sig`).
    #[must_use]
    pub fn sign(&self, alg: &str, data: &[u8]) -> Option<Vec<u8>> {
        if !self.sig_algs().contains(&alg) {
            return None;
        }
        let sig = match self {
            Self::Ed25519(k) => k.sign(data).to_vec(),
            Self::EcdsaP256(k) => {
                let mut buf = [0u8; 64];
                let fixed = k.sign::<Sha256>(&[data], &mut buf).ok()?;
                let mut w = Writer::new();
                w.mpint(&fixed[..32]).mpint(&fixed[32..]);
                w.finish()
            }
            Self::Rsa(k) => {
                let mut buf = vec![0u8; k.modulus_len_bytes()];
                let s = if alg == RSA_SHA512 {
                    k.sign_pkcs1_sha512(&mut buf, data).ok()?
                } else {
                    k.sign_pkcs1_sha256(&mut buf, data).ok()?
                };
                s.to_vec()
            }
        };
        let mut w = Writer::new();
        w.str(alg).bytes(&sig);
        Some(w.finish())
    }

    /// PKCS#8 PEM (`PRIVATE KEY`).
    #[must_use]
    pub fn to_pem(&self) -> String {
        let mut buf = vec![0u8; 8192];
        let der = match self {
            Self::Ed25519(k) => k.to_pkcs8_der(&mut buf).ok(),
            Self::EcdsaP256(k) => k.to_pkcs8_der(&mut buf).ok(),
            Self::Rsa(k) => k.to_pkcs8_der(&mut buf).ok(),
        };
        pem("PRIVATE KEY", der.unwrap_or_default())
    }

    /// Read a key file.
    pub fn from_pem(text: &str) -> Result<Self, KeyError> {
        let (label, der) = unpem(text).ok_or(KeyError::Format)?;
        let key = match label.as_str() {
            "PRIVATE KEY" => Ed25519SigningKey::from_pkcs8_der(&der)
                .map(Self::Ed25519)
                .or_else(|_| EcSign::<P256>::from_pkcs8_der(&der).map(Self::EcdsaP256))
                .or_else(|_| RsaSign::from_pkcs8_der(&der).map(|k| Self::Rsa(Box::new(k))))
                .ok(),
            "RSA PRIVATE KEY" => RsaSign::from_pkcs1_der(&der)
                .map(|k| Self::Rsa(Box::new(k)))
                .ok(),
            "EC PRIVATE KEY" => EcSign::<P256>::from_sec1_der(&der)
                .map(Self::EcdsaP256)
                .ok(),
            "OPENSSH PRIVATE KEY" => openssh(&der),
            _ => None,
        };
        key.ok_or(KeyError::Format)
    }
}

/// An unencrypted `openssh-key-v1` file holding one Ed25519 or P-256 key.
fn openssh(der: &[u8]) -> Option<PrivateKey> {
    let body = der.strip_prefix(b"openssh-key-v1\0")?;
    let mut r = Reader::new(body);
    if r.str().ok()? != "none" || r.str().ok()? != "none" {
        return None;
    }
    r.bytes().ok()?;
    if r.u32().ok()? != 1 {
        return None;
    }
    r.bytes().ok()?;
    let mut p = Reader::new(r.bytes().ok()?);
    if p.u32().ok()? != p.u32().ok()? {
        return None;
    }
    match p.str().ok()? {
        ED25519 => {
            p.bytes().ok()?;
            let both = p.bytes().ok()?;
            Ed25519SigningKey::from_bytes(both.get(..32)?)
                .ok()
                .map(PrivateKey::Ed25519)
        }
        ECDSA_P256 => {
            if p.str().ok()? != "nistp256" {
                return None;
            }
            p.bytes().ok()?;
            let d = p.mpint().ok()?;
            if d.len() > 32 {
                return None;
            }
            let mut fixed = [0u8; 32];
            fixed[32 - d.len()..].copy_from_slice(d);
            P256Key::from_bytes(&fixed)
                .ok()
                .map(|k| PrivateKey::EcdsaP256(EcSign { private_key: k }))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_and_pem_round_trip() {
        for kind in [KeyKind::Ed25519, KeyKind::EcdsaP256] {
            let k = PrivateKey::generate(kind).unwrap();
            let pubk = k.public();
            for alg in k.sig_algs() {
                let sig = k.sign(alg, b"data").unwrap();
                assert!(pubk.verify(&sig, b"data"), "{alg}");
                assert!(!pubk.verify(&sig, b"other"), "{alg}");
            }
            let back = PrivateKey::from_pem(&k.to_pem()).unwrap();
            assert_eq!(back.public(), pubk);
            assert_eq!(PublicKey::from_blob(&pubk.blob()), Some(pubk));
        }
    }
}
