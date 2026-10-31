// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! License keys: an ed25519 signature over the key's fields, printed
//! as eighteen groups of six characters (`docs/LICPGM.md` "License
//! keys").

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::id::{LicTerm, LoadId, ProductId};
use crate::model::Date;

/// The fields a key is bound to; ADDLICKEY takes them as parameters
/// beside the key itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyFields {
    /// `PRDID`.
    pub product: ProductId,
    /// `LICTRM`.
    pub term: LicTerm,
    /// `FEATURE`.
    pub feature: LoadId,
    /// `SERIAL`: the system serial number.
    pub serial: String,
    /// `PRCGRP`: the processor group, or `*ANY`.
    pub processor_group: String,
    /// `USGLMT`; `None` is `*NOMAX`.
    pub usage_limit: Option<u32>,
    /// `EXPDATE`; `None` is `*NONE`.
    pub expiration: Option<Date>,
    /// `VNDDTA`: up to eight characters of the vendor's.
    pub vendor_data: String,
}

/// Why a key or its fields are not acceptable.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum KeyError {
    /// Not 108 key characters.
    #[error("a license key has 108 characters in 18 groups of 6; this one has {0}")]
    Length(usize),
    /// A character outside the key alphabet.
    #[error("{0:?} cannot appear in a license key")]
    Char(char),
    /// The check characters do not match: a typing error.
    #[error("the license key was not typed correctly (group {0} or later)")]
    Check(usize),
    /// The signature does not match the fields.
    #[error("the license key does not match its product, feature, serial number or limits")]
    Signature,
    /// A field that cannot be in a key.
    #[error("license key field {0} is not valid")]
    Field(&'static str),
}

const DOMAIN: &[u8] = b"QS400-LICKEY-1\0";
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Characters in a key.
pub const KEY_CHARS: usize = 108;
/// Characters in a group.
pub const GROUP: usize = 6;

fn pad(s: &str, n: usize) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.resize(n, b' ');
    b
}

impl KeyFields {
    /// Check the fields' forms.
    pub fn validate(&self) -> Result<(), KeyError> {
        let alnum = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        };
        if self.serial.is_empty() || self.serial.len() > 8 || !alnum(&self.serial) {
            return Err(KeyError::Field("SERIAL"));
        }
        let pg = &self.processor_group;
        if pg != "*ANY" && (pg.is_empty() || pg.len() > 4 || !alnum(pg)) {
            return Err(KeyError::Field("PRCGRP"));
        }
        if self.usage_limit.is_some_and(|n| n > 999_999) {
            return Err(KeyError::Field("USGLMT"));
        }
        if self.vendor_data.len() > 8
            || !self.vendor_data.bytes().all(|b| (b' '..=b'~').contains(&b))
        {
            return Err(KeyError::Field("VNDDTA"));
        }
        Ok(())
    }

    /// The bytes the signature covers: a domain tag, then each field
    /// at a fixed width.
    #[must_use]
    pub fn message(&self) -> Vec<u8> {
        let mut m = DOMAIN.to_vec();
        m.extend(pad(self.product.as_str(), 7));
        m.extend(pad(self.term.as_str(), 6));
        m.extend(pad(self.feature.as_str(), 4));
        m.extend(pad(&self.serial, 8));
        m.extend(pad(&self.processor_group, 4));
        m.extend(self.usage_limit.unwrap_or(u32::MAX).to_be_bytes());
        m.extend(self.expiration.map_or(0, Date::ymd).to_be_bytes());
        m.extend(pad(&self.vendor_data, 8));
        m
    }

    /// Whether the key has expired on a date.
    #[must_use]
    pub fn expired(&self, today: Date) -> bool {
        self.expiration.is_some_and(|e| today > e)
    }
}

/// A license key: the 64-byte signature.
#[derive(Clone, PartialEq, Eq)]
pub struct LicenseKey {
    sig: [u8; 64],
}

impl fmt::Debug for LicenseKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LicenseKey({})", self.groups()[0])
    }
}

fn check28(sig: &[u8; 64]) -> u32 {
    let h = Sha256::new()
        .chain_update(b"QSLK")
        .chain_update(sig)
        .finalize();
    u32::from_be_bytes([h[0], h[1], h[2], h[3]]) >> 4
}

impl LicenseKey {
    /// The signature bytes.
    #[must_use]
    pub fn signature(&self) -> &[u8; 64] {
        &self.sig
    }

    /// The 108 key characters: 512 signature bits and a 28-bit check,
    /// five bits per character.
    #[must_use]
    pub fn chars(&self) -> String {
        let mut bytes = self.sig.to_vec();
        bytes.extend((check28(&self.sig) << 4).to_be_bytes());
        let mut out = String::with_capacity(KEY_CHARS);
        for i in 0..KEY_CHARS {
            let mut v = 0usize;
            for bit in i * 5..i * 5 + 5 {
                v = (v << 1) | usize::from((bytes[bit / 8] >> (7 - bit % 8)) & 1);
            }
            out.push(char::from(ALPHABET[v]));
        }
        out
    }

