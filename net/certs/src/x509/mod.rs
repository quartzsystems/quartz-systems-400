// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! X.509: the facts of a certificate ([`CertInfo`]), building
//! certificates and requests ([`build`]) and the signer adapter.

pub mod build;
mod signer;

use std::fmt;

use der::Decode;
use der::asn1::ObjectIdentifier;
use serde::{Deserialize, Serialize};
use x509_cert::Certificate;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAltName};

use crate::crypto::keys::KeyAlgorithm;
use crate::crypto::{sha1, sha256};
use crate::error::{CertError, Result};

pub use signer::Signer;

const RSA_ENCRYPTION: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.113549.1.1.1");
const EC_PUBLIC_KEY: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.2.1");
const P256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.3.1.7");
const P384: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.132.0.34");
/// id-kp-serverAuth.
pub const SERVER_AUTH: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.1");
/// id-kp-clientAuth.
pub const CLIENT_AUTH: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.2");

/// What a certificate says, decoded once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct CertInfo {
    /// Subject distinguished name, RFC 4514 form.
    pub subject: String,
    /// Issuer distinguished name.
    pub issuer: String,
    /// Subject common name, if any.
    pub common_name: Option<String>,
    /// Serial number, upper-case hex.
    pub serial: String,
    /// Not valid before, Unix seconds.
    pub not_before: u64,
    /// Not valid after, Unix seconds.
    pub not_after: u64,
    /// DNS names and IP addresses of the subject alternative name.
    pub san: Vec<String>,
    /// The basic constraints say CA.
    pub is_ca: bool,
    /// Subject and issuer are the same name.
    pub self_signed: bool,
    /// The public key's algorithm, when it is one we issue.
    pub key_algorithm: Option<KeyAlgorithm>,
    /// Public key bits.
    pub key_bits: u32,
    /// Key usage bits as the certificate carries them (0 when absent).
    pub key_usage: u16,
    /// Extended key usage: server authentication.
    pub server_auth: bool,
    /// Extended key usage: client authentication.
    pub client_auth: bool,
    /// SHA-1 fingerprint, upper-case hex.
    pub sha1: String,
    /// SHA-256 fingerprint, upper-case hex.
    pub sha256: String,
}

impl CertInfo {
    /// Decode a DER certificate.
    pub fn from_der(der: &[u8]) -> Result<Self> {
        let cert = Certificate::from_der(der)?;
        Self::from_certificate(&cert, der)
    }

