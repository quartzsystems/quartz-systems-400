// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PKCS#12: reading a file's keys and certificates, and writing one
//! identity with its chain the way OpenSSL 3 does (PBES2 with
//! PBKDF2-HMAC-SHA256 and AES-256-CBC, a SHA-256 MAC).

pub mod asn1;
pub mod pbe;

use der::asn1::{BmpString, OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use hmac::{KeyInit, Mac};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;
use zeroize::Zeroizing;

use self::asn1::{
    CERT_BAG, CertBag, ContentInfo, DigestInfo, EncryptedContentInfo, EncryptedData,
    EncryptedPrivateKeyInfo, FRIENDLY_NAME, ID_DATA, ID_ENCRYPTED_DATA, KEY_BAG, LOCAL_KEY_ID,
    MacData, Pfx, SHA1, SHA256, SHROUDED_KEY_BAG, SafeBag, X509_CERTIFICATE,
};
use crate::crypto::kdf::{self, P12Hash, P12Purpose};
use crate::crypto::keys::PrivateKey;
use crate::crypto::random;
use crate::error::{CertError, Result};
use crate::x509::CertInfo;

/// A certificate read from a file.
#[derive(Clone, Debug)]
pub struct ReadCert {
    /// DER.
    pub der: Vec<u8>,
    /// Its `friendlyName`, if any.
    pub friendly_name: Option<String>,
    /// Its `localKeyId`, if any.
    pub local_key_id: Option<Vec<u8>>,
}

/// A private key read from a file.
pub struct ReadKey {
    /// PKCS#8 DER.
    pub pkcs8: Zeroizing<Vec<u8>>,
    /// Its `localKeyId`, if any.
    pub local_key_id: Option<Vec<u8>>,
}

/// Everything a file held.
#[derive(Default)]
pub struct Contents {
    /// The certificates, in file order.
    pub certs: Vec<ReadCert>,
    /// The private keys.
    pub keys: Vec<ReadKey>,
}

/// An identity picked out of [`Contents`].
pub struct Identity {
    /// The private key.
    pub key: PrivateKey,
    /// Its certificate, DER.
    pub cert: Vec<u8>,
    /// The other certificates of the file, chain order where known.
    pub others: Vec<Vec<u8>>,
}

impl Contents {
    /// Pair the first key with its certificate (by `localKeyId`, else by
    /// public key); the other certificates come along as `others`.
    pub fn identity(&self) -> Result<Option<Identity>> {
        let Some(k) = self.keys.first() else {
            return Ok(None);
        };
        let key = PrivateKey::from_pkcs8_der(&k.pkcs8)?;
        let spki = key.spki_der()?;
        let mut at = None;
        if let Some(id) = &k.local_key_id {
            at = self
                .certs
                .iter()
                .position(|c| c.local_key_id.as_ref() == Some(id));
        }
        if at.is_none() {
            for (i, c) in self.certs.iter().enumerate() {
                if spki_of(&c.der)? == spki {
                    at = Some(i);
                    break;
                }
            }
        }
        let Some(at) = at else {
            return Err(CertError::Invalid(
                "the file's private key has no matching certificate".into(),
            ));
        };
        let cert = self.certs[at].der.clone();
        let mut others: Vec<Vec<u8>> = self
            .certs
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != at)
            .map(|(_, c)| c.der.clone())
            .collect();
        order_chain(&cert, &mut others)?;
        Ok(Some(Identity { key, cert, others }))
    }
}

fn spki_of(der: &[u8]) -> Result<Vec<u8>> {
    let c = x509_cert::Certificate::from_der(der)?;
    Ok(c.tbs_certificate().subject_public_key_info().to_der()?)
}

