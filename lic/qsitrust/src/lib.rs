// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The trust rules both ends of QSI share (`docs/QSI.md`): mutual TLS
//! 1.3 with graviola's cryptography, in which each end presents its
//! [`Identity`] (an Ed25519 key and a self-signed certificate) and
//! accepts exactly the peer certificates it has pinned by their
//! SHA-256 ([`Fingerprint`]), with no certificate authority and no host
//! name check ([`tls`]); and the pairing exchange by which a console
//! and a managed system come to pin each other, a one-time password
//! authenticated by SPAKE2 over a transcript that binds both
//! certificates and the connection ([`pair`]).
//!
//! The crate keeps no state: keys and pins come from the caller, and
//! counting attempts, expiring the password and storing pins are the
//! caller's ([`Pins`], [`pair::Guard`]).

#![forbid(unsafe_code)]

mod cert;
mod error;
pub mod pair;
pub mod tls;

pub use cert::{Fingerprint, Identity, from_pem, subject, to_pem};
pub use error::Error;
pub use tls::{Binding, Peer, Pins};
