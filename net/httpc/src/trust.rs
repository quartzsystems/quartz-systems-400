// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a TLS connection trusts, as the caller says.

use std::fmt;
use std::sync::Arc;

use rustls::client::Resumption;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Makes a client configuration for each connection.
pub type MakeConfig = dyn Fn() -> std::result::Result<Arc<ClientConfig>, String> + Send + Sync;

/// What an `https://` request trusts. The default trusts nothing.
#[derive(Clone)]
pub enum Trust {
    /// These certificate authorities (DER); the server's name must
    /// match its certificate.
    Roots(Vec<Vec<u8>>),
    /// The one certificate whose DER has this SHA-256. Its name and
    /// dates are not checked: the pin is the identity. Sessions are not
    /// resumed, so every connection checks it.
    Pinned([u8; 32]),
    /// A configuration of the caller's own.
    Config(Arc<ClientConfig>),
    /// A configuration the caller makes for each connection.
    Make(Arc<MakeConfig>),
}

impl Default for Trust {
    fn default() -> Self {
        Self::Roots(Vec::new())
    }
}

impl fmt::Debug for Trust {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Roots(r) => write!(f, "Roots({} certificates)", r.len()),
            Self::Pinned(p) => write!(f, "Pinned({})", hex(p)),
            Self::Config(_) => f.write_str("Config"),
            Self::Make(_) => f.write_str("Make"),
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        use std::fmt::Write as _;
        let _ = write!(s, "{x:02x}");
        s
    })
}

impl Trust {
    /// A pin from its hex text, with or without colons between bytes.
    pub fn pin(text: &str) -> Result<Self> {
        let bad = || Error::Tls(format!("{text} is not a SHA-256"));
        let digits: String = text.chars().filter(|c| *c != ':').collect();
        if digits.len() != 64 || !digits.is_ascii() {
            return Err(bad());
        }
        let mut pin = [0u8; 32];
        for (i, b) in pin.iter_mut().enumerate() {
            *b = u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).map_err(|_| bad())?;
        }
        Ok(Self::Pinned(pin))
    }

    /// The SHA-256 a pin of `cert_der` holds.
    #[must_use]
    pub fn sha256(cert_der: &[u8]) -> [u8; 32] {
        Sha256::digest(cert_der).into()
    }

    /// The configuration for one connection.
    pub(crate) fn config(&self) -> Result<Arc<ClientConfig>> {
        let provider = Arc::new(rustls_graviola::default_provider());
        let builder = || {
            ClientConfig::builder_with_provider(provider.clone())
                .with_safe_default_protocol_versions()
                .map_err(|e| Error::Tls(e.to_string()))
        };
        match self {
            Self::Roots(ders) => {
                let mut roots = RootCertStore::empty();
                for d in ders {
                    roots
                        .add(CertificateDer::from(d.clone()))
                        .map_err(|e| Error::Tls(format!("a root: {e}")))?;
                }
                Ok(Arc::new(
                    builder()?
                        .with_root_certificates(roots)
                        .with_no_client_auth(),
                ))
            }
            Self::Pinned(pin) => {
                let mut c = builder()?
                    .dangerous()
                    .with_custom_certificate_verifier(Arc::new(Pinned {
                        pin: *pin,
                        provider: provider.clone(),
                    }))
                    .with_no_client_auth();
                c.resumption = Resumption::disabled();
                Ok(Arc::new(c))
            }
            Self::Config(c) => Ok(c.clone()),
            Self::Make(f) => f().map_err(Error::Tls),
        }
    }
}

/// Accepts the one certificate pinned; the handshake's signatures are
/// still checked against it.
#[derive(Debug)]
struct Pinned {
    pin: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if Trust::sha256(end_entity) == self.pin {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "the server's certificate is not the one pinned".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_from_text() {
        let hexed = "ab".repeat(32);
        let Trust::Pinned(p) = Trust::pin(&hexed).unwrap() else {
            panic!("not a pin")
        };
        assert_eq!(p, [0xab; 32]);
        let colons = vec!["0F"; 32].join(":");
        assert!(matches!(Trust::pin(&colons).unwrap(), Trust::Pinned(p) if p == [15; 32]));
        for bad in ["", "ab", &"zz".repeat(32), &"é".repeat(32)] {
            assert!(Trust::pin(bad).is_err(), "{bad}");
        }
    }
}