/// Put `others` in chain order after `leaf` where issuers are found;
/// unrelated certificates stay at the end.
fn order_chain(leaf: &[u8], others: &mut Vec<Vec<u8>>) -> Result<()> {
    let mut ordered = Vec::new();
    let mut last = CertInfo::from_der(leaf)?;
    while !last.self_signed {
        let mut found = None;
        for (i, c) in others.iter().enumerate() {
            let info = CertInfo::from_der(c)?;
            if info.subject == last.issuer {
                found = Some((i, info));
                break;
            }
        }
        let Some((i, info)) = found else { break };
        ordered.push(others.remove(i));
        last = info;
    }
    ordered.append(others);
    *others = ordered;
    Ok(())
}

/// Read a PKCS#12 file.
pub fn read(data: &[u8], password: &str) -> Result<Contents> {
    let pfx = Pfx::from_der(data).map_err(|e| CertError::Malformed(format!("PKCS#12: {e}")))?;
    if pfx.version != 3 {
        return Err(CertError::Unsupported(format!(
            "PKCS#12 version {}",
            pfx.version
        )));
    }
    if pfx.auth_safe.content_type != ID_DATA {
        return Err(CertError::Unsupported(
            "public-key protected PKCS#12".into(),
        ));
    }
    let auth_safe: OctetString = pfx.auth_safe.content.decode_as()?;
    let auth_safe = auth_safe.as_bytes();
    if let Some(mac) = &pfx.mac_data {
        verify_mac(mac, auth_safe, password)?;
    }
    let safes: Vec<ContentInfo> = Vec::from_der(auth_safe)?;
    let mut out = Contents::default();
    for ci in safes {
        let contents: Zeroizing<Vec<u8>> = if ci.content_type == ID_DATA {
            let o: OctetString = ci.content.decode_as()?;
            Zeroizing::new(o.as_bytes().to_vec())
        } else if ci.content_type == ID_ENCRYPTED_DATA {
            let ed: EncryptedData = ci.content.decode_as()?;
            let eci = ed.encrypted_content_info;
            let ct = eci
                .encrypted_content
                .ok_or_else(|| CertError::Malformed("encrypted content".into()))?;
            Zeroizing::new(pbe::decrypt(
                &eci.content_encryption_algorithm,
                password,
                ct.as_bytes(),
            )?)
        } else {
            return Err(CertError::Unsupported(format!(
                "PKCS#12 content {}",
                ci.content_type
            )));
        };
        let bags: Vec<SafeBag> = Vec::from_der(&contents)?;
        for bag in bags {
            read_bag(&bag, password, &mut out)?;
        }
    }
    Ok(out)
}

fn read_bag(bag: &SafeBag, password: &str, out: &mut Contents) -> Result<()> {
    let (friendly_name, local_key_id) = attributes(bag.bag_attributes.as_ref());
    if bag.bag_id == CERT_BAG {
        let cb: CertBag = bag.bag_value.decode_as()?;
        if cb.cert_id != X509_CERTIFICATE {
            return Ok(());
        }
        let der: OctetString = cb.cert_value.decode_as()?;
        CertInfo::from_der(der.as_bytes())?;
        out.certs.push(ReadCert {
            der: der.as_bytes().to_vec(),
            friendly_name,
            local_key_id,
        });
    } else if bag.bag_id == KEY_BAG {
        out.keys.push(ReadKey {
            pkcs8: Zeroizing::new(bag.bag_value.to_der()?),
            local_key_id,
        });
    } else if bag.bag_id == SHROUDED_KEY_BAG {
        let epki: EncryptedPrivateKeyInfo = bag.bag_value.decode_as()?;
        let pkcs8 = pbe::decrypt(
            &epki.encryption_algorithm,
            password,
            epki.encrypted_data.as_bytes(),
        )?;
        out.keys.push(ReadKey {
            pkcs8: Zeroizing::new(pkcs8),
            local_key_id,
        });
    }
    Ok(())
}

