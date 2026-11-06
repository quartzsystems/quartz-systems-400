// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A new db key: RSA-2048 and a self-signed code-signing certificate.
//! The serial is taken from the key, so one key always gives the same
//! certificate for the same name and dates.

use std::str::FromStr;
use std::time::Duration;

use certs::crypto::keys::{KeyAlgorithm, PrivateKey};
use der::asn1::{BitString, GeneralizedTime};
use der::{Decode, Encode};
use sha2::{Digest, Sha256};
use spki::SubjectPublicKeyInfoRef;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder};
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages, SubjectKeyIdentifier};
use x509_cert::ext::{Extension, ToExtension};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfo;
use x509_cert::time::{Time, Validity};

use super::DbKey;
use crate::Error;

/// `id-kp-codeSigning`.
const CODE_SIGNING: der::asn1::ObjectIdentifier = der::asn1::ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.3");

/// What the certificate says.
pub struct Spec<'a> {
    /// The subject (and issuer), RFC 4514: `CN=...,O=...`.
    pub subject: &'a str,
    /// Valid from, Unix seconds.
    pub not_before: u64,
    /// Years of validity.
    pub years: u32,
}

struct Profile(Name);

impl BuilderProfile for Profile {
    fn get_issuer(&self, _subject: &Name) -> Name {
        self.0.clone()
    }

    fn get_subject(&self) -> Name {
        self.0.clone()
    }

    fn build_extensions(
        &self,
        spk: SubjectPublicKeyInfoRef<'_>,
        _issuer_spk: SubjectPublicKeyInfoRef<'_>,
        tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        let mut out = Vec::new();
        let ski = SubjectKeyIdentifier::try_from(spk)?;
        out.push((&ski).to_extension(tbs.subject(), &out)?);
        out.push((&BasicConstraints { ca: false, path_len_constraint: None }).to_extension(tbs.subject(), &out)?);
        out.push((&KeyUsage(KeyUsages::DigitalSignature.into())).to_extension(tbs.subject(), &out)?);
        out.push((&ExtendedKeyUsage(vec![CODE_SIGNING])).to_extension(tbs.subject(), &out)?);
        Ok(out)
    }
}

fn key_err(e: impl std::fmt::Display) -> Error {
    Error::Key(e.to_string())
}

/// The certificate of `key` as `spec` says.
pub fn certificate(key: &PrivateKey, spec: &Spec<'_>) -> Result<Vec<u8>, Error> {
    let spki_der = key.spki_der().map_err(key_err)?;
    let spki = SubjectPublicKeyInfo::from_der(&spki_der).map_err(key_err)?;
    let mut serial = Sha256::digest(&spki_der)[..16].to_vec();
    serial[0] = (serial[0] & 0x7F) | 0x40;
    let from = Duration::from_secs(spec.not_before);
    let to = from + Duration::from_secs(u64::from(spec.years) * 365 * 86_400);
    let validity = Validity::new(
        Time::GeneralTime(GeneralizedTime::from_unix_duration(from).map_err(key_err)?),
        Time::GeneralTime(GeneralizedTime::from_unix_duration(to).map_err(key_err)?),
    );
    let name = Name::from_str(spec.subject).map_err(key_err)?;
    let builder = CertificateBuilder::new(Profile(name), SerialNumber::new(&serial).map_err(key_err)?, validity, spki)
        .map_err(key_err)?;
    let cert = builder.build::<_, Sig>(&KeySigner(key)).map_err(key_err)?;
    cert.to_der().map_err(key_err)
}

/// A new random key and its certificate.
pub fn new_pair(spec: &Spec<'_>) -> Result<DbKey, Error> {
    let key = PrivateKey::generate(KeyAlgorithm::Rsa2048).map_err(key_err)?;
    let cert = certificate(&key, spec)?;
    Ok(DbKey { key, cert, development: false })
}

/// The key as the certificate builder wants it.
struct KeySigner<'a>(&'a PrivateKey);

/// The public half, `SubjectPublicKeyInfo` DER.
#[derive(Clone)]
struct PublicKey(Vec<u8>);

impl spki::EncodePublicKey for PublicKey {
    fn to_public_key_der(&self) -> spki::Result<spki::Document> {
        spki::Document::try_from(self.0.clone()).map_err(spki::Error::from)
    }
}

impl signature::Keypair for KeySigner<'_> {
    type VerifyingKey = PublicKey;

    fn verifying_key(&self) -> PublicKey {
        PublicKey(self.0.spki_der().unwrap_or_default())
    }
}

impl spki::DynSignatureAlgorithmIdentifier for KeySigner<'_> {
    fn signature_algorithm_identifier(&self) -> spki::Result<spki::AlgorithmIdentifierOwned> {
        Ok(self.0.signature_algorithm())
    }
}

impl signature::Signer<Sig> for KeySigner<'_> {
    fn try_sign(&self, msg: &[u8]) -> Result<Sig, signature::Error> {
        self.0.sign(msg).map(Sig).map_err(|_| signature::Error::new())
    }
}

/// A signature as the builder wants it back.
struct Sig(Vec<u8>);

impl spki::SignatureBitStringEncoding for Sig {
    fn to_bitstring(&self) -> der::Result<BitString> {
        BitString::from_bytes(&self.0)
    }
}