    /// The facts of an already decoded certificate; `der` is its encoding.
    pub fn from_certificate(cert: &Certificate, der: &[u8]) -> Result<Self> {
        let tbs = cert.tbs_certificate();
        let spki = tbs.subject_public_key_info();
        let key_der = spki.subject_public_key.raw_bytes();
        let (key_algorithm, key_bits) = if spki.algorithm.oid == RSA_ENCRYPTION {
            let bits = rsa_modulus_bits(key_der).unwrap_or(0);
            let alg = match bits {
                2048 => Some(KeyAlgorithm::Rsa2048),
                3072 => Some(KeyAlgorithm::Rsa3072),
                4096 => Some(KeyAlgorithm::Rsa4096),
                _ => None,
            };
            (alg, bits)
        } else if spki.algorithm.oid == EC_PUBLIC_KEY {
            let curve = spki
                .algorithm
                .parameters
                .as_ref()
                .and_then(|p| p.decode_as::<ObjectIdentifier>().ok());
            match curve {
                Some(c) if c == P256 => (Some(KeyAlgorithm::EcdsaP256), 256),
                Some(c) if c == P384 => (Some(KeyAlgorithm::EcdsaP384), 384),
                _ => (None, 0),
            }
        } else {
            (None, 0)
        };

        let mut san = Vec::new();
        if let Some((_, names)) = tbs.get_extension::<SubjectAltName>()? {
            for n in names.0 {
                match n {
                    GeneralName::DnsName(d) => san.push(d.to_string()),
                    GeneralName::IpAddress(ip) => {
                        let b = ip.as_bytes();
                        if b.len() == 4 {
                            san.push(std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string());
                        } else if b.len() == 16 {
                            let o: [u8; 16] = b.try_into().expect("16 bytes");
                            san.push(std::net::Ipv6Addr::from(o).to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        let is_ca = tbs
            .get_extension::<BasicConstraints>()?
            .is_some_and(|(_, bc)| bc.ca);
        let key_usage = tbs
            .get_extension::<KeyUsage>()?
            .map_or(0, |(_, ku)| ku.0.bits());
        let (mut server_auth, mut client_auth) = (false, false);
        if let Some((_, eku)) = tbs.get_extension::<ExtendedKeyUsage>()? {
            server_auth = eku.0.contains(&SERVER_AUTH);
            client_auth = eku.0.contains(&CLIENT_AUTH);
        }
        let subject = tbs.subject().to_string();
        let issuer = tbs.issuer().to_string();
        Ok(Self {
            common_name: tbs
                .subject()
                .common_name()
                .ok()
                .flatten()
                .map(|s| s.value().into_owned()),
            self_signed: subject == issuer && tbs.subject() == tbs.issuer(),
            subject,
            issuer,
            serial: hex_upper(tbs.serial_number().as_bytes()),
            not_before: tbs.validity().not_before.to_unix_duration().as_secs(),
            not_after: tbs.validity().not_after.to_unix_duration().as_secs(),
            san,
            is_ca,
            key_algorithm,
            key_bits,
            key_usage,
            server_auth,
            client_auth,
            sha1: hex_upper(&sha1(der)),
            sha256: hex_upper(&sha256(der)),
        })
    }

    /// Seconds until expiry at `now`; negative once expired.
    pub fn seconds_left(&self, now: u64) -> i64 {
        i64::try_from(self.not_after).unwrap_or(i64::MAX) - i64::try_from(now).unwrap_or(i64::MAX)
    }

    /// Whole days until expiry at `now`; negative once expired.
    pub fn days_left(&self, now: u64) -> i64 {
        self.seconds_left(now).div_euclid(86_400)
    }

    /// Whether `now` is inside the validity period.
    pub fn is_valid_at(&self, now: u64) -> bool {
        now >= self.not_before && now <= self.not_after
    }

    /// The fingerprint as `AB:CD:...`.
    pub fn sha256_colons(&self) -> String {
        colons(&self.sha256)
    }
}

impl fmt::Display for CertInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Subject . . . . . :  {}", self.subject)?;
        writeln!(f, "Issuer  . . . . . :  {}", self.issuer)?;
        writeln!(f, "Serial number . . :  {}", self.serial)?;
        writeln!(f, "Valid from  . . . :  {}", iso_date(self.not_before))?;
        writeln!(f, "Valid to  . . . . :  {}", iso_date(self.not_after))?;
        let key = self.key_algorithm.map_or_else(
            || format!("{} bits", self.key_bits),
            |a| a.name().trim_start_matches('*').to_string(),
        );
        writeln!(f, "Key . . . . . . . :  {key}")?;
        if !self.san.is_empty() {
            writeln!(f, "Alternative names :  {}", self.san.join(", "))?;
        }
        writeln!(
            f,
            "Certificate authority:  {}",
            if self.is_ca { "Yes" } else { "No" }
        )?;
        write!(f, "SHA-256 . . . . . :  {}", colons(&self.sha256))
    }
}

fn rsa_modulus_bits(key_der: &[u8]) -> Option<u32> {
    // RSAPublicKey ::= SEQUENCE { modulus INTEGER, publicExponent INTEGER }
    let seq = der::asn1::AnyRef::from_der(key_der).ok()?;
    let mut inner = der::SliceReader::new(seq.value()).ok()?;
    let modulus = der::asn1::UintRef::decode(&mut inner).ok()?;
    let bytes = modulus.as_bytes();
    let lead = bytes.first().copied().unwrap_or(0);
    let bits = bytes.len() * 8 - lead.leading_zeros() as usize;
    u32::try_from(bits).ok()
}

/// Upper-case hex.
pub fn hex_upper(b: &[u8]) -> String {
    hex::encode_upper(b)
}

fn colons(hex: &str) -> String {
    hex.as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or("??"))
        .collect::<Vec<_>>()
        .join(":")
}

/// `YYYY-MM-DD HH:MM:SS` of a Unix time.
pub fn iso_date(secs: u64) -> String {
    match der::DateTime::from_unix_duration(std::time::Duration::from_secs(secs)) {
        Ok(d) => format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            d.year(),
            d.month(),
            d.day(),
            d.hour(),
            d.minutes(),
            d.seconds()
        ),
        Err(_) => "?".into(),
    }
}

/// Decode every `CERTIFICATE` block of a PEM text to DER.
pub fn pem_certificates(pem: &str) -> Result<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for block in crate::pem::blocks(pem)? {
        if block.label == "CERTIFICATE" {
            Certificate::from_der(&block.der)
                .map_err(|e| CertError::Malformed(format!("certificate: {e}")))?;
            out.push(block.der);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_dates() {
        assert_eq!(hex_upper(&[0xab, 1]), "AB01");
        assert_eq!(colons("AB01"), "AB:01");
        assert_eq!(iso_date(0), "1970-01-01 00:00:00");
        assert_eq!(iso_date(1_700_000_000), "2023-11-14 22:13:20");
    }
}
