// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Import and export: PEM and PKCS#12 in and out of a store.

use zeroize::Zeroizing;

use crate::crypto::keys::PrivateKey;
use crate::error::{CertError, Result};
use crate::pem;
use crate::pkcs12;
use crate::store::{CertStore, EntryKind};
use crate::x509::CertInfo;

/// What an import found and did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Imported {
    /// The identity entry made, if a key was present.
    pub identity: Option<String>,
    /// CA entries made.
    pub cas: Vec<String>,
}

impl CertStore {
    /// Import PEM text: a certificate with its private key becomes a
    /// server or client entry `label` (with any intermediates as its
    /// chain); CA certificates without a key become trusted CA entries
    /// labelled from their common names. `key_password` opens an
    /// `ENCRYPTED PRIVATE KEY` block.
    pub fn import_pem(
        &mut self,
        label: &str,
        text: &str,
        key_password: Option<&str>,
        now: u64,
    ) -> Result<Imported> {
        let mut certs = Vec::new();
        let mut key = None;
        for b in pem::blocks(text)? {
            match b.label.as_str() {
                "CERTIFICATE" => certs.push(b.der),
                "PRIVATE KEY" => key = Some(PrivateKey::from_pkcs8_der(&Zeroizing::new(b.der))?),
                "RSA PRIVATE KEY" => {
                    key = Some(PrivateKey::from_pkcs1_der(&Zeroizing::new(b.der))?);
                }
                "EC PRIVATE KEY" => key = Some(PrivateKey::from_sec1_der(&Zeroizing::new(b.der))?),
                "ENCRYPTED PRIVATE KEY" => {
                    let pw = key_password.ok_or_else(|| {
                        CertError::Invalid(
                            "the private key is encrypted; a password is needed".into(),
                        )
                    })?;
                    let epki = decode_epki(&b.der)?;
                    let pkcs8 = Zeroizing::new(pkcs12::pbe::decrypt(&epki.0, pw, &epki.1)?);
                    key = Some(PrivateKey::from_pkcs8_der(&pkcs8)?);
                }
                _ => {}
            }
        }
        if certs.is_empty() {
            return Err(CertError::Invalid("no certificate in the PEM text".into()));
        }
        self.import_parts(label, certs, key, now)
    }

    /// Import a PKCS#12 file.
    pub fn import_pkcs12(
        &mut self,
        label: &str,
        data: &[u8],
        password: &str,
        now: u64,
    ) -> Result<Imported> {
        let contents = pkcs12::read(data, password)?;
        match contents.identity()? {
            Some(id) => {
                let mut certs = vec![id.cert];
                certs.extend(id.others);
                self.import_parts(label, certs, Some(id.key), now)
            }
            None => self.import_parts(
                label,
                contents.certs.into_iter().map(|c| c.der).collect(),
                None,
                now,
            ),
        }
    }

    fn import_parts(
        &mut self,
        label: &str,
        certs: Vec<Vec<u8>>,
        key: Option<PrivateKey>,
        now: u64,
    ) -> Result<Imported> {
        let mut done = Imported::default();
        let mut certs = certs;
        if let Some(key) = key {
            let spki = key.spki_der()?;
            let at = certs
                .iter()
                .position(|c| spki_of(c).ok().as_deref() == Some(&spki))
                .ok_or_else(|| {
                    CertError::Invalid("no certificate matches the private key".into())
                })?;
            let leaf = certs.remove(at);
            let info = CertInfo::from_der(&leaf)?;
            // Intermediates: the chain up from the leaf; roots become CAs.
            let mut chain = Vec::new();
            let mut last = info.clone();
            while !last.self_signed {
                let Some(i) = certs
                    .iter()
                    .position(|c| CertInfo::from_der(c).is_ok_and(|x| x.subject == last.issuer))
                else {
                    break;
                };
                let c = certs.remove(i);
                let ci = CertInfo::from_der(&c)?;
                if ci.self_signed {
                    certs.push(c);
                    break;
                }
                chain.push(c);
                last = ci;
            }
            let kind = if info.server_auth || !info.client_auth {
                EntryKind::Server
            } else {
                EntryKind::Client
            };
            self.add_identity(label, kind, leaf, chain, &key, now)?;
            done.identity = Some(label.to_string());
        }
        for c in certs {
            let info = CertInfo::from_der(&c)?;
            if !info.is_ca || self.find_by_sha256(&info.sha256)?.is_some() {
                continue;
            }
            let base = info
                .common_name
                .clone()
                .unwrap_or_else(|| info.subject.clone());
            let mut l =
                if done.identity.is_none() && done.cas.is_empty() && self.get(label).is_err() {
                    label.to_string()
                } else {
                    base.clone()
                };
            let mut n = 1;
            while self.get(&l).is_ok() {
                n += 1;
                l = format!("{base} ({n})");
            }
            self.add_ca(&l, c, true, now)?;
            done.cas.push(l);
        }
        if done.identity.is_none() && done.cas.is_empty() {
            return Err(CertError::Invalid(
                "nothing to import: no key, and no CA certificate that is not already present"
                    .into(),
            ));
        }
        Ok(done)
    }

    /// The entry's certificate and chain as PEM. With `with_key`, the
    /// private key follows as an unencrypted `PRIVATE KEY` block.
    pub fn export_pem(&self, label: &str, with_key: bool) -> Result<Zeroizing<String>> {
        let e = self.get(label)?;
        let mut out = String::new();
        for c in self.chain(label)? {
            out.push_str(&pem::encode("CERTIFICATE", &c));
        }
        if with_key {
            let key = e.private_key()?;
            out.push_str(&pem::encode("PRIVATE KEY", &key.to_pkcs8_der()?));
        }
        Ok(Zeroizing::new(out))
    }

    /// The entry's key, certificate and chain as a PKCS#12 file.
    pub fn export_pkcs12(&self, label: &str, password: &str) -> Result<Vec<u8>> {
        let e = self.get(label)?;
        let key = e.private_key()?;
        let chain = self.chain(label)?;
        pkcs12::write(label, &key, &chain[0], &chain[1..], password)
    }
}

fn spki_of(der: &[u8]) -> Result<Vec<u8>> {
    use der::{Decode, Encode};
    let c = x509_cert::Certificate::from_der(der)?;
    Ok(c.tbs_certificate().subject_public_key_info().to_der()?)
}

fn decode_epki(der: &[u8]) -> Result<(spki::AlgorithmIdentifierOwned, Vec<u8>)> {
    use der::Decode;
    let e = pkcs12::asn1::EncryptedPrivateKeyInfo::from_der(der)?;
    Ok((e.encryption_algorithm, e.encrypted_data.as_bytes().to_vec()))
}
