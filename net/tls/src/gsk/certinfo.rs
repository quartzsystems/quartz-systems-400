// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `gsk_attribute_get_cert_info`: a certificate as the array of
//! `gsk_cert_data_elem` entries IBM's programs walk.

use certs::x509::CertInfo;
use x509_cert::Certificate;
use x509_cert::der::{Decode, Encode};

#[allow(clippy::wildcard_imports)] // the header's names
use super::codes::*;

/// One element: the data ID and its bytes (text without a terminator).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CertDataElem {
    /// A `CERT_*` ID.
    pub id: i32,
    /// The data.
    pub data: Vec<u8>,
}

fn text(id: i32, s: impl Into<String>) -> CertDataElem {
    CertDataElem {
        id,
        data: s.into().into_bytes(),
    }
}

/// The elements of a DER certificate. Elements a certificate lacks are
/// left out, as IBM's are.
#[allow(clippy::too_many_lines)] // one push per element
pub fn elements(der: &[u8]) -> Result<Vec<CertDataElem>, Rc> {
    let cert = Certificate::from_der(der).map_err(|_| GSK_ERROR_ASN)?;
    let info = CertInfo::from_der(der).map_err(|_| GSK_ERROR_ASN)?;
    let tbs = cert.tbs_certificate();
    let mut out = vec![
        CertDataElem {
            id: CERT_BODY_DER,
            data: der.to_vec(),
        },
        text(CERT_BODY_BASE64, certs::pem::base64_encode(der)),
        text(CERT_SERIAL_NUMBER, info.serial.clone()),
    ];
    let subject = tbs.subject();
    let issuer = tbs.issuer();
    let names = |base: i32, name: &x509_cert::name::Name, out: &mut Vec<CertDataElem>| {
        if let Ok(Some(v)) = name.common_name() {
            out.push(text(base, v.value()));
        }
        if let Ok(Some(v)) = name.locality() {
            out.push(text(base + 1, v.value()));
        }
        if let Ok(Some(v)) = name.state_or_province() {
            out.push(text(base + 2, v.value()));
        }
        if let Ok(Some(v)) = name.country() {
            out.push(text(base + 3, v.to_string()));
        }
        if let Ok(Some(v)) = name.organization() {
            out.push(text(base + 4, v.value()));
        }
        if let Ok(Some(v)) = name.organization_unit() {
            out.push(text(base + 5, v.value()));
        }
        out.push(text(base + 6, name.to_string()));
        if let Ok(der) = name.to_der() {
            out.push(CertDataElem {
                id: base + 7,
                data: der,
            });
        }
        if let Ok(Some(v)) = name.email_address() {
            out.push(text(base + 9, v.to_string()));
        }
    };
    names(CERT_COMMON_NAME, subject, &mut out);
    names(CERT_ISSUER_COMMON_NAME, issuer, &mut out);
    out.push(text(CERT_VERSION, format!("{}", tbs.version() as u8 + 1)));
    out.push(text(
        CERT_SIGNATURE_ALGORITHM,
        cert.signature_algorithm().oid.to_string(),
    ));
    // IBM gives the dates in the certificate's own `YYMMDDhhmmss` form;
    // the EX forms are readable text.
    out.push(text(CERT_VALID_FROM, compact(info.not_before)));
    out.push(text(CERT_VALID_TO, compact(info.not_after)));
    out.push(text(
        CERT_PUBLIC_KEY_ALGORITHM,
        tbs.subject_public_key_info().algorithm.oid.to_string(),
    ));
    out.push(CertDataElem {
        id: CERT_PUBLIC_KEY,
        data: tbs
            .subject_public_key_info()
            .subject_public_key
            .raw_bytes()
            .to_vec(),
    });
    out.push(text(CERT_PUBLIC_KEY_SIZE, info.key_bits.to_string()));
    out.push(text(CERT_FINGERPRINT_ALGORITHM, "SHA-1"));
    out.push(CertDataElem {
        id: CERT_FINGERPRINT,
        data: certs::crypto::sha1(der).to_vec(),
    });
    if info.key_usage != 0 {
        out.push(text(CERT_KEY_USAGE, format!("{:04X}", info.key_usage)));
    }
    for n in &info.san {
        let id = if n.parse::<std::net::IpAddr>().is_ok() {
            CERT_SUBJECT_ALTERNATIVE_NAME_IPADDRESS
        } else {
            CERT_SUBJECT_ALTERNATIVE_NAME_DNSNAME
        };
        out.push(text(id, n.clone()));
    }
    out.push(text(
        CERT_BASIC_CONSTRAINTS_CA,
        if info.is_ca { "TRUE" } else { "FALSE" },
    ));
    out.push(text(
        CERT_VALID_FROM_EX,
        certs::x509::iso_date(info.not_before),
    ));
    out.push(text(
        CERT_VALID_TO_EX,
        certs::x509::iso_date(info.not_after),
    ));
    out.push(text(CERT_FINGERPRINT_SHA1, info.sha1.clone()));
    out.push(text(CERT_FINGERPRINT_SHA256, info.sha256.clone()));
    if info.server_auth {
        out.push(text(
            CERT_EXTENDEDKEYUSAGE_IDENTIFIER,
            certs::x509::SERVER_AUTH.to_string(),
        ));
    }
    if info.client_auth {
        out.push(text(
            CERT_EXTENDEDKEYUSAGE_IDENTIFIER,
            certs::x509::CLIENT_AUTH.to_string(),
        ));
    }
    Ok(out)
}

