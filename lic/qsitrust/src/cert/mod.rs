// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An end's identity: an Ed25519 key and a self-signed X.509
//! certificate that names the end and carries its serial number. The
//! certificate is trusted by nothing but its pin: the SHA-256 of its
//! DER ([`Fingerprint`]). [`Identity::make`] writes one field by field,
//! in the same layout a QS/VS host makes for itself, so a caller may
//! bring its own or have one made.

pub(crate) mod der;
mod pem;

use graviola::hashing::{Hash, Sha256};
use graviola::signing::eddsa::Ed25519SigningKey;

use crate::Error;
use der::{
    BIT_STRING, BOOLEAN, GENERALIZED_TIME, INTEGER, OCTET_STRING, OID, PRINTABLE_STRING, SET,
    UTC_TIME, UTF8_STRING, bits, sequence,
};

pub use pem::{from_pem, to_pem};

/// `id-Ed25519`, 1.3.101.112.
const ED25519: [u8; 3] = [0x2b, 0x65, 0x70];
/// `commonName`, 2.5.4.3.
const COMMON_NAME: [u8; 3] = [0x55, 0x04, 0x03];
/// `serialNumber`, 2.5.4.5.
const SERIAL_NUMBER: [u8; 3] = [0x55, 0x04, 0x05];
/// `basicConstraints`, 2.5.29.19.
const BASIC_CONSTRAINTS: [u8; 3] = [0x55, 0x1d, 0x13];
/// `keyUsage`, 2.5.29.15.
const KEY_USAGE: [u8; 3] = [0x55, 0x1d, 0x0f];

/// The SHA-256 of a certificate's DER: what an end pins.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fingerprint(pub [u8; 32]);

impl Fingerprint {
    /// The fingerprint of the certificate `der`.
    #[must_use]
    pub fn of(der: &[u8]) -> Self {
        let mut out = [0u8; 32];
        out.copy_from_slice(Sha256::hash(der).as_ref());
        Self(out)
    }

    /// As lowercase hexadecimal, 64 characters.
    #[must_use]
    pub fn hex(&self) -> String {
        use std::fmt::Write;
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    /// From 64 hexadecimal characters (either case, `:` between bytes
    /// allowed).
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits: Vec<u8> = text.bytes().filter(|b| *b != b':').collect();
        if digits.len() != 64 {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, pair) in digits.chunks(2).enumerate() {
            let s = std::str::from_utf8(pair).ok()?;
            out[i] = u8::from_str_radix(s, 16).ok()?;
        }
        Some(Self(out))
    }
}

impl std::fmt::Debug for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fingerprint({})", self.hex())
    }
}

impl std::fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.hex())
    }
}

/// An end's key and certificate, both DER.
#[derive(Clone)]
pub struct Identity {
    /// The certificate.
    pub cert: Vec<u8>,
    /// The private key, PKCS #8.
    pub key: Vec<u8>,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

impl Identity {
    /// An identity from its DER parts, checked: the key reads as an
    /// Ed25519 key and the certificate carries its public key.
    ///
    /// # Errors
    /// [`Error::Identity`] when either does not.
    pub fn new(cert: Vec<u8>, key: Vec<u8>) -> Result<Self, Error> {
        let sk = Ed25519SigningKey::from_pkcs8_der(&key)
            .map_err(|_| Error::Identity("the key is not an Ed25519 PKCS #8 key"))?;
        let public = sk.public_key().as_bytes();
        if !cert.windows(32).any(|w| w == public) {
            return Err(Error::Identity("the certificate is not the key's"));
        }
        Ok(Self { cert, key })
    }

    /// An identity from PEM text: the key (`PRIVATE KEY`) and the
    /// certificate (`CERTIFICATE`).
    ///
    /// # Errors
    /// [`Error::Identity`] when either is not one.
    pub fn from_pem(cert: &str, key: &str) -> Result<Self, Error> {
        let cert =
            from_pem("CERTIFICATE", cert).ok_or(Error::Identity("the certificate is not PEM"))?;
        let key = from_pem("PRIVATE KEY", key).ok_or(Error::Identity("the key is not PEM"))?;
        Self::new(cert, key)
    }

