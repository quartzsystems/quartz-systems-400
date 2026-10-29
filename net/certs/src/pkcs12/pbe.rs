// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password-based encryption as PKCS#12 files use it: PBES2 (PBKDF2
//! with HMAC-SHA1 or HMAC-SHA256, AES-128/256-CBC) for reading and
//! writing, and the PKCS#12 PBE schemes (SHA-1 with 3-key triple DES,
//! 40- and 128-bit RC2) for reading older files.

use cipher::block_padding::Pkcs7;
use cipher::{BlockModeDecrypt, BlockModeEncrypt, InnerIvInit, KeyIvInit};
use der::asn1::{ObjectIdentifier, OctetString};
use der::{Encode, Sequence};
use spki::AlgorithmIdentifierOwned;
use zeroize::Zeroizing;

use crate::crypto::kdf::{self, P12Hash, P12Purpose, Prf};
use crate::crypto::random;
use crate::error::{CertError, Result};

/// PBES2.
pub const PBES2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.5.13");
/// PBKDF2.
pub const PBKDF2: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.5.12");
/// hmacWithSHA1.
pub const HMAC_SHA1: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.2.7");
/// hmacWithSHA256.
pub const HMAC_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.2.9");
/// aes128-CBC.
pub const AES128_CBC: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.1.2");
/// aes256-CBC.
pub const AES256_CBC: ObjectIdentifier = ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.1.42");
/// pbeWithSHAAnd3-KeyTripleDES-CBC.
pub const PBE_SHA1_3DES: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.3");
/// pbeWithSHAAnd2-KeyTripleDES-CBC.
pub const PBE_SHA1_2DES: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.4");
/// pbeWithSHAAnd128BitRC2-CBC.
pub const PBE_SHA1_RC2_128: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.5");
/// pbeWithSHAAnd40BitRC2-CBC.
pub const PBE_SHA1_RC2_40: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.1.6");

/// Iterations used when writing.
pub const ITERATIONS: u32 = 2048;

/// PBES2-params.
#[derive(Clone, Debug, Sequence)]
pub struct Pbes2Params {
    /// The key derivation function (PBKDF2).
    pub kdf: AlgorithmIdentifierOwned,
    /// The cipher.
    pub encryption: AlgorithmIdentifierOwned,
}

/// PBKDF2-params with a specified salt.
#[derive(Clone, Debug, Sequence)]
pub struct Pbkdf2Params {
    /// The salt.
    pub salt: OctetString,
    /// Iterations.
    pub iteration_count: u32,
    /// Key length, when given.
    pub key_length: Option<u16>,
    /// The PRF; hmacWithSHA1 when absent.
    pub prf: Option<AlgorithmIdentifierOwned>,
}

/// pkcs-12PbeParams.
#[derive(Clone, Debug, Sequence)]
pub struct Pkcs12PbeParams {
    /// The salt.
    pub salt: OctetString,
    /// Iterations.
    pub iterations: u32,
}

type Aes128Dec = cbc::Decryptor<aes::Aes128>;
type Aes256Dec = cbc::Decryptor<aes::Aes256>;
type Aes256Enc = cbc::Encryptor<aes::Aes256>;
type TdesDec = cbc::Decryptor<des::TdesEde3>;
type Tdes2Dec = cbc::Decryptor<des::TdesEde2>;
type Rc2Dec = cbc::Decryptor<rc2::Rc2>;

fn bad() -> CertError {
    CertError::BadPassword
}

