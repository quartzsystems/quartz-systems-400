// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Server and client configurations from the certificate store, an
//! entry label or an application ID, and the policy.

use std::sync::Arc;

use certs::apps::{AppKind, AppRegistry, CaTrust, ClientAuth};
use certs::store::CertStore;
use certs::x509::CertInfo;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::WebPkiClientVerifier;
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme};

use crate::error::{Result, TlsError};
use crate::policy::TlsPolicy;

/// Whether a server asks for client certificates (IBM's
/// `GSK_CLIENT_AUTH_TYPE` and DCM's application setting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClientAuthMode {
    /// Never.
    #[default]
    None,
    /// Asks; a client without one is still accepted.
    Optional,
    /// Asks and requires one from a trusted CA.
    Required,
}

impl From<ClientAuth> for ClientAuthMode {
    fn from(a: ClientAuth) -> Self {
        match a {
            ClientAuth::None => ClientAuthMode::None,
            ClientAuth::Optional => ClientAuthMode::Optional,
            ClientAuth::Required => ClientAuthMode::Required,
        }
    }
}

/// A certificate chain and its key, ready for rustls.
#[derive(Clone, Debug)]
pub struct Identity {
    /// The label it came from.
    pub label: String,
    /// Leaf first.
    pub chain: Vec<CertificateDer<'static>>,
    /// PKCS#8.
    pub key: Arc<PrivateKeyDer<'static>>,
    /// The leaf's facts.
    pub info: CertInfo,
}

impl Identity {
    /// The identity of `label` in `store`, checked to be valid at `now`.
    pub fn from_store(store: &CertStore, label: &str, now: u64) -> Result<Self> {
        let e = store.get(label)?;
        if !e.has_identity() {
            return Err(TlsError::NotAnIdentity(label.to_string()));
        }
        let chain: Vec<CertificateDer<'static>> = store
            .chain(label)?
            .into_iter()
            .map(CertificateDer::from)
            .collect();
        let info = CertInfo::from_der(&chain[0])?;
        if !info.is_valid_at(now) {
            return Err(TlsError::Expired(label.to_string()));
        }
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
            e.private_key()?.to_pkcs8_der()?.to_vec(),
        ));
        Ok(Self {
            label: label.to_string(),
            chain,
            key: Arc::new(key),
            info,
        })
    }
}

/// The CAs an application trusts, from the store and its trust setting.
pub fn trusted_roots(store: &CertStore, trust: &CaTrust) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();
    match trust {
        CaTrust::All => {
            for der in store.trusted_cas() {
                roots.add(CertificateDer::from(der))?;
            }
        }
        CaTrust::Only(labels) => {
            for l in labels {
                let e = store.get(l)?;
                if let Some(der) = &e.cert
                    && e.trusted
                {
                    roots.add(CertificateDer::from(der.clone()))?;
                }
            }
        }
    }
    Ok(roots)
}

/// A server configuration.
pub fn server_config(
    identity: &Identity,
    policy: &TlsPolicy,
    client_auth: ClientAuthMode,
    roots: RootCertStore,
) -> Result<Arc<ServerConfig>> {
    let provider = policy.provider()?;
    let builder = ServerConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&policy.versions()?)?;
    let builder = match client_auth {
        ClientAuthMode::None => builder.with_no_client_auth(),
        ClientAuthMode::Optional | ClientAuthMode::Required => {
            if roots.is_empty() {
                return Err(TlsError::NoTrustedCas);
            }
            let mut v = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider);
            if client_auth == ClientAuthMode::Optional {
                v = v.allow_unauthenticated();
            }
            let verifier = v
                .build()
                .map_err(|e| TlsError::Rustls(rustls::Error::General(e.to_string())))?;
            builder.with_client_cert_verifier(verifier)
        }
    };
    let config =
        builder.with_single_cert(identity.chain.clone(), identity.key.as_ref().clone_key())?;
    Ok(Arc::new(config))
}

/// How a client checks the server's certificate (IBM's
/// `GSK_SERVER_AUTH_TYPE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ServerAuthMode {
    /// Chain to a trusted CA, name must match (`GSK_SERVER_AUTH_FULL`).
    #[default]
    Full,
    /// Accept any certificate; the application checks it
    /// (`GSK_SERVER_AUTH_PASSTHRU`).
    Passthru,
    /// Chain to a trusted CA; the name is not checked. What IBM's `GSKit`
    /// does for `GSK_SERVER_AUTH_FULL`, where programs rarely set a
    /// server name.
    ChainOnly,
}

