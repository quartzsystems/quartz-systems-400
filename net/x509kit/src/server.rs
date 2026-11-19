// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A server's own identity in one call: an ECDSA P-256 key and a
//! self-signed certificate for the DNS names and IP addresses the
//! caller gives, valid for the days it gives; and a request for a CA to
//! sign for the same key and names.

use zeroize::Zeroizing;

use crate::Result;
use crate::crypto::keys::{KeyAlgorithm, PrivateKey};
use crate::pem;
use crate::x509::CertInfo;
use crate::x509::build::{self, CertKind, CertSpec, Subject};

/// A key and the certificate made for it.
#[derive(Debug)]
pub struct ServerIdentity {
    /// The private key.
    pub key: PrivateKey,
    /// The certificate, DER.
    pub cert: Vec<u8>,
    /// What the certificate holds.
    pub info: CertInfo,
}

impl ServerIdentity {
    /// The certificate as a PEM `CERTIFICATE` block.
    pub fn cert_pem(&self) -> String {
        pem::encode("CERTIFICATE", &self.cert)
    }

    /// The key as an unencrypted PKCS#8 `PRIVATE KEY` block.
    pub fn key_pem(&self) -> Result<Zeroizing<String>> {
        Ok(Zeroizing::new(pem::encode(
            "PRIVATE KEY",
            &self.key.to_pkcs8_der()?,
        )))
    }

    /// A PKCS#10 request for this key, `common_name` and `names`.
    pub fn request(&self, common_name: &str, names: &[&str]) -> Result<Vec<u8>> {
        request(&self.key, common_name, names)
    }
}

fn spec(common_name: &str, names: &[&str], not_before: u64, days: u32) -> CertSpec {
    CertSpec {
        subject: Subject::cn(common_name),
        kind: CertKind::Server,
        san: names.iter().map(ToString::to_string).collect(),
        not_before,
        days,
    }
}

/// A new P-256 key and a self-signed server certificate: subject
/// `CN=common_name`, `names` (each a DNS name or an IP address) as its
/// subject alternative names, valid `days` from `not_before` (Unix
/// seconds).
pub fn self_signed(
    common_name: &str,
    names: &[&str],
    not_before: u64,
    days: u32,
) -> Result<ServerIdentity> {
    let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256)?;
    let cert = build::self_signed(&spec(common_name, names, not_before, days), &key)?;
    let info = CertInfo::from_der(&cert)?;
    Ok(ServerIdentity { key, cert, info })
}

/// A PKCS#10 request (DER) for `key`: subject `CN=common_name`, `names`
/// requested as subject alternative names.
pub fn request(key: &PrivateKey, common_name: &str, names: &[&str]) -> Result<Vec<u8>> {
    build::request(&spec(common_name, names, 0, 1), key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle;

    #[test]
    fn makes_a_server_identity() {
        let id = self_signed(
            "console.example",
            &["console.example", "10.0.0.5", "fe80::1"],
            1_800_000_000,
            397,
        )
        .unwrap();
        assert_eq!(id.info.common_name.as_deref(), Some("console.example"));
        assert_eq!(id.info.san, ["console.example", "10.0.0.5", "fe80::1"]);
        assert_eq!(id.info.not_before, 1_800_000_000);
        assert_eq!(id.info.not_after, 1_800_000_000 + 397 * 86_400);
        assert!(id.info.self_signed && id.info.server_auth && !id.info.is_ca);
        assert_eq!(id.info.key_algorithm, Some(KeyAlgorithm::EcdsaP256));
        assert_eq!(id.info.sha256.len(), 64);
        // Back through PEM: the key matches.
        let text = format!("{}{}", id.cert_pem(), id.key_pem().unwrap().as_str());
        let b = bundle::read_pem(&text, None).unwrap();
        assert_eq!(b.key_matches, Some(true));
        assert_eq!(b.chain[0].info, id.info);
        let csr = id.request("console.example", &["console.example"]).unwrap();
        let r = build::request_info(&csr).unwrap();
        assert_eq!(r.san, ["console.example"]);
        assert_eq!(r.spki, id.key.spki_der().unwrap());
    }
}