/// Decrypt `data` under the scheme `alg` names.
pub fn decrypt(alg: &AlgorithmIdentifierOwned, password: &str, data: &[u8]) -> Result<Vec<u8>> {
    let params = alg
        .parameters
        .as_ref()
        .ok_or_else(|| CertError::Malformed("PBE parameters".into()))?;
    if alg.oid == PBES2 {
        let p: Pbes2Params = params.decode_as()?;
        if p.kdf.oid != PBKDF2 {
            return Err(CertError::Unsupported(format!(
                "key derivation {}",
                p.kdf.oid
            )));
        }
        let k: Pbkdf2Params = p
            .kdf
            .parameters
            .as_ref()
            .ok_or_else(|| CertError::Malformed("PBKDF2 parameters".into()))?
            .decode_as()?;
        let prf = match k.prf.as_ref().map(|a| a.oid) {
            None => Prf::HmacSha1,
            Some(o) if o == HMAC_SHA1 => Prf::HmacSha1,
            Some(o) if o == HMAC_SHA256 => Prf::HmacSha256,
            Some(o) => return Err(CertError::Unsupported(format!("PBKDF2 PRF {o}"))),
        };
        let iv: OctetString = p
            .encryption
            .parameters
            .as_ref()
            .ok_or_else(|| CertError::Malformed("cipher IV".into()))?
            .decode_as()?;
        let key_len = match p.encryption.oid {
            o if o == AES128_CBC => 16,
            o if o == AES256_CBC => 32,
            o => return Err(CertError::Unsupported(format!("cipher {o}"))),
        };
        let key = Zeroizing::new(kdf::pbkdf2(
            prf,
            password.as_bytes(),
            k.salt.as_bytes(),
            k.iteration_count.max(1),
            key_len,
        ));
        return if key_len == 16 {
            Aes128Dec::new_from_slices(&key, iv.as_bytes())
                .map_err(|_| bad())?
                .decrypt_padded_vec::<Pkcs7>(data)
                .map_err(|_| bad())
        } else {
            Aes256Dec::new_from_slices(&key, iv.as_bytes())
                .map_err(|_| bad())?
                .decrypt_padded_vec::<Pkcs7>(data)
                .map_err(|_| bad())
        };
    }
    let p: Pkcs12PbeParams = params.decode_as()?;
    let pw = kdf::bmp_password(password);
    let iters = p.iterations.max(1);
    let derive = |purpose, len| {
        Zeroizing::new(kdf::pkcs12(
            P12Hash::Sha1,
            &pw,
            p.salt.as_bytes(),
            purpose,
            iters,
            len,
        ))
    };
    let iv = derive(P12Purpose::Iv, 8);
    match alg.oid {
        o if o == PBE_SHA1_3DES => {
            let key = derive(P12Purpose::Key, 24);
            TdesDec::new_from_slices(&key, &iv)
                .map_err(|_| bad())?
                .decrypt_padded_vec::<Pkcs7>(data)
                .map_err(|_| bad())
        }
        o if o == PBE_SHA1_2DES => {
            let key = derive(P12Purpose::Key, 16);
            Tdes2Dec::new_from_slices(&key, &iv)
                .map_err(|_| bad())?
                .decrypt_padded_vec::<Pkcs7>(data)
                .map_err(|_| bad())
        }
        o if o == PBE_SHA1_RC2_40 || o == PBE_SHA1_RC2_128 => {
            let (len, eff) = if o == PBE_SHA1_RC2_40 {
                (5, 40)
            } else {
                (16, 128)
            };
            let key = derive(P12Purpose::Key, len);
            let cipher = rc2::Rc2::new_with_eff_key_len(&key, eff);
            Rc2Dec::inner_iv_slice_init(cipher, &iv)
                .map_err(|_| bad())?
                .decrypt_padded_vec::<Pkcs7>(data)
                .map_err(|_| bad())
        }
        o => Err(CertError::Unsupported(format!("encryption scheme {o}"))),
    }
}

/// Encrypt with PBES2: PBKDF2-HMAC-SHA256, AES-256-CBC. Returns the
/// algorithm identifier to record and the ciphertext.
pub fn encrypt(password: &str, data: &[u8]) -> Result<(AlgorithmIdentifierOwned, Vec<u8>)> {
    let salt = random(16)?;
    let iv = random(16)?;
    let key = Zeroizing::new(kdf::pbkdf2(
        Prf::HmacSha256,
        password.as_bytes(),
        &salt,
        ITERATIONS,
        32,
    ));
    let ct = Aes256Enc::new_from_slices(&key, &iv)
        .map_err(|_| CertError::Crypto("AES-CBC".into()))?
        .encrypt_padded_vec::<Pkcs7>(data);
    let params = Pbes2Params {
        kdf: AlgorithmIdentifierOwned {
            oid: PBKDF2,
            parameters: Some(der::Any::encode_from(&Pbkdf2Params {
                salt: OctetString::new(salt)?,
                iteration_count: ITERATIONS,
                key_length: None,
                prf: Some(AlgorithmIdentifierOwned {
                    oid: HMAC_SHA256,
                    parameters: Some(der::asn1::Null.into()),
                }),
            })?),
        },
        encryption: AlgorithmIdentifierOwned {
            oid: AES256_CBC,
            parameters: Some(der::Any::encode_from(&OctetString::new(iv)?)?),
        },
    };
    Ok((
        AlgorithmIdentifierOwned {
            oid: PBES2,
            parameters: Some(der::Any::encode_from(&params)?),
        },
        ct,
    ))
}

/// A parameter block re-encoded to DER, for tests and hashing.
pub fn params_der(alg: &AlgorithmIdentifierOwned) -> Result<Vec<u8>> {
    Ok(alg.to_der()?)
}

#[cfg(test)]
mod tests {
    use der::Decode;

    use super::*;

    #[test]
    fn pbes2_round_trip_and_wrong_password() {
        let (alg, ct) = encrypt("pw", b"hello world, this is longer than a block").unwrap();
        assert_eq!(alg.oid, PBES2);
        let alg2 = AlgorithmIdentifierOwned::from_der(&params_der(&alg).unwrap()).unwrap();
        assert_eq!(
            decrypt(&alg2, "pw", &ct).unwrap(),
            b"hello world, this is longer than a block"
        );
        assert!(decrypt(&alg2, "nope", &ct).is_err());
    }

    #[test]
    fn unknown_scheme_is_unsupported() {
        let alg = AlgorithmIdentifierOwned {
            oid: ObjectIdentifier::new_unwrap("1.2.3.4"),
            parameters: Some(
                der::Any::encode_from(&Pkcs12PbeParams {
                    salt: OctetString::new(vec![1]).unwrap(),
                    iterations: 1,
                })
                .unwrap(),
            ),
        };
        assert!(matches!(
            decrypt(&alg, "x", &[0; 8]),
            Err(CertError::Unsupported(_))
        ));
    }
}
