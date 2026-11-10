// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NTLMv2 responses ([MS-NLMP] 3.3.2): computing them (for tests and clients)
//! and verifying them (the server side).

use super::owf::ntowf_v2;
use crate::ct_eq;
use crate::digest::hmac_md5;

/// The shortest NTLMv2 response: the 16-byte proof and a 28-byte blob header.
/// Anything of 24 bytes is NTLMv1, which QS/400 refuses.
pub const MIN_NTLMV2_RESPONSE: usize = 16 + 28;

/// The NTLMv2 NT response for `client_blob` (the "temp" structure: version
/// 1, 1, reserved, time, client challenge, AV pairs) and its session base key.
pub fn ntlmv2_response(
    nt_hash: &[u8; 16],
    user: &str,
    domain: &str,
    server_challenge: &[u8; 8],
    client_blob: &[u8],
) -> (Vec<u8>, [u8; 16]) {
    let owf = ntowf_v2(nt_hash, user, domain);
    let proof = hmac_md5(&owf, &[server_challenge, client_blob]);
    let mut resp = proof.to_vec();
    resp.extend_from_slice(client_blob);
    (resp, hmac_md5(&owf, &[&proof]))
}

/// The LMv2 response: proof over the client challenge, then the challenge.
pub fn lmv2_response(
    nt_hash: &[u8; 16],
    user: &str,
    domain: &str,
    server_challenge: &[u8; 8],
    client_challenge: &[u8; 8],
) -> [u8; 24] {
    let owf = ntowf_v2(nt_hash, user, domain);
    let proof = hmac_md5(&owf, &[server_challenge, client_challenge]);
    let mut out = [0u8; 24];
    out[..16].copy_from_slice(&proof);
    out[16..].copy_from_slice(client_challenge);
    out
}

/// The client blob's header fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob<'a> {
    /// FILETIME the client put in (100 ns since 1601).
    pub time: u64,
    /// The client challenge.
    pub client_challenge: [u8; 8],
    /// The AV pairs, as sent (up to the end of the response).
    pub av_pairs: &'a [u8],
}

/// Splits an NTLMv2 NT response's blob, or `None` when it is not one.
pub fn parse_blob(nt_response: &[u8]) -> Option<Blob<'_>> {
    if nt_response.len() < MIN_NTLMV2_RESPONSE {
        return None;
    }
    let b = &nt_response[16..];
    if b[0] != 1 || b[1] != 1 {
        return None;
    }
    let time = u64::from_le_bytes(b[8..16].try_into().ok()?);
    let client_challenge = b[16..24].try_into().ok()?;
    Some(Blob { time, client_challenge, av_pairs: &b[28..] })
}

/// Verifies an NTLMv2 NT response against the NT hash. Returns the session
/// base key when it matches.
///
/// The proof is tried with the domain as sent, then upper-cased, then
/// empty: clients differ in which form they hash.
pub fn verify_ntlmv2(
    nt_hash: &[u8; 16],
    user: &str,
    domain: &str,
    server_challenge: &[u8; 8],
    nt_response: &[u8],
) -> Option<[u8; 16]> {
    parse_blob(nt_response)?;
    let (proof, blob) = nt_response.split_at(16);
    let upper = domain.to_uppercase();
    let mut tried: Vec<&str> = Vec::with_capacity(3);
    for d in [domain, upper.as_str(), ""] {
        if tried.contains(&d) {
            continue;
        }
        tried.push(d);
        let owf = ntowf_v2(nt_hash, user, d);
        let want = hmac_md5(&owf, &[server_challenge, blob]);
        if ct_eq(&want, proof) {
            return Some(hmac_md5(&owf, &[&want]));
        }
    }
    None
}
