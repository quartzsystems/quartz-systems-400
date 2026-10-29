// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Making certificates and requests: self-signed, CA, CA-issued, and
//! certificate signing requests, with the extensions each kind carries.

use std::str::FromStr;
use std::time::Duration;

use der::asn1::{GeneralizedTime, Ia5String, OctetString};
use der::oid::AssociatedOid;
use der::{Decode, Encode};
use serde::{Deserialize, Serialize};
use spki::SubjectPublicKeyInfoRef;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder, RequestBuilder};
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::name::{GeneralName, GeneralNames};
use x509_cert::ext::pkix::{
    AuthorityKeyIdentifier, BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages,
    SubjectAltName, SubjectKeyIdentifier,
};
use x509_cert::ext::{Extension, ToExtension};
use x509_cert::name::Name;
use x509_cert::request::{CertReq, ExtensionReq};
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfo;
use x509_cert::time::{Time, Validity};

use super::signer::{DerSignature, Signer};
use super::{CLIENT_AUTH, SERVER_AUTH};
use crate::crypto::keys::PrivateKey;
use crate::crypto::random;
use crate::error::{CertError, Result};

/// The subject of a certificate, as the create command prompts it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subject {
    /// Common name (required).
    pub common_name: String,
    /// Organization.
    pub organization: Option<String>,
    /// Organizational unit.
    pub organizational_unit: Option<String>,
    /// Locality.
    pub locality: Option<String>,
    /// State or province.
    pub state: Option<String>,
    /// Two-letter country.
    pub country: Option<String>,
}

impl Subject {
    /// A subject with only a common name.
    pub fn cn(common_name: &str) -> Self {
        Self {
            common_name: common_name.to_string(),
            ..Default::default()
        }
    }

    /// The RFC 4514 string, most specific first.
    pub fn to_rfc4514(&self) -> String {
        let mut parts = vec![format!("CN={}", escape(&self.common_name))];
        for (k, v) in [
            ("OU", &self.organizational_unit),
            ("O", &self.organization),
            ("L", &self.locality),
            ("ST", &self.state),
            ("C", &self.country),
        ] {
            if let Some(v) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                parts.push(format!("{k}={}", escape(v)));
            }
        }
        parts.join(",")
    }

    fn name(&self) -> Result<Name> {
        if self.common_name.trim().is_empty() {
            return Err(CertError::Invalid("a common name is required".into()));
        }
        Name::from_str(&self.to_rfc4514())
            .map_err(|e| CertError::Invalid(format!("subject name: {e}")))
    }
}

fn escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for (i, c) in v.chars().enumerate() {
        if matches!(c, ',' | '+' | '"' | '\\' | '<' | '>' | ';' | '=')
            || (i == 0 && matches!(c, ' ' | '#'))
        {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// What kind of certificate to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CertKind {
    /// A server certificate (server and client authentication usages).
    Server,
    /// A client certificate.
    Client,
    /// A certificate authority.
    Ca,
}

/// The request for a new certificate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertSpec {
    /// The subject.
    pub subject: Subject,
    /// What it is for.
    pub kind: CertKind,
    /// Subject alternative names: DNS names or IP addresses.
    pub san: Vec<String>,
    /// Valid from, Unix seconds.
    pub not_before: u64,
    /// Days of validity from `not_before`.
    pub days: u32,
}

impl CertSpec {
    /// A server certificate for `host`, valid `days` from `now`.
    pub fn server(host: &str, now: u64, days: u32) -> Self {
        Self {
            subject: Subject::cn(host),
            kind: CertKind::Server,
            san: vec![host.to_string()],
            not_before: now,
            days,
        }
    }

    fn validity(&self) -> Result<Validity> {
        if self.days == 0 {
            return Err(CertError::Invalid(
                "validity must be at least one day".into(),
            ));
        }
        let from = Duration::from_secs(self.not_before);
        let to = from + Duration::from_secs(u64::from(self.days) * 86_400);
        Ok(Validity::new(
            Time::GeneralTime(GeneralizedTime::from_unix_duration(from)?),
            Time::GeneralTime(GeneralizedTime::from_unix_duration(to)?),
        ))
    }

    fn general_names(&self) -> Result<GeneralNames> {
        let mut names = Vec::new();
        for s in &self.san {
            let s = s.trim();
            if s.is_empty() {
                continue;
            }
            if let Ok(ip) = s.parse::<std::net::IpAddr>() {
                names.push(GeneralName::from(ip));
            } else {
                names.push(GeneralName::DnsName(Ia5String::new(s).map_err(|_| {
                    CertError::Invalid(format!("alternative name {s}"))
                })?));
            }
        }
        Ok(names)
    }
}

/// The extensions of each kind.
struct Profile {
    issuer: Name,
    subject: Name,
    kind: CertKind,
    san: GeneralNames,
}