fn compact(secs: u64) -> String {
    let d = certs::x509::iso_date(secs);
    // "YYYY-MM-DD HH:MM:SS" -> "YYMMDDHHMMSS"
    let digits: String = d.chars().filter(char::is_ascii_digit).collect();
    digits.get(2..).unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use certs::crypto::keys::{KeyAlgorithm, PrivateKey};
    use certs::x509::build::{self, CertSpec, Subject};

    #[test]
    fn elements_of_a_certificate() {
        let key = PrivateKey::generate(KeyAlgorithm::EcdsaP256).unwrap();
        let spec = CertSpec {
            subject: Subject {
                common_name: "host.example".into(),
                organization: Some("Quartz".into()),
                country: Some("US".into()),
                ..Default::default()
            },
            ..CertSpec::server("host.example", 1_700_000_000, 10)
        };
        let der = build::self_signed(&spec, &key).unwrap();
        let e = elements(&der).unwrap();
        let get = |id| {
            e.iter()
                .find(|x| x.id == id)
                .map(|x| String::from_utf8_lossy(&x.data).into_owned())
        };
        assert_eq!(get(CERT_COMMON_NAME).as_deref(), Some("host.example"));
        assert_eq!(get(CERT_ORG).as_deref(), Some("Quartz"));
        assert_eq!(get(CERT_COUNTRY).as_deref(), Some("US"));
        assert_eq!(
            get(CERT_ISSUER_COMMON_NAME).as_deref(),
            Some("host.example")
        );
        assert_eq!(get(CERT_VALID_FROM).as_deref(), Some("231114221320"));
        assert_eq!(get(CERT_BASIC_CONSTRAINTS_CA).as_deref(), Some("FALSE"));
        assert_eq!(
            get(CERT_SUBJECT_ALTERNATIVE_NAME_DNSNAME).as_deref(),
            Some("host.example")
        );
        assert_eq!(get(CERT_VERSION).as_deref(), Some("3"));
        assert_eq!(get(CERT_PUBLIC_KEY_SIZE).as_deref(), Some("256"));
        assert!(get(CERT_LOCALITY).is_none());
        assert_eq!(e[0].id, CERT_BODY_DER);
        assert_eq!(e[0].data, der);
        assert!(elements(b"nope").is_err());
    }
}
