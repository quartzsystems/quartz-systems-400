// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The verifiers both ends use: a peer's certificate is accepted when
//! its fingerprint is pinned, or, while the caller has pairing open,
//! when it is any certificate at all, so that the peer may pair and do
//! nothing else ([`crate::tls::Peer::pinned`]). No certificate
//! authority, no chain, no host name, no dates: the pin is the trust.
//! What is checked always is that the peer holds the certificate's
//! key: the handshake's signature, Ed25519 only.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{
    CertificateError, DigitallySignedStruct, DistinguishedName, Error, PeerIncompatible,
    SignatureScheme,
};

use crate::Fingerprint;

/// What an end has pinned, asked at each handshake. The caller keeps
/// the pins; this crate keeps nothing.
pub trait Pins: Send + Sync + std::fmt::Debug {
    /// Whether the peer with this certificate is pinned.
    fn pinned(&self, peer: &Fingerprint) -> bool;
    /// Whether a peer that is not pinned may connect, to pair: on the
    /// managed side while its operator has a pairing password out, on
    /// a console while it is pairing with a new system.
    fn pairing_open(&self) -> bool {
        false
    }
}

/// The schemes a handshake may be signed with.
const SCHEMES: &[SignatureScheme] = &[SignatureScheme::ED25519];

/// Both directions' verifier.
#[derive(Debug)]
pub(crate) struct PinVerifier {
    pins: Arc<dyn Pins>,
    algs: WebPkiSupportedAlgorithms,
}

impl PinVerifier {
    pub(crate) fn new(pins: Arc<dyn Pins>, algs: WebPkiSupportedAlgorithms) -> Self {
        Self { pins, algs }
    }

    fn accept(&self, cert: &CertificateDer<'_>) -> Result<(), Error> {
        let fp = Fingerprint::of(cert);
        if self.pins.pinned(&fp) || self.pins.pairing_open() {
            Ok(())
        } else {
            Err(Error::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn signed(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        if !SCHEMES.contains(&dss.scheme) {
            return Err(PeerIncompatible::NoSignatureSchemesInCommon.into());
        }
        verify_tls13_signature(message, cert, dss, &self.algs)
    }
}

impl ClientCertVerifier for PinVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        self.accept(end_entity)
            .map(|()| ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("only TLS 1.3 is spoken".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.signed(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        self.accept(end_entity)
            .map(|()| ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Err(Error::General("only TLS 1.3 is spoken".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        self.signed(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        SCHEMES.to_vec()
    }
}