    /// A new identity for the end named `name` with the serial number
    /// `serial`, valid from `now` (Unix seconds) with no end. The key
    /// comes from the system's random source.
    ///
    /// # Errors
    /// [`Error::Random`] when there is no random source.
    pub fn make(name: &str, serial: &str, now: i64) -> Result<Self, Error> {
        let mut seed = [0u8; 32];
        let mut number = [0u8; 16];
        graviola::random::fill(&mut seed).map_err(|_| Error::Random)?;
        graviola::random::fill(&mut number).map_err(|_| Error::Random)?;
        Ok(Self::from_seed(&seed, name, serial, now, number))
    }

    /// The identity of the key with the seed `seed`, its certificate
    /// numbered `number`; the same arguments make the same bytes.
    #[must_use]
    pub fn from_seed(
        seed: &[u8; 32],
        name: &str,
        serial: &str,
        now: i64,
        number: [u8; 16],
    ) -> Self {
        // A 32-byte seed always makes a key.
        let sk = Ed25519SigningKey::from_bytes(seed).expect("a 32-byte seed");
        Self {
            cert: certificate(&sk, name, serial, now, number),
            key: pkcs8(seed),
        }
    }

    /// The certificate's fingerprint.
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.cert)
    }
}

fn algorithm() -> Vec<u8> {
    sequence(&[der::der(OID, &ED25519)])
}

/// One attribute of a name, in a set of its own.
fn attribute(oid: &[u8], tag: u8, value: &str) -> Vec<u8> {
    der::der(
        SET,
        &sequence(&[der::der(OID, oid), der::der(tag, value.as_bytes())]),
    )
}

/// A serial number as a `PrintableString` may hold it.
fn printable(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || " '()+,-./:=?".contains(*c))
        .collect()
}

/// The end's name: its common name, and its serial number when it has
/// one.
fn name(host: &str, serial: &str) -> Vec<u8> {
    let mut parts = vec![attribute(&COMMON_NAME, UTF8_STRING, host)];
    let serial = printable(serial);
    if !serial.is_empty() {
        parts.push(attribute(&SERIAL_NUMBER, PRINTABLE_STRING, &serial));
    }
    sequence(&parts)
}