impl BuilderProfile for Profile {
    fn get_issuer(&self, _subject: &Name) -> Name {
        self.issuer.clone()
    }

    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn build_extensions(
        &self,
        spk: SubjectPublicKeyInfoRef<'_>,
        issuer_spk: SubjectPublicKeyInfoRef<'_>,
        tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        let mut out = Vec::new();
        let ski = SubjectKeyIdentifier::try_from(spk)?;
        out.push((&ski).to_extension(tbs.subject(), &out)?);
        let aki = AuthorityKeyIdentifier::try_from(issuer_spk)?;
        out.push((&aki).to_extension(tbs.subject(), &out)?);
        match self.kind {
            CertKind::Ca => {
                out.push(
                    (&BasicConstraints {
                        ca: true,
                        path_len_constraint: None,
                    })
                        .to_extension(tbs.subject(), &out)?,
                );
                out.push(
                    (&KeyUsage(
                        KeyUsages::KeyCertSign | KeyUsages::CRLSign | KeyUsages::DigitalSignature,
                    ))
                        .to_extension(tbs.subject(), &out)?,
                );
            }
            CertKind::Server | CertKind::Client => {
                out.push(
                    (&BasicConstraints {
                        ca: false,
                        path_len_constraint: None,
                    })
                        .to_extension(tbs.subject(), &out)?,
                );
                out.push(
                    (&KeyUsage(KeyUsages::DigitalSignature | KeyUsages::KeyEncipherment))
                        .to_extension(tbs.subject(), &out)?,
                );
                let eku = if self.kind == CertKind::Server {
                    vec![SERVER_AUTH, CLIENT_AUTH]
                } else {
                    vec![CLIENT_AUTH]
                };
                out.push((&ExtendedKeyUsage(eku)).to_extension(tbs.subject(), &out)?);
                if !self.san.is_empty() {
                    out.push(
                        (&SubjectAltName(self.san.clone())).to_extension(tbs.subject(), &out)?,
                    );
                }
            }
        }
        Ok(out)
    }
}

/// A fresh random serial: 16 bytes, high bit clear.
fn serial() -> Result<SerialNumber> {
    let mut b = random(16)?;
    b[0] &= 0x7F;
    b[0] |= 0x40;
    Ok(SerialNumber::new(&b)?)
}

fn build(
    spec: &CertSpec,
    issuer: Name,
    subject: Name,
    subject_key: &PrivateKey,
    signing_key: &PrivateKey,
) -> Result<Vec<u8>> {
    let spki = SubjectPublicKeyInfo::from_der(&subject_key.spki_der()?)?;
    let profile = Profile {
        issuer,
        subject,
        kind: spec.kind,
        san: spec.general_names()?,
    };
    let builder = CertificateBuilder::new(profile, serial()?, spec.validity()?, spki)
        .map_err(|e| CertError::Crypto(format!("certificate builder: {e}")))?;
    let cert = builder
        .build::<_, DerSignature>(&Signer(signing_key))
        .map_err(|e| CertError::Crypto(format!("signing: {e}")))?;
    Ok(cert.to_der()?)
}

/// A self-signed certificate (a CA's own, or a stand-alone server's).
pub fn self_signed(spec: &CertSpec, key: &PrivateKey) -> Result<Vec<u8>> {
    let name = spec.subject.name()?;
    build(spec, name.clone(), name, key, key)
}

/// A certificate for `key` issued by the CA whose certificate is
/// `ca_der` and whose key is `ca_key`.
pub fn issue(
    spec: &CertSpec,
    key: &PrivateKey,
    ca_der: &[u8],
    ca_key: &PrivateKey,
) -> Result<Vec<u8>> {
    let ca = x509_cert::Certificate::from_der(ca_der)?;
    let issuer = ca.tbs_certificate().subject().clone();
    build(spec, issuer, spec.subject.name()?, key, ca_key)
}

/// A PKCS#10 request for `key`; the SAN travels as a requested extension.
pub fn request(spec: &CertSpec, key: &PrivateKey) -> Result<Vec<u8>> {
    let mut b = RequestBuilder::new(spec.subject.name()?)
        .map_err(|e| CertError::Crypto(format!("request builder: {e}")))?;
    let san = spec.general_names()?;
    if !san.is_empty() {
        b.add_extension(&SubjectAltName(san))
            .map_err(|e| CertError::Crypto(format!("request extension: {e}")))?;
    }
    let req = b
        .build::<_, DerSignature>(&Signer(key))
        .map_err(|e| CertError::Crypto(format!("signing: {e}")))?;
    Ok(req.to_der()?)
}

/// What a request asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestInfo {
    /// The subject.
    pub subject: String,
    /// Requested alternative names.
    pub san: Vec<String>,
    /// The public key, `SubjectPublicKeyInfo` DER.
    pub spki: Vec<u8>,
}

