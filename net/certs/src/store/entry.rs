// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One store entry: a certificate with or without its key, a CA, or a
//! pending request.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::crypto::keys::PrivateKey;
use crate::error::{CertError, Result};
use crate::x509::CertInfo;

/// What an entry holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    /// A server certificate with its private key.
    Server,
    /// A client certificate with its private key.
    Client,
    /// A certificate authority's certificate (no key here).
    Ca,
    /// A key waiting for its certificate; `csr` holds the request.
    Request,
}

impl EntryKind {
    /// The name shown in lists.
    pub fn name(self) -> &'static str {
        match self {
            EntryKind::Server => "Server",
            EntryKind::Client => "Client",
            EntryKind::Ca => "CA",
            EntryKind::Request => "Request",
        }
    }
}

/// One entry. Binary fields are hex in the JSON body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The label, unique in the store, case preserved, compared as given.
    pub label: String,
    /// The kind.
    pub kind: EntryKind,
    /// The certificate, DER (absent for a request).
    #[serde(with = "hex_opt")]
    pub cert: Option<Vec<u8>>,
    /// Intermediate certificates sent with it, DER, leaf's issuer first.
    #[serde(with = "hex_vec")]
    pub chain: Vec<Vec<u8>>,
    /// The private key, PKCS#8 DER.
    #[serde(with = "hex_opt")]
    pub key: Option<Vec<u8>>,
    /// The request, PKCS#10 DER.
    #[serde(with = "hex_opt")]
    pub csr: Option<Vec<u8>>,
    /// A CA is trusted (ignored for other kinds).
    pub trusted: bool,
    /// Free text.
    pub text: String,
    /// Unix time added.
    pub added: u64,
}

impl Entry {
    /// The certificate's facts, if it has one.
    pub fn info(&self) -> Result<Option<CertInfo>> {
        self.cert.as_deref().map(CertInfo::from_der).transpose()
    }

    /// The private key.
    pub fn private_key(&self) -> Result<PrivateKey> {
        let der = self
            .key
            .as_deref()
            .ok_or_else(|| CertError::NoPrivateKey(self.label.clone()))?;
        PrivateKey::from_pkcs8_der(&Zeroizing::new(der.to_vec()))
    }

    /// Whether the entry can serve as a TLS identity.
    pub fn has_identity(&self) -> bool {
        self.cert.is_some()
            && self.key.is_some()
            && matches!(self.kind, EntryKind::Server | EntryKind::Client)
    }
}

mod hex_opt {
    use serde::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(b) => s.serialize_some(&hex::encode(b)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let s: Option<String> = Option::deserialize(d)?;
        s.map(|s| hex::decode(s).map_err(serde::de::Error::custom))
            .transpose()
    }
}

mod hex_vec {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[Vec<u8>], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(hex::encode))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
        let v: Vec<String> = Vec::deserialize(d)?;
        v.into_iter()
            .map(|s| hex::decode(s).map_err(serde::de::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_is_hex() {
        let e = Entry {
            label: "A".into(),
            kind: EntryKind::Ca,
            cert: Some(vec![0xAB]),
            chain: vec![vec![1, 2]],
            key: None,
            csr: None,
            trusted: true,
            text: String::new(),
            added: 0,
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"cert\":\"ab\""));
        assert!(j.contains("\"chain\":[\"0102\"]"));
        assert_eq!(serde_json::from_str::<Entry>(&j).unwrap(), e);
        assert!(matches!(e.private_key(), Err(CertError::NoPrivateKey(_))));
    }
}
