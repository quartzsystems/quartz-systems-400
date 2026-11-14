// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Connections: plain, `StartTLS` and LDAPS, with rustls over graviola's
//! provider (pure Rust) and an optional client certificate.

use std::sync::{Arc, Once};

use ldap3::{LdapConn, LdapConnSettings};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};

use crate::{Error, Target};

/// How the connection is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tls {
    /// Plain LDAP.
    None,
    /// `StartTLS` on the LDAP port.
    StartTls,
    /// TLS from the first byte on the LDAPS port.
    Ldaps,
}

static PROVIDER: Once = Once::new();

fn provider() -> Arc<CryptoProvider> {
    PROVIDER.call_once(|| {
        // ldap3 builds its own configurations from the process default.
        let _ = rustls_graviola::default_provider().install_default();
    });
    Arc::new(rustls_graviola::default_provider())
}

/// Takes the server's certificate without checking its chain or name (the
/// signatures over the handshake are still checked): test servers have
/// self-signed certificates.
#[derive(Debug)]
struct AnyCert(Arc<CryptoProvider>);

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// The client configuration: `--ca` trusted (or any certificate), the
/// client certificate when `with_cert` and one is given.
pub fn tls_config(t: &Target, with_cert: bool) -> Result<Arc<ClientConfig>, Error> {
    let p = provider();
    let builder = ClientConfig::builder_with_provider(p.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::Tls(e.to_string()))?;
    let builder = if let Some(ca) = &t.ca {
        let mut roots = RootCertStore::empty();
        for c in CertificateDer::pem_file_iter(ca)
            .map_err(|e| Error::Tls(format!("{}: {e}", ca.display())))?
        {
            roots
                .add(c.map_err(|e| Error::Tls(e.to_string()))?)
                .map_err(|e| Error::Tls(e.to_string()))?;
        }
        builder.with_root_certificates(roots)
    } else {
        builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCert(p)))
    };
    let config = match (&t.client, with_cert) {
        (Some((cert, key)), true) => {
            let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert)
                .map_err(|e| Error::Tls(format!("{}: {e}", cert.display())))?
                .collect::<Result<_, _>>()
                .map_err(|e| Error::Tls(e.to_string()))?;
            let key = PrivateKeyDer::from_pem_file(key)
                .map_err(|e| Error::Tls(format!("{}: {e}", key.display())))?;
            builder
                .with_client_auth_cert(chain, key)
                .map_err(|e| Error::Tls(e.to_string()))?
        }
        _ => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
}

/// A new anonymous connection.
pub fn open(t: &Target, tls: Tls) -> Result<LdapConn, Error> {
    open_with(t, tls, false)
}

/// A new connection, with the client certificate when `with_cert`.
pub fn open_with(t: &Target, tls: Tls, with_cert: bool) -> Result<LdapConn, Error> {
    let mut s = LdapConnSettings::new().set_conn_timeout(t.timeout);
    if tls != Tls::None {
        s = s.set_config(tls_config(t, with_cert)?);
    }
    let url = match tls {
        Tls::None => t.url(),
        Tls::StartTls => {
            s = s.set_starttls(true);
            t.url()
        }
        Tls::Ldaps => t.ldaps_url(),
    };
    let mut c = LdapConn::with_settings(s, &url)?;
    c.with_timeout(t.timeout);
    Ok(c)
}