/// A client configuration.
pub fn client_config(
    policy: &TlsPolicy,
    roots: RootCertStore,
    identity: Option<&Identity>,
    server_auth: ServerAuthMode,
) -> Result<Arc<ClientConfig>> {
    let provider = policy.provider()?;
    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&policy.versions()?)?;
    let builder = match server_auth {
        ServerAuthMode::Full => builder.with_root_certificates(roots),
        ServerAuthMode::Passthru => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAny(provider))),
        ServerAuthMode::ChainOnly => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(ChainOnly { provider, roots })),
    };
    let config = match identity {
        Some(id) => builder.with_client_auth_cert(id.chain.clone(), id.key.as_ref().clone_key())?,
        None => builder.with_no_client_auth(),
    };
    Ok(Arc::new(config))
}

/// A server name for the client side.
pub fn server_name(host: &str) -> Result<ServerName<'static>> {
    ServerName::try_from(host.to_string()).map_err(|_| TlsError::ServerName(host.to_string()))
}

/// Accepts any server certificate; signatures are still checked.
#[derive(Debug)]
struct AcceptAny(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
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
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
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

/// Verifies the chain to a trusted root and the validity dates, not
/// the name.
#[derive(Debug)]
struct ChainOnly {
    provider: Arc<CryptoProvider>,
    roots: RootCertStore,
}

impl ServerCertVerifier for ChainOnly {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let cert = webpki::EndEntityCert::try_from(end_entity).map_err(webpki_error)?;
        cert.verify_for_usage(
            self.provider.signature_verification_algorithms.all,
            &self.roots.roots,
            intermediates,
            now,
            webpki::KeyUsage::server_auth(),
            None,
            None,
        )
        .map_err(webpki_error)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
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
        rustls::crypto::verify_tls13_signature(
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

fn webpki_error(e: webpki::Error) -> rustls::Error {
    use rustls::CertificateError as C;
    match e {
        webpki::Error::UnknownIssuer => rustls::Error::InvalidCertificate(C::UnknownIssuer),
        webpki::Error::CertExpired { .. } => rustls::Error::InvalidCertificate(C::Expired),
        webpki::Error::CertNotValidYet { .. } => rustls::Error::InvalidCertificate(C::NotValidYet),
        webpki::Error::CertRevoked => rustls::Error::InvalidCertificate(C::Revoked),
        webpki::Error::InvalidSignatureForPublicKey => {
            rustls::Error::InvalidCertificate(C::BadSignature)
        }
        webpki::Error::CertNotValidForName(_) => {
            rustls::Error::InvalidCertificate(C::NotValidForName)
        }
        other => rustls::Error::General(format!("certificate chain: {other:?}")),
    }
}

/// What an application ID resolves to.
pub enum AppConfig {
    /// A server application.
    Server {
        /// The configuration.
        config: Arc<ServerConfig>,
        /// Its identity.
        identity: Identity,
        /// Its client authentication setting.
        client_auth: ClientAuthMode,
    },
    /// A client application.
    Client {
        /// The configuration.
        config: Arc<ClientConfig>,
        /// Its identity, if one is assigned.
        identity: Option<Identity>,
    },
}

/// The configuration an application ID gets from the `*SYSTEM` store
/// and the registry: its assigned certificate, client authentication
/// and CA trust. A server with no certificate is `NoCertificate`.
pub fn for_app(
    store: &CertStore,
    registry: &AppRegistry,
    app_id: &str,
    policy: &TlsPolicy,
    now: u64,
) -> Result<AppConfig> {
    let def = registry.get(app_id)?;
    let roots = trusted_roots(store, &def.ca_trust)?;
    match def.kind {
        AppKind::Server => {
            let label = def
                .label
                .as_deref()
                .or_else(|| store.default_label())
                .ok_or_else(|| TlsError::NoCertificate(app_id.to_string()))?;
            let identity = Identity::from_store(store, label, now)?;
            let client_auth = ClientAuthMode::from(def.client_auth);
            let config = server_config(&identity, policy, client_auth, roots)?;
            Ok(AppConfig::Server {
                config,
                identity,
                client_auth,
            })
        }
        AppKind::Client => {
            let identity = def
                .label
                .as_deref()
                .map(|l| Identity::from_store(store, l, now))
                .transpose()?;
            let config = client_config(policy, roots, identity.as_ref(), ServerAuthMode::Full)?;
            Ok(AppConfig::Client { config, identity })
        }
    }
}
