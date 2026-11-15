// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TLS configurations of both ends: TLS 1.3 only, with graviola's
//! cryptography, each end presenting its [`Identity`] and accepting the
//! peer's certificate by its pin ([`Pins`]). Nothing is resumed: every
//! connection is a full handshake checked against the pins as they are
//! then, so a pin removed takes effect at the next connection.

mod verify;

use std::sync::Arc;

use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::server::NoServerSessionStorage;
use rustls::{ClientConfig, CommonState, Connection, ServerConfig};

pub use verify::Pins;

use crate::{Error, Fingerprint, Identity};

/// The application protocol both ends name.
pub const ALPN: &[u8] = b"qsi/1";

/// The name a console puts in its hello to the server. No end checks
/// names; this is only what TLS asks for.
pub const SERVER_NAME: &str = "qsi.invalid";

/// The exporter's label for what binds the pairing exchange to this TLS
/// connection.
const EXPORTER_LABEL: &[u8] = b"EXPORTER-qsi-pairing";

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls_graviola::default_provider())
}

fn key(identity: &Identity) -> PrivateKeyDer<'static> {
    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone()))
}

/// The managed side's configuration: it presents `identity` and asks
/// every console for its certificate.
///
/// # Errors
/// [`Error::Tls`] when rustls refuses the identity.
pub fn server_config(identity: &Identity, pins: Arc<dyn Pins>) -> Result<Arc<ServerConfig>, Error> {
    let provider = provider();
    let algs = provider.signature_verification_algorithms;
    let verifier = Arc::new(verify::PinVerifier::new(pins, algs));
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_client_cert_verifier(verifier)
        .with_single_cert(
            vec![CertificateDer::from(identity.cert.clone())],
            key(identity),
        )?;
    config.alpn_protocols = vec![ALPN.to_vec()];
    config.session_storage = Arc::new(NoServerSessionStorage {});
    config.send_tls13_tickets = 0;
    Ok(Arc::new(config))
}

/// A console's configuration: it presents `identity` and accepts the
/// server it has pinned (or, pairing, any server).
///
/// # Errors
/// [`Error::Tls`] when rustls refuses the identity.
pub fn client_config(identity: &Identity, pins: Arc<dyn Pins>) -> Result<Arc<ClientConfig>, Error> {
    let provider = provider();
    let algs = provider.signature_verification_algorithms;
    let verifier = Arc::new(verify::PinVerifier::new(pins, algs));
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(
            vec![CertificateDer::from(identity.cert.clone())],
            key(identity),
        )?;
    config.alpn_protocols = vec![ALPN.to_vec()];
    config.resumption = rustls::client::Resumption::disabled();
    config.enable_sni = false;
    Ok(Arc::new(config))
}

/// The server name a client connection is made with.
#[must_use]
pub fn server_name() -> ServerName<'static> {
    // A constant that is a valid DNS name.
    ServerName::try_from(SERVER_NAME).expect("a DNS name")
}

/// Who is at the other end of a finished handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Its certificate, DER.
    pub cert: Vec<u8>,
    /// Its certificate's fingerprint.
    pub fingerprint: Fingerprint,
    /// Whether it is pinned. A peer that is not may pair and nothing
    /// else.
    pub pinned: bool,
}

/// What ties a pairing exchange to one TLS connection: both ends'
/// fingerprints and a value exported from the connection's keys, which
/// differ on each leg of a relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding {
    /// This end's certificate.
    pub mine: Fingerprint,
    /// The peer's certificate.
    pub peer: Fingerprint,
    /// The connection's exported value.
    pub exporter: [u8; 32],
}

/// The peer of a connection whose handshake is done, judged by `pins`
/// as they are now; `None` before the handshake is done.
#[must_use]
pub fn peer(conn: &CommonState, pins: &dyn Pins) -> Option<Peer> {
    if conn.is_handshaking() {
        return None;
    }
    let cert = conn.peer_certificates()?.first()?.to_vec();
    let fingerprint = Fingerprint::of(&cert);
    Some(Peer {
        pinned: pins.pinned(&fingerprint),
        fingerprint,
        cert,
    })
}

/// The binding of a connection whose handshake is done.
///
/// # Errors
/// [`Error::Tls`] when the connection is not yet established or has no
/// peer certificate.
pub fn binding(conn: &Connection, mine: &Identity) -> Result<Binding, Error> {
    let peer = conn
        .peer_certificates()
        .and_then(|c| c.first())
        .ok_or(Error::NoPeer)?;
    let mut exporter = [0u8; 32];
    conn.export_keying_material(&mut exporter, EXPORTER_LABEL, None)?;
    Ok(Binding {
        mine: mine.fingerprint(),
        peer: Fingerprint::of(peer),
        exporter,
    })
}
