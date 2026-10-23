// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TLS for the telnet server's port 992: a self-signed certificate on
//! a P-256 key, made on first use and kept as PEM in the data areas
//! `QUSRSYS/QTVTLSCRT` and `QUSRSYS/QTVTLSKEY` (the key's area is
//! `AUT(*EXCLUDE)`). The protocol versions follow `QSSLPCL`.
//! `MIGRATE(phase10)`: certificate management.

use std::sync::Arc;

use mi::io::DataAreaValue;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use stg::{Client, Link};

/// The certificate's data area.
pub const CERT_AREA: &str = "QTVTLSCRT";
/// The private key's data area.
pub const KEY_AREA: &str = "QTVTLSKEY";
const LIB: &str = "QUSRSYS";
const AREA_LEN: u32 = 2000;

fn random_bytes(n: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut b = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(b)
}

/// A new self-signed certificate for `host`: the certificate and the
/// key, both PEM. A P-256 key signs it (ECDSA with SHA-256), the
/// profile is a root's, valid for ten years.
pub fn make_certificate(host: &str) -> Result<(String, String), String> {
    use std::str::FromStr;
    use x509_cert::builder::{Builder, CertificateBuilder};
    use x509_cert::der::{EncodePem, pem::LineEnding};
    use x509_cert::name::Name;
    use x509_cert::serial_number::SerialNumber;
    use x509_cert::spki::SubjectPublicKeyInfo;
    use x509_cert::time::Validity;
    let bytes = random_bytes(32)?;
    let key = p256::ecdsa::SigningKey::from_slice(&bytes).map_err(|e| e.to_string())?;
    let key_pem = {
        use p256::pkcs8::EncodePrivateKey;
        key.to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| e.to_string())?
            .to_string()
    };
    let serial = random_bytes(8)?;
    let serial = SerialNumber::new(&serial).map_err(|e| e.to_string())?;
    let name = host
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect::<String>();
    let subject = Name::from_str(&format!(
        "CN={},O=Quartz Systems,C=US",
        if name.is_empty() { "QS400" } else { &name }
    ))
    .map_err(|e| e.to_string())?;
    let profile =
        x509_cert::builder::profile::cabf::Root::new(false, subject).map_err(|e| e.to_string())?;
    let validity = Validity::from_now(std::time::Duration::from_hours(10 * 365 * 24))
        .map_err(|e| e.to_string())?;
    let spki = SubjectPublicKeyInfo::from_key(key.verifying_key()).map_err(|e| e.to_string())?;
    let builder =
        CertificateBuilder::new(profile, serial, validity, spki).map_err(|e| e.to_string())?;
    let cert = builder
        .build::<_, p256::ecdsa::DerSignature>(&key)
        .map_err(|e| e.to_string())?;
    let cert_pem = cert.to_pem(LineEnding::LF).map_err(|e| e.to_string())?;
    Ok((cert_pem, key_pem))
}

fn read_area(link: &mut Link, name: &str) -> Option<String> {
    let v = link.da_read(name.into(), LIB.into()).ok()?.ok()?;
    let text = ccsid::from_ebcdic(&v.value);
    let t = text.trim_end().to_string();
    if t.contains("-----BEGIN") {
        Some(t)
    } else {
        None
    }
}

fn write_area(link: &mut Link, name: &str, text: &str, exclude: bool) -> Result<(), String> {
    let aut = if exclude { "*EXCLUDE" } else { "*USE" };
    let made = link
        .exec(format!(
            "CRTDTAARA DTAARA({LIB}/{name}) TYPE(*CHAR) LEN({AREA_LEN}) AUT({aut}) TEXT('TELNET server TLS material')"
        ))
        .map_err(|e| e.to_string())?;
    let first = made.text.lines().next().unwrap_or("");
    if first.starts_with("CPF") && !first.starts_with("CPF1023") {
        return Err(format!("{LIB}/{name}: {first}"));
    }
    let mut v = DataAreaValue::chars(AREA_LEN);
    let bytes = ccsid::to_ebcdic(text).map_err(|e| e.to_string())?;
    if bytes.len() > AREA_LEN as usize {
        return Err(format!("{name}: the PEM is longer than the data area"));
    }
    v.value.clone_from(&bytes);
    v.value.resize(AREA_LEN as usize, 0x40);
    link.da_write(name.into(), LIB.into(), v)
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{LIB}/{name}: {}", e.msgid))
}

/// The protocol versions `QSSLPCL` allows; TLS 1.3 and 1.2 for `*OPSYS`.
fn versions(link: &mut Link) -> Vec<&'static rustls::SupportedProtocolVersion> {
    let text = link
        .sysval("QSSLPCL".into())
        .ok()
        .flatten()
        .map(|v| ccsid::from_ebcdic(&v.data).to_ascii_uppercase())
        .unwrap_or_default();
    let mut out = Vec::new();
    if text.contains("*TLSV1.3") || text.contains("*OPSYS") || text.trim().is_empty() {
        out.push(&rustls::version::TLS13);
    }
    if text.contains("*TLSV1.2") || text.contains("*OPSYS") || text.trim().is_empty() {
        out.push(&rustls::version::TLS12);
    }
    if out.is_empty() {
        out.push(&rustls::version::TLS13);
    }
    out
}

/// The server configuration: the stored certificate and key, made on
/// first use.
pub fn server_config(link: &mut Link, host: &str) -> Result<Arc<rustls::ServerConfig>, String> {
    let stored = read_area(link, CERT_AREA).zip(read_area(link, KEY_AREA));
    let (cert_pem, key_pem) = if let Some(pair) = stored {
        pair
    } else {
        let (c, k) = make_certificate(host)?;
        write_area(link, CERT_AREA, &c, false)?;
        write_area(link, KEY_AREA, &k, true)?;
        (c, k)
    };
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("certificate: {e}"))?;
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).map_err(|e| format!("key: {e}"))?;
    let provider = Arc::new(rustls_graviola::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&versions(link))
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_certificate_rustls_accepts() {
        let (c, k) = make_certificate("QS400").unwrap();
        assert!(c.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(k.starts_with("-----BEGIN PRIVATE KEY-----"));
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(c.as_bytes())
            .collect::<Result<_, _>>()
            .unwrap();
        let key = PrivateKeyDer::from_pem_slice(k.as_bytes()).unwrap();
        let provider = Arc::new(rustls_graviola::default_provider());
        rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
    }
}