    /// The eighteen six-character groups.
    #[must_use]
    pub fn groups(&self) -> Vec<String> {
        let c = self.chars();
        (0..KEY_CHARS / GROUP)
            .map(|g| c[g * GROUP..(g + 1) * GROUP].to_string())
            .collect()
    }

    /// Six lines of three groups, IBM's 18-character key layout once
    /// per line.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.groups().chunks(3).map(|g| g.join(" ")).collect()
    }

    /// Read a key typed by hand: groups in any spacing, either case;
    /// `I` and `L` read as `1`, `O` as `0`.
    pub fn parse(text: &str) -> Result<Self, KeyError> {
        let mut vals = Vec::with_capacity(KEY_CHARS);
        for ch in text.chars().filter(|c| !c.is_whitespace() && *c != '-') {
            let up = match ch.to_ascii_uppercase() {
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            };
            let v = ALPHABET
                .iter()
                .position(|a| char::from(*a) == up)
                .ok_or(KeyError::Char(ch))?;
            vals.push(v);
        }
        if vals.len() != KEY_CHARS {
            return Err(KeyError::Length(vals.len()));
        }
        let mut bytes = [0u8; 68];
        for (i, v) in vals.iter().enumerate() {
            for b in 0..5 {
                let bit = i * 5 + b;
                if (v >> (4 - b)) & 1 == 1 {
                    bytes[bit / 8] |= 1 << (7 - bit % 8);
                }
            }
        }
        let sig: [u8; 64] = bytes[..64].try_into().expect("64 bytes");
        let check = u32::from_be_bytes(bytes[64..].try_into().expect("4 bytes"));
        if check != check28(&sig) << 4 {
            // The check covers the whole key; point at the first group
            // a typo could be in.
            return Err(KeyError::Check(1));
        }
        Ok(Self { sig })
    }

    /// Read a key from ADDLICKEY's `LICKEY` elements.
    pub fn from_groups(groups: &[&str]) -> Result<Self, KeyError> {
        Self::parse(&groups.join(""))
    }
}

pub(crate) fn from_sig(sig: [u8; 64]) -> LicenseKey {
    LicenseKey { sig }
}

/// Sign the fields: the build pipeline and the tests.
pub fn generate(signer: &SigningKey, f: &KeyFields) -> Result<LicenseKey, KeyError> {
    f.validate()?;
    Ok(LicenseKey {
        sig: signer.sign(&f.message()).to_bytes(),
    })
}

/// Check a key against its fields and the signer's public key.
pub fn verify(public: &[u8; 32], f: &KeyFields, key: &LicenseKey) -> Result<(), KeyError> {
    f.validate()?;
    let vk = VerifyingKey::from_bytes(public).map_err(|_| KeyError::Signature)?;
    vk.verify(&f.message(), &Signature::from_bytes(&key.sig))
        .map_err(|_| KeyError::Signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> KeyFields {
        KeyFields {
            product: "QS00WDS".parse().unwrap(),
            term: "V1".parse().unwrap(),
            feature: LoadId::feature("5102").unwrap(),
            serial: "QS40001".into(),
            processor_group: "*ANY".into(),
            usage_limit: Some(10),
            expiration: Date::new(2027, 12, 31),
            vendor_data: String::new(),
        }
    }

    #[test]
    fn chars_round_trip_and_typos_are_caught() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let k = generate(&sk, &fields()).unwrap();
        assert_eq!(k.chars().len(), 108);
        assert_eq!(k.groups().len(), 18);
        assert_eq!(k.lines().len(), 6);
        assert_eq!(k.lines()[0].len(), 20);
        let back = LicenseKey::parse(&k.lines().join("\n")).unwrap();
        assert_eq!(back, k);
        assert_eq!(LicenseKey::parse(&k.chars().to_lowercase()).unwrap(), k);
        let mut typo: Vec<char> = k.chars().chars().collect();
        typo[40] = if typo[40] == 'A' { 'B' } else { 'A' };
        let typo: String = typo.into_iter().collect();
        assert!(matches!(LicenseKey::parse(&typo), Err(KeyError::Check(_))));
        assert_eq!(LicenseKey::parse("ABC"), Err(KeyError::Length(3)));
        assert_eq!(LicenseKey::parse("U"), Err(KeyError::Char('U')));
    }

    #[test]
    fn verify_binds_every_field() {
        let sk = SigningKey::from_bytes(&[7; 32]);
        let pk = sk.verifying_key().to_bytes();
        let f = fields();
        let k = generate(&sk, &f).unwrap();
        assert!(verify(&pk, &f, &k).is_ok());
        let mut g = f.clone();
        g.serial = "QS40002".into();
        assert_eq!(verify(&pk, &g, &k), Err(KeyError::Signature));
        let mut g = f.clone();
        g.usage_limit = Some(11);
        assert_eq!(verify(&pk, &g, &k), Err(KeyError::Signature));
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes();
        assert_eq!(verify(&other, &f, &k), Err(KeyError::Signature));
    }
}
