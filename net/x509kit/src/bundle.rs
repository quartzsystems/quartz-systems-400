// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A certificate chain and its key as an administrator brings them: PEM
//! text (certificates and a `PRIVATE KEY`, `RSA PRIVATE KEY`,
//! `EC PRIVATE KEY` or `ENCRYPTED PRIVATE KEY` block, in any order) or a
//! PKCS#12 file. Read, the key matched against the certificates, and the
//! chain put in order: the key's certificate first, then each issuer.

use der::Decode;
use zeroize::Zeroizing;

use crate::crypto::keys::PrivateKey;
use crate::error::{Error, Result};
use crate::x509::CertInfo;
use crate::{pem, pkcs12};

/// One certificate of a bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cert {
    /// DER.
    pub der: Vec<u8>,
    /// What it holds.
    pub info: CertInfo,
}

/// What a bundle held.
#[derive(Debug)]
pub struct Bundle {
    /// The leaf first (the key's certificate, or the first one when no
    /// certificate matches the key or there is none), then its issuers
    /// in order, then any other certificates as they came.
    pub chain: Vec<Cert>,
    /// The private key, if there was one.
    pub key: Option<PrivateKey>,
    /// Whether the key belongs to the leaf; `None` without a key.
    pub key_matches: Option<bool>,
}

impl Bundle {
    /// The leaf certificate.
    pub fn leaf(&self) -> &Cert {
        &self.chain[0]
    }
}

/// Whether `key` is the private key of certificate `cert_der`.
pub fn key_matches(cert_der: &[u8], key: &PrivateKey) -> Result<bool> {
    Ok(spki_of(cert_der)? == key.spki_der()?)
}

fn spki_of(der: &[u8]) -> Result<Vec<u8>> {
    use der::Encode;
    let c = x509_cert::Certificate::from_der(der)?;
    Ok(c.tbs_certificate().subject_public_key_info().to_der()?)
}

/// Read PEM text. `key_password` opens an `ENCRYPTED PRIVATE KEY`
/// block (PBES2, or PKCS#12's own ciphers).
pub fn read_pem(text: &str, key_password: Option<&str>) -> Result<Bundle> {
    let mut certs = Vec::new();
    let mut key = None;
    for b in pem::blocks(text)? {
        match b.label.as_str() {
            "CERTIFICATE" => certs.push(b.der),
            "PRIVATE KEY" => key = Some(PrivateKey::from_pkcs8_der(&Zeroizing::new(b.der))?),
            "RSA PRIVATE KEY" => key = Some(PrivateKey::from_pkcs1_der(&Zeroizing::new(b.der))?),
            "EC PRIVATE KEY" => key = Some(PrivateKey::from_sec1_der(&Zeroizing::new(b.der))?),
            "ENCRYPTED PRIVATE KEY" => {
                let pw = key_password.ok_or_else(|| {
                    Error::Invalid("the private key is encrypted; a password is needed".into())
                })?;
                let e = pkcs12::asn1::EncryptedPrivateKeyInfo::from_der(&b.der)?;
                let pkcs8 = Zeroizing::new(pkcs12::pbe::decrypt(
                    &e.encryption_algorithm,
                    pw,
                    e.encrypted_data.as_bytes(),
                )?);
                key = Some(PrivateKey::from_pkcs8_der(&pkcs8)?);
            }
            _ => {}
        }
    }
    assemble(certs, key)
}

/// Read a PKCS#12 file with its password.
pub fn read_pkcs12(data: &[u8], password: &str) -> Result<Bundle> {
    let contents = pkcs12::read(data, password)?;
    match contents.identity()? {
        Some(id) => {
            let mut certs = vec![id.cert];
            certs.extend(id.others);
            assemble(certs, Some(id.key))
        }
        None => assemble(contents.certs.into_iter().map(|c| c.der).collect(), None),
    }
}

fn assemble(certs: Vec<Vec<u8>>, key: Option<PrivateKey>) -> Result<Bundle> {
    if certs.is_empty() {
        return Err(Error::Invalid("no certificate in the bundle".into()));
    }
    let mut rest = certs
        .into_iter()
        .map(|der| {
            let info = CertInfo::from_der(&der)?;
            Ok(Cert { der, info })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut key_matches = None;
    let mut leaf_at = 0;
    if let Some(k) = &key {
        let spki = k.spki_der()?;
        let at = rest
            .iter()
            .position(|c| spki_of(&c.der).ok().as_deref() == Some(&spki));
        key_matches = Some(at.is_some());
        leaf_at = at.unwrap_or(0);
    }
    let mut chain = vec![rest.remove(leaf_at)];
    loop {
        let last = &chain[chain.len() - 1].info;
        if last.self_signed {
            break;
        }
        let Some(i) = rest.iter().position(|c| c.info.subject == last.issuer) else {
            break;
        };
        chain.push(rest.remove(i));
    }
    chain.extend(rest);
    Ok(Bundle {
        chain,
        key,
        key_matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::build::{self, CertKind, CertSpec, Subject};

    fn ca(name: &str) -> (PrivateKey, Vec<u8>) {
        let k = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let spec = CertSpec {
            subject: Subject::cn(name),
            kind: CertKind::Ca,
            san: Vec::new(),
            not_before: 1_800_000_000,
            days: 30,
        };
        let c = build::self_signed(&spec, &k).unwrap();
        (k, c)
    }

    #[test]
    fn orders_a_chain_and_matches_the_key() {
        let (ca_key, ca_cert) = ca("Root");
        let leaf_key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let leaf = build::issue(
            &CertSpec::server("web.example", 1_800_000_000, 10),
            &leaf_key,
            &ca_cert,
            &ca_key,
        )
        .unwrap();
        // Root first, key last: read back leaf first.
        let text = format!(
            "{}{}{}",
            pem::encode("CERTIFICATE", &ca_cert),
            pem::encode("CERTIFICATE", &leaf),
            pem::encode("PRIVATE KEY", &leaf_key.to_pkcs8_der().unwrap())
        );
        let b = read_pem(&text, None).unwrap();
        assert_eq!(b.key_matches, Some(true));
        assert_eq!(b.leaf().der, leaf);
        assert_eq!(b.chain[1].der, ca_cert);
        assert_eq!(b.leaf().info.issuer, b.chain[1].info.subject);
        // Another key: no match, and the first certificate leads.
        let other = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let text = format!(
            "{}{}",
            pem::encode("CERTIFICATE", &leaf),
            pem::encode("PRIVATE KEY", &other.to_pkcs8_der().unwrap())
        );
        let b = read_pem(&text, None).unwrap();
        assert_eq!(b.key_matches, Some(false));
        assert!(!key_matches(&leaf, &other).unwrap());
        assert!(key_matches(&leaf, &leaf_key).unwrap());
        // Through PKCS#12 with a password.
        let p12 = pkcs12::write(
            "web",
            &leaf_key,
            &leaf,
            std::slice::from_ref(&ca_cert),
            "s3cret",
        )
        .unwrap();
        let b = read_pkcs12(&p12, "s3cret").unwrap();
        assert_eq!((b.key_matches, b.chain.len()), (Some(true), 2));
        assert!(matches!(
            read_pkcs12(&p12, "wrong"),
            Err(Error::BadPassword)
        ));
        assert!(read_pem("", None).is_err());
    }
}