/// Decode a PKCS#10 request.
pub fn request_info(csr_der: &[u8]) -> Result<RequestInfo> {
    let req = CertReq::from_der(csr_der)?;
    let info = &req.info;
    let mut san = Vec::new();
    for attr in info.attributes.iter() {
        if attr.oid != ExtensionReq::OID {
            continue;
        }
        for v in attr.values.iter() {
            let exts: ExtensionReq = v.decode_as()?;
            for e in exts.0 {
                if e.extn_id == SubjectAltName::OID {
                    let names = SubjectAltName::from_der(e.extn_value.as_bytes())?;
                    for n in names.0 {
                        if let GeneralName::DnsName(d) = n {
                            san.push(d.to_string());
                        } else if let GeneralName::IpAddress(ip) = n {
                            san.push(ip_text(&ip));
                        }
                    }
                }
            }
        }
    }
    Ok(RequestInfo {
        subject: info.subject.to_string(),
        san,
        spki: info.public_key.to_der()?,
    })
}

fn ip_text(ip: &OctetString) -> String {
    let b = ip.as_bytes();
    match b.len() {
        4 => std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string(),
        16 => {
            let o: [u8; 16] = b.try_into().expect("16 bytes");
            std::net::Ipv6Addr::from(o).to_string()
        }
        _ => super::hex_upper(b),
    }
}

/// Sign a request with the CA: the subject and public key come from the
/// request, the usages and validity from `kind`, `not_before` and `days`.
pub fn sign_request(
    csr_der: &[u8],
    kind: CertKind,
    not_before: u64,
    days: u32,
    ca_der: &[u8],
    ca_key: &PrivateKey,
) -> Result<Vec<u8>> {
    let req = CertReq::from_der(csr_der)?;
    let info = request_info(csr_der)?;
    let ca = x509_cert::Certificate::from_der(ca_der)?;
    let spec = CertSpec {
        subject: Subject::default(),
        kind,
        san: info.san,
        not_before,
        days,
    };
    let profile = Profile {
        issuer: ca.tbs_certificate().subject().clone(),
        subject: req.info.subject.clone(),
        kind,
        san: spec.general_names()?,
    };
    let builder = CertificateBuilder::new(
        profile,
        serial()?,
        spec.validity()?,
        req.info.public_key.clone(),
    )
    .map_err(|e| CertError::Crypto(format!("certificate builder: {e}")))?;
    let cert = builder
        .build::<_, DerSignature>(&Signer(ca_key))
        .map_err(|e| CertError::Crypto(format!("signing: {e}")))?;
    Ok(cert.to_der()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keys::KeyAlgorithm;
    use crate::x509::CertInfo;

    #[test]
    fn subject_string_escapes() {
        let s = Subject {
            common_name: "a,b".into(),
            organization: Some("Quartz Systems".into()),
            country: Some("US".into()),
            ..Default::default()
        };
        assert_eq!(s.to_rfc4514(), "CN=a\\,b,O=Quartz Systems,C=US");
    }

    #[test]
    fn self_signed_server_has_the_server_extensions() {
        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let spec = CertSpec {
            san: vec!["qs400.example".into(), "10.0.0.1".into()],
            ..CertSpec::server("qs400.example", 1_700_000_000, 365)
        };
        let der = self_signed(&spec, &key).unwrap();
        let info = CertInfo::from_der(&der).unwrap();
        assert_eq!(info.common_name.as_deref(), Some("qs400.example"));
        assert!(info.self_signed);
        assert!(!info.is_ca);
        assert!(info.server_auth && info.client_auth);
        assert_eq!(info.san, vec!["qs400.example", "10.0.0.1"]);
        assert_eq!(info.key_algorithm, Some(KeyAlgorithm::EcdsaP256));
        assert_eq!(info.not_before, 1_700_000_000);
        assert_eq!(info.days_left(1_700_000_000), 365);
    }

    #[test]
    fn ca_issues_and_signs_requests() {
        let ca_key = PrivateKey::generate(KeyAlgorithm::EcdsaP384).unwrap();
        let ca_spec = CertSpec {
            subject: Subject::cn("QS400 Local CA"),
            kind: CertKind::Ca,
            san: vec![],
            not_before: 1_700_000_000,
            days: 3650,
        };
        let ca = self_signed(&ca_spec, &ca_key).unwrap();
        assert!(CertInfo::from_der(&ca).unwrap().is_ca);

        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let spec = CertSpec::server("host.example", 1_700_000_000, 30);
        let leaf = issue(&spec, &key, &ca, &ca_key).unwrap();
        let info = CertInfo::from_der(&leaf).unwrap();
        assert_eq!(info.issuer, "CN=QS400 Local CA");
        assert!(!info.self_signed);

        let csr = request(&spec, &key).unwrap();
        let ri = request_info(&csr).unwrap();
        assert_eq!(ri.subject, "CN=host.example");
        assert_eq!(ri.san, vec!["host.example"]);
        assert_eq!(ri.spki, key.spki_der().unwrap());
        let signed = sign_request(&csr, CertKind::Server, 1_700_000_000, 10, &ca, &ca_key).unwrap();
        let si = CertInfo::from_der(&signed).unwrap();
        assert_eq!(si.san, vec!["host.example"]);
        assert_eq!(si.days_left(1_700_000_000), 10);
    }
}