fn attributes(attrs: Option<&SetOfVec<Attribute>>) -> (Option<String>, Option<Vec<u8>>) {
    let mut name = None;
    let mut id = None;
    if let Some(attrs) = attrs {
        for a in attrs.iter() {
            let Some(v) = a.values.iter().next() else {
                continue;
            };
            if a.oid == FRIENDLY_NAME {
                if let Ok(s) = v.decode_as::<BmpString>() {
                    name = Some(s.to_string());
                }
            } else if a.oid == LOCAL_KEY_ID
                && let Ok(o) = v.decode_as::<OctetString>()
            {
                id = Some(o.as_bytes().to_vec());
            }
        }
    }
    (name, id)
}

fn mac_hash(oid: der::asn1::ObjectIdentifier) -> Result<P12Hash> {
    match oid {
        o if o == SHA1 => Ok(P12Hash::Sha1),
        o if o == SHA256 => Ok(P12Hash::Sha256),
        o => Err(CertError::Unsupported(format!("PKCS#12 MAC hash {o}"))),
    }
}

fn compute_mac(
    hash: P12Hash,
    password: &str,
    salt: &[u8],
    iterations: u32,
    data: &[u8],
) -> Vec<u8> {
    let pw = kdf::bmp_password(password);
    let len = match hash {
        P12Hash::Sha1 => 20,
        P12Hash::Sha256 => 32,
    };
    let key = Zeroizing::new(kdf::pkcs12(
        hash,
        &pw,
        salt,
        P12Purpose::Mac,
        iterations,
        len,
    ));
    match hash {
        P12Hash::Sha1 => {
            let mut m = <hmac::Hmac<sha1::Sha1> as KeyInit>::new_from_slice(&key).expect("any key");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
        P12Hash::Sha256 => {
            let mut m =
                <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(&key).expect("any key");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }
    }
}

fn verify_mac(mac: &MacData, data: &[u8], password: &str) -> Result<()> {
    let hash = mac_hash(mac.mac.algorithm.oid)?;
    let got = compute_mac(
        hash,
        password,
        mac.mac_salt.as_bytes(),
        mac.iterations.max(1),
        data,
    );
    if constant_eq(&got, mac.mac.digest.as_bytes()) {
        Ok(())
    } else {
        Err(CertError::BadPassword)
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn bmp_attr(oid: der::asn1::ObjectIdentifier, value: Any) -> Result<Attribute> {
    let mut values = SetOfVec::new();
    values.insert(value)?;
    Ok(Attribute { oid, values })
}

fn bag_attributes(name: &str, local_id: Option<&[u8]>) -> Result<SetOfVec<Attribute>> {
    let mut set = SetOfVec::new();
    set.insert(bmp_attr(
        FRIENDLY_NAME,
        Any::encode_from(&BmpString::from_utf8(name)?)?,
    )?)?;
    if let Some(id) = local_id {
        set.insert(bmp_attr(
            LOCAL_KEY_ID,
            Any::encode_from(&OctetString::new(id)?)?,
        )?)?;
    }
    Ok(set)
}

fn cert_bag(der: &[u8], name: &str, local_id: Option<&[u8]>) -> Result<SafeBag> {
    let cb = CertBag {
        cert_id: X509_CERTIFICATE,
        cert_value: Any::encode_from(&OctetString::new(der)?)?,
    };
    Ok(SafeBag {
        bag_id: CERT_BAG,
        bag_value: Any::encode_from(&cb)?,
        bag_attributes: Some(bag_attributes(name, local_id)?),
    })
}

fn data_content(bytes: &[u8]) -> Result<ContentInfo> {
    Ok(ContentInfo {
        content_type: ID_DATA,
        content: Any::encode_from(&OctetString::new(bytes)?)?,
    })
}

fn encrypted_content(bytes: &[u8], password: &str) -> Result<ContentInfo> {
    let (alg, ct) = pbe::encrypt(password, bytes)?;
    let ed = EncryptedData {
        version: 0,
        encrypted_content_info: EncryptedContentInfo {
            content_type: ID_DATA,
            content_encryption_algorithm: alg,
            encrypted_content: Some(OctetString::new(ct)?),
        },
    };
    Ok(ContentInfo {
        content_type: ID_ENCRYPTED_DATA,
        content: Any::encode_from(&ed)?,
    })
}

/// Write one identity: `key` and `cert` (named `name`) with `chain`.
pub fn write(
    name: &str,
    key: &PrivateKey,
    cert: &[u8],
    chain: &[Vec<u8>],
    password: &str,
) -> Result<Vec<u8>> {
    let local_id = crate::crypto::sha1(cert).to_vec();
    // Certificates: an encrypted safe, as OpenSSL writes them.
    let mut cert_bags = vec![cert_bag(cert, name, Some(&local_id))?];
    for (i, c) in chain.iter().enumerate() {
        let info = CertInfo::from_der(c)?;
        let n = info
            .common_name
            .unwrap_or_else(|| format!("chain {}", i + 1));
        cert_bags.push(cert_bag(c, &n, None)?);
    }
    let certs_safe = encrypted_content(&cert_bags.to_der()?, password)?;
    // The key: a shrouded bag in a plain safe.
    let pkcs8 = key.to_pkcs8_der()?;
    let (alg, ct) = pbe::encrypt(password, &pkcs8)?;
    let epki = EncryptedPrivateKeyInfo {
        encryption_algorithm: alg,
        encrypted_data: OctetString::new(ct)?,
    };
    let key_bag = SafeBag {
        bag_id: SHROUDED_KEY_BAG,
        bag_value: Any::encode_from(&epki)?,
        bag_attributes: Some(bag_attributes(name, Some(&local_id))?),
    };
    let key_safe = data_content(&vec![key_bag].to_der()?)?;
    let auth_safe = vec![certs_safe, key_safe].to_der()?;
    let salt = random(16)?;
    let digest = compute_mac(
        P12Hash::Sha256,
        password,
        &salt,
        pbe::ITERATIONS,
        &auth_safe,
    );
    let pfx = Pfx {
        version: 3,
        auth_safe: data_content(&auth_safe)?,
        mac_data: Some(MacData {
            mac: DigestInfo {
                algorithm: AlgorithmIdentifierOwned {
                    oid: SHA256,
                    parameters: Some(der::asn1::Null.into()),
                },
                digest: OctetString::new(digest)?,
            },
            mac_salt: OctetString::new(salt)?,
            iterations: pbe::ITERATIONS,
        }),
    };
    Ok(pfx.to_der()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::build::{self, CertKind, CertSpec, Subject};

    #[test]
    fn write_then_read_pairs_key_and_chain() {
        let ca_key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let ca = build::self_signed(
            &CertSpec {
                subject: Subject::cn("Test CA"),
                kind: CertKind::Ca,
                san: vec![],
                not_before: 1_700_000_000,
                days: 100,
            },
            &ca_key,
        )
        .unwrap();
        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let leaf = build::issue(
            &CertSpec::server("h", 1_700_000_000, 10),
            &key,
            &ca,
            &ca_key,
        )
        .unwrap();
        let p12 = write("my cert", &key, &leaf, std::slice::from_ref(&ca), "pw").unwrap();
        let c = read(&p12, "pw").unwrap();
        assert_eq!(c.certs.len(), 2);
        assert_eq!(c.certs[0].friendly_name.as_deref(), Some("my cert"));
        assert_eq!(c.keys.len(), 1);
        let id = c.identity().unwrap().unwrap();
        assert_eq!(id.cert, leaf);
        assert_eq!(id.others, vec![ca]);
        assert_eq!(id.key.spki_der().unwrap(), key.spki_der().unwrap());
        assert!(matches!(read(&p12, "bad"), Err(CertError::BadPassword)));
        assert!(read(b"not a file", "pw").is_err());
    }
}
