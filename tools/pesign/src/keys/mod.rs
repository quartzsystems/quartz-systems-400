// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QS/400 Secure Boot db key: an RSA-2048 key and its X.509
//! certificate. The release key comes from `QS400_SB_KEY` and
//! `QS400_SB_CERT` (NEEDS-CODY: the real one is made with `pesign
//! keygen` and held off the tree); without them the development pair
//! checked in under `keys/` signs, and says so.

pub mod esl;
pub mod generate;
pub mod ovmf;

use std::path::Path;

use certs::crypto::keys::PrivateKey;
use der::{Decode, Encode};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use crate::Error;

/// The private key of the release's db key, PKCS#8 PEM or DER.
pub const KEY_ENV: &str = "QS400_SB_KEY";
/// Its certificate, PEM or DER.
pub const CERT_ENV: &str = "QS400_SB_CERT";

/// The development key: never for a release that leaves the building.
const DEV_KEY: &str = include_str!("../../keys/qs400-db-DEV-ONLY.key");
/// The development key's certificate (DER).
const DEV_CERT: &[u8] = include_bytes!("../../keys/qs400-db-DEV-ONLY.cer");

/// A db key and its certificate.
pub struct DbKey {
    /// The private key.
    pub key: PrivateKey,
    /// The certificate, DER.
    pub cert: Vec<u8>,
    /// Whether this is the development pair.
    pub development: bool,
}

/// DER from a file's bytes: a PEM block with `label`, or DER as it is.
fn der_of(bytes: &[u8], label: &str) -> Result<Vec<u8>, Error> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(bytes.to_vec());
    };
    if !text.contains("-----BEGIN") {
        return Ok(bytes.to_vec());
    }
    let blocks = certs::pem::blocks(text).map_err(|e| Error::Key(e.to_string()))?;
    blocks
        .into_iter()
        .find(|b| b.label == label)
        .map(|b| b.der)
        .ok_or_else(|| Error::Key(format!("no {label} block")))
}

impl DbKey {
    /// From a PKCS#8 key and a certificate, PEM or DER each.
    pub fn from_bytes(key: &[u8], cert: &[u8], development: bool) -> Result<Self, Error> {
        let key = PrivateKey::from_pkcs8_der(&der_of(key, "PRIVATE KEY")?)
            .map_err(|e| Error::Key(e.to_string()))?;
        let cert = der_of(cert, "CERTIFICATE")?;
        let c = Certificate::from_der(&cert).map_err(|e| Error::Key(e.to_string()))?;
        let spki = c
            .tbs_certificate()
            .subject_public_key_info()
            .to_der()
            .map_err(|e| Error::Key(e.to_string()))?;
        if spki != key.spki_der().map_err(|e| Error::Key(e.to_string()))? {
            return Err(Error::Key("the certificate is not the key's".into()));
        }
        if !key.algorithm().is_rsa() {
            return Err(Error::Key("a Secure Boot db key is RSA".into()));
        }
        Ok(Self {
            key,
            cert,
            development,
        })
    }

    /// The development pair.
    pub fn development() -> Result<Self, Error> {
        Self::from_bytes(DEV_KEY.as_bytes(), DEV_CERT, true)
    }

    /// From files.
    pub fn load(key: &Path, cert: &Path) -> Result<Self, Error> {
        let read =
            |p: &Path| std::fs::read(p).map_err(|e| Error::Key(format!("{}: {e}", p.display())));
        Self::from_bytes(&read(key)?, &read(cert)?, false)
    }

    /// `QS400_SB_KEY` and `QS400_SB_CERT` when both are set, else the
    /// development pair. One without the other is an error.
    pub fn from_env() -> Result<Self, Error> {
        match (std::env::var_os(KEY_ENV), std::env::var_os(CERT_ENV)) {
            (Some(k), Some(c)) => Self::load(Path::new(&k), Path::new(&c)),
            (None, None) => Self::development(),
            _ => Err(Error::Key(format!(
                "set both {KEY_ENV} and {CERT_ENV}, or neither"
            ))),
        }
    }

    /// The signer's issuer name and serial number, DER.
    pub fn issuer_and_serial(&self) -> Result<(Vec<u8>, Vec<u8>), Error> {
        let c = Certificate::from_der(&self.cert).map_err(|e| Error::Key(e.to_string()))?;
        let tbs = c.tbs_certificate();
        let issuer = tbs
            .issuer()
            .to_der()
            .map_err(|e| Error::Key(e.to_string()))?;
        let serial = tbs
            .serial_number()
            .to_der()
            .map_err(|e| Error::Key(e.to_string()))?;
        Ok((issuer, serial))
    }

    /// The certificate's SHA-256 fingerprint, colon-separated hex.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        fingerprint(&self.cert)
    }
}

/// SHA-256 of a DER certificate as `AB:CD:...`.
#[must_use]
pub fn fingerprint(cert: &[u8]) -> String {
    Sha256::digest(cert)
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}
