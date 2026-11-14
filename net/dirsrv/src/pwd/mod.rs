// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Passwords (docs/DIRSRV.md §11): storage schemes ([`scheme`], with
//! `crypt(3)` in [`crypt`]) and password policy ([`policy`]).

// Short names follow the specifications' notation (RFCs, crypt).
#![allow(clippy::many_single_char_names, clippy::similar_names)]

pub mod crypt;
pub mod policy;
pub mod scheme;

pub use policy::{Partial, Policy, State};
pub use scheme::{Scheme, verify};

/// `password` as `{SSHA512}` with a fresh 16-byte salt: how the
/// configuration keeps `ibm-slapdAdminPW` (`CHGDIRSVRA`).
///
/// # Panics
///
/// If the system has no random source.
#[must_use]
pub fn hash(password: &[u8]) -> String {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("no random source");
    String::from_utf8(Scheme::Ssha512.hash(password, &salt)).unwrap_or_default()
}
