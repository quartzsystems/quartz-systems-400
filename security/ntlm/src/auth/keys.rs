// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Session keys after an NTLMv2 sign-on ([MS-NLMP] 3.4.5, 3.1.5.1.2).

use crate::cipher::rc4;
use crate::ct_eq;
use crate::digest::hmac_md5;

/// The key exchange key. For NTLMv2 it is the session base key itself.
pub fn key_exchange_key(session_base_key: &[u8; 16]) -> [u8; 16] {
    *session_base_key
}

/// The exported session key: with `NTLMSSP_NEGOTIATE_KEY_EXCH`, the client's
/// random key RC4-decrypted under the key exchange key; else the key
/// exchange key.
pub fn exported_session_key(
    kxkey: &[u8; 16],
    encrypted_random_key: Option<&[u8]>,
) -> Option<[u8; 16]> {
    match encrypted_random_key {
        None => Some(*kxkey),
        Some(k) if k.len() == 16 => rc4(kxkey, k).try_into().ok(),
        Some(_) => None,
    }
}

/// The client's encrypted random session key (the inverse, for tests).
pub fn encrypt_random_session_key(kxkey: &[u8; 16], random: &[u8; 16]) -> [u8; 16] {
    rc4(kxkey, random).try_into().expect("16 bytes")
}

/// The MIC: HMAC-MD5 under the exported session key of the three messages,
/// the AUTHENTICATE one with its MIC field zeroed.
pub fn mic(
    exported_session_key: &[u8; 16],
    negotiate: &[u8],
    challenge: &[u8],
    authenticate: &[u8],
) -> [u8; 16] {
    hmac_md5(exported_session_key, &[negotiate, challenge, authenticate])
}

/// Checks `got` against the MIC of the messages.
pub fn verify_mic(
    exported_session_key: &[u8; 16],
    negotiate: &[u8],
    challenge: &[u8],
    authenticate_zeroed: &[u8],
    got: &[u8],
) -> bool {
    ct_eq(
        &mic(
            exported_session_key,
            negotiate,
            challenge,
            authenticate_zeroed,
        ),
        got,
    )
}
