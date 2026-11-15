// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What can go wrong with an identity, a connection or a pairing.

use thiserror::Error;

/// An error of this crate.
#[derive(Debug, Error)]
pub enum Error {
    /// An identity that is not one.
    #[error("identity: {0}")]
    Identity(&'static str),
    /// The system gave no random bytes.
    #[error("no random source")]
    Random,
    /// TLS refused.
    #[error("TLS: {0}")]
    Tls(#[from] rustls::Error),
    /// The connection has no peer certificate.
    #[error("the peer has presented no certificate")]
    NoPeer,
    /// A pairing exchange failed. The reason is deliberately vague on
    /// the wire; this says which step.
    #[error("pairing failed: {0}")]
    Pairing(&'static str),
}