/// Days since 1970-01-01 as a civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let month = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A time as a certificate has it: `UTCTime` before 2050,
/// `GeneralizedTime` from then on.
fn time(secs: i64) -> Vec<u8> {
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    let rest = format!(
        "{month:02}{day:02}{:02}{:02}{:02}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    );
    if year < 2050 {
        der::der(UTC_TIME, format!("{:02}{rest}", year % 100).as_bytes())
    } else {
        der::der(GENERALIZED_TIME, format!("{year:04}{rest}").as_bytes())
    }
}

/// An extension that is critical.
fn extension(oid: &[u8], value: &[u8]) -> Vec<u8> {
    sequence(&[
        der::der(OID, oid),
        der::der(BOOLEAN, &[0xff]),
        der::der(OCTET_STRING, value),
    ])
}

/// The self-signed certificate of `key`.
fn certificate(
    key: &Ed25519SigningKey,
    host: &str,
    serial: &str,
    now: i64,
    number: [u8; 16],
) -> Vec<u8> {
    let name = name(host, serial);
    // A positive integer: the first bit is clear.
    let mut number = number;
    number[0] = number[0] & 0x7f | 0x40;
    // No well-defined end, as RFC 5280 writes it.
    let never = der::der(GENERALIZED_TIME, b"99991231235959Z");
    let extensions = sequence(&[
        // Not a certificate authority.
        extension(&BASIC_CONSTRAINTS, &sequence(&[])),
        // digitalSignature.
        extension(&KEY_USAGE, &der::der(BIT_STRING, &[0x07, 0x80])),
    ]);
    let tbs = sequence(&[
        // Version 3.
        der::der(0xa0, &der::der(INTEGER, &[2])),
        der::der(INTEGER, &number),
        algorithm(),
        name.clone(),
        sequence(&[time(now), never]),
        name,
        sequence(&[algorithm(), bits(&key.public_key().as_bytes())]),
        der::der(0xa3, &extensions),
    ]);
    let signature = key.sign(&tbs);
    sequence(&[tbs, algorithm(), bits(&signature)])
}

/// The private key as PKCS #8 (RFC 8410's form, without the public key).
fn pkcs8(seed: &[u8; 32]) -> Vec<u8> {
    sequence(&[
        der::der(INTEGER, &[0]),
        algorithm(),
        der::der(OCTET_STRING, &der::der(OCTET_STRING, seed)),
    ])
}

/// The common name and serial number in a certificate's subject, as far
/// as they read: what a peer calls itself, for a caller to show.
#[must_use]
pub fn subject(cert: &[u8]) -> (Option<String>, Option<String>) {
    let read = || -> Option<(Option<String>, Option<String>)> {
        let (_, body, _) = der::open(cert)?;
        let (_, tbs, _) = der::open(body)?;
        // version, serial, algorithm, issuer, validity, subject.
        let mut rest = tbs;
        let mut subject = None;
        for i in 0..6 {
            let (_, content, next) = der::open(rest)?;
            if i == 5 {
                subject = Some(content);
            }
            rest = next;
        }
        let mut cn = None;
        let mut sn = None;
        let mut names = subject?;
        while !names.is_empty() {
            let (_, set, next) = der::open(names)?;
            let (_, attr, _) = der::open(set)?;
            let (_, oid, value) = der::open(attr)?;
            let (_, text, _) = der::open(value)?;
            let text = String::from_utf8_lossy(text).into_owned();
            if oid == COMMON_NAME {
                cn = Some(text);
            } else if oid == SERIAL_NUMBER {
                sn = Some(text);
            }
            names = next;
        }
        Some((cn, sn))
    };
    read().unwrap_or((None, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use graviola::signing::eddsa::Ed25519VerifyingKey;

    #[test]
    fn the_certificate_is_signed_by_its_own_key() {
        let id = Identity::from_seed(
            &[7; 32],
            "vs-01",
            "M525400123456",
            1_794_488_700,
            [0xff; 16],
        );
        let (_, body, rest) = der::open(&id.cert).unwrap();
        assert!(rest.is_empty());
        let (_, _, after_tbs) = der::open(body).unwrap();
        let tbs = &body[..body.len() - after_tbs.len()];
        let (_, _, after_alg) = der::open(after_tbs).unwrap();
        let (tag, signature, _) = der::open(after_alg).unwrap();
        assert_eq!(tag, BIT_STRING);
        let sk = Ed25519SigningKey::from_pkcs8_der(&id.key).unwrap();
        let vk = Ed25519VerifyingKey::from_bytes(&sk.public_key().as_bytes()).unwrap();
        vk.verify(&signature[1..], tbs).unwrap();
        let text = String::from_utf8_lossy(&id.cert);
        assert!(text.contains("261112130500Z") && text.contains("99991231235959Z"));
        assert_eq!(
            subject(&id.cert),
            (Some("vs-01".into()), Some("M525400123456".into()))
        );
        assert!(Identity::new(id.cert.clone(), id.key.clone()).is_ok());
        let other = Identity::from_seed(&[8; 32], "x", "", 0, [1; 16]);
        assert!(Identity::new(id.cert.clone(), other.key).is_err());
    }

    #[test]
    fn dates_and_fingerprints() {
        // 2050-01-01 00:00:00 UTC.
        assert_eq!(
            time(2_524_608_000),
            der::der(GENERALIZED_TIME, b"20500101000000Z")
        );
        assert_eq!(time(0), der::der(UTC_TIME, b"700101000000Z"));
        let id = Identity::from_seed(&[1; 32], "a", "", 0, [0; 16]);
        let fp = id.fingerprint();
        assert_eq!(Fingerprint::from_hex(&fp.hex()), Some(fp));
        assert_eq!(fp.hex().len(), 64);
        assert!(Fingerprint::from_hex("00").is_none());
        assert_eq!(subject(b"junk"), (None, None));
    }
}
