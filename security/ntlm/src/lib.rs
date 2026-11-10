// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NTLM for `NetServer` ([MS-NLMP]): the legacy digests and RC4 it is built
//! on, the NT hash, `NTLMv2` verification and the session keys. `NTLMv1` and
//! LM are not offered: only the NT hash (`NTOWFv1`) is, as `NTLMv2` starts
//! from it.
//!
//! [MS-NLMP]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nlmp

#![deny(missing_docs)]

pub mod auth;
pub mod cipher;
pub mod digest;

pub use auth::keys::{
    encrypt_random_session_key, exported_session_key, key_exchange_key, mic, verify_mic,
};
pub use auth::owf::{nt_hash, ntowf_v2, utf16le};
pub use auth::v2::{
    Blob, MIN_NTLMV2_RESPONSE, lmv2_response, ntlmv2_response, parse_blob, verify_ntlmv2,
};

/// Constant-time equality of two byte strings (length is not secret).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
