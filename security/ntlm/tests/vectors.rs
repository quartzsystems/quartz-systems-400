// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Published vectors: RFC 1320, 1321, 2104, 6229 and [MS-NLMP] 4.2.

use ntlm::cipher::{Rc4, rc4};
use ntlm::digest::{hmac_md5, md4, md5};

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.split_whitespace().collect();
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

const SUITE: [&str; 7] = [
    "",
    "a",
    "abc",
    "message digest",
    "abcdefghijklmnopqrstuvwxyz",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
    "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
];

#[test]
fn md4_rfc1320() {
    let want = [
        "31d6cfe0d16ae931b73c59d7e0c089c0",
        "bde52cb31de33e46245e05fbdbd6fb24",
        "a448017aaf21d8525fc10ae87aa6729d",
        "d9130a8164549fe818874806e1c7014b",
        "d79e1c308aa5bbcdeea8ed63df412da9",
        "043f8582f241db351ce627e153e7f0e4",
        "e33b4ddc9c38f2199c3e7b164fcc0536",
    ];
    for (m, w) in SUITE.iter().zip(want) {
        assert_eq!(md4(m.as_bytes()).to_vec(), hex(w), "{m}");
    }
}

#[test]
fn md5_rfc1321() {
    let want = [
        "d41d8cd98f00b204e9800998ecf8427e",
        "0cc175b9c0f1b6a831c399e269772661",
        "900150983cd24fb0d6963f7d28e17f72",
        "f96b697d7cb7938d525a2f31aaf161d0",
        "c3fcd3d76192e4007dfb496cca67e13b",
        "d174ab98d277d9f5a5611c2c9f419d9f",
        "57edf4a22be3c955ac49da2e2107b67a",
    ];
    for (m, w) in SUITE.iter().zip(want) {
        assert_eq!(md5(m.as_bytes()).to_vec(), hex(w), "{m}");
    }
}

#[test]
fn hmac_md5_rfc2104() {
    assert_eq!(
        hmac_md5(&[0x0b; 16], &[b"Hi There"]).to_vec(),
        hex("9294727a3638bb1c13f48ef8158bfc9d")
    );
    assert_eq!(
        hmac_md5(b"Jefe", &[b"what do ya want ", b"for nothing?"]).to_vec(),
        hex("750c783e6ab0b503eaa86e310a5db738")
    );
    assert_eq!(
        hmac_md5(&[0xaa; 16], &[&[0xdd; 50]]).to_vec(),
        hex("56be34521d144c88dbb8c733f0e8b3f6")
    );
    // RFC 2202 case 6: a key longer than the block.
    assert_eq!(
        hmac_md5(
            &[0xaa; 80],
            &[b"Test Using Larger Than Block-Size Key - Hash Key First"]
        )
        .to_vec(),
        hex("6b1ab7fe4bd7bf8f0b62e6ce61b9d0cd")
    );
}

#[test]
fn rc4_rfc6229() {
    let ks = rc4(&hex("0102030405"), &[0u8; 32]);
    assert_eq!(ks[..16], hex("b2396305f03dc027ccc3524a0a1118a8")[..]);
    assert_eq!(ks[16..], hex("6982944f18fc82d589c403a47a0d0919")[..]);
    // The same stream in pieces.
    let mut r = Rc4::new(&hex("0102030405"));
    let mut a = [0u8; 7];
    let mut b = [0u8; 25];
    r.apply(&mut a);
    r.apply(&mut b);
    assert_eq!([a.as_slice(), b.as_slice()].concat(), ks);
    let ks = rc4(&hex("0102030405060708090a0b0c0d0e0f10"), &[0u8; 32]);
    assert_eq!(ks[..16], hex("9ac7cc9a609d1ef7b2932899cde41b97")[..]);
    assert_eq!(ks[16..], hex("5248c4959014126a6e8a84f11d1a9e1c")[..]);
}

// [MS-NLMP] 4.2.1: User "User", domain "Domain", password "Password".
const SERVER_CHALLENGE: [u8; 8] = [0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];

#[test]
fn nt_hash_nlmp_4_2_2() {
    assert_eq!(
        ntlm::nt_hash("Password").to_vec(),
        hex("a4f49c406510bdcab6824ee7c30fd852")
    );
}

fn temp_4_2_4() -> Vec<u8> {
    hex("0101000000000000 0000000000000000 aaaaaaaaaaaaaaaa 00000000
         02000c0044006f006d00610069006e00 01000c00530065007200760065007200 0000000000000000")
}

#[test]
fn ntlmv2_nlmp_4_2_4() {
    let h = ntlm::nt_hash("Password");
    assert_eq!(
        ntlm::ntowf_v2(&h, "User", "Domain").to_vec(),
        hex("0c868a403bfd7a93a3001ef22ef02e3f")
    );
    let temp = temp_4_2_4();
    let (resp, key) = ntlm::ntlmv2_response(&h, "User", "Domain", &SERVER_CHALLENGE, &temp);
    assert_eq!(resp[..16], hex("68cd0ab851e51c96aabc927bebef6a1c")[..]);
    assert_eq!(resp[16..], temp[..]);
    assert_eq!(key.to_vec(), hex("8de40ccadbc14a82f15cb0ad0de95ca3"));
    let lm = ntlm::lmv2_response(&h, "User", "Domain", &SERVER_CHALLENGE, &[0xaa; 8]);
    assert_eq!(
        lm.to_vec(),
        hex("86c35097ac9cec102554764a57cccc19aaaaaaaaaaaaaaaa")
    );

    assert_eq!(
        ntlm::verify_ntlmv2(&h, "User", "Domain", &SERVER_CHALLENGE, &resp),
        Some(key)
    );
    // The user name's case does not matter; the domain's fallbacks do not
    // admit another domain.
    assert_eq!(
        ntlm::verify_ntlmv2(&h, "user", "Domain", &SERVER_CHALLENGE, &resp),
        Some(key)
    );
    assert_eq!(
        ntlm::verify_ntlmv2(&h, "User", "Other", &SERVER_CHALLENGE, &resp),
        None
    );
    assert_eq!(
        ntlm::verify_ntlmv2(
            &ntlm::nt_hash("password"),
            "User",
            "Domain",
            &SERVER_CHALLENGE,
            &resp
        ),
        None
    );
    assert_eq!(
        ntlm::verify_ntlmv2(&h, "User", "Domain", &[0; 8], &resp),
        None
    );
    let mut bad = resp.clone();
    *bad.last_mut().unwrap() ^= 1;
    assert_eq!(
        ntlm::verify_ntlmv2(&h, "User", "Domain", &SERVER_CHALLENGE, &bad),
        None
    );
    // NTLMv1-sized responses are not NTLMv2.
    assert_eq!(
        ntlm::verify_ntlmv2(&h, "User", "Domain", &SERVER_CHALLENGE, &resp[..24]),
        None
    );

    let blob = ntlm::parse_blob(&resp).unwrap();
    assert_eq!(blob.time, 0);
    assert_eq!(blob.client_challenge, [0xaa; 8]);
}

#[test]
fn domain_fallbacks() {
    let h = ntlm::nt_hash("Password");
    let temp = temp_4_2_4();
    for hashed in ["DOMAIN", ""] {
        let (resp, key) = ntlm::ntlmv2_response(&h, "User", hashed, &SERVER_CHALLENGE, &temp);
        assert_eq!(
            ntlm::verify_ntlmv2(&h, "User", "Domain", &SERVER_CHALLENGE, &resp),
            Some(key)
        );
    }
}

#[test]
fn key_exchange_nlmp_4_2_4() {
    let base: [u8; 16] = hex("8de40ccadbc14a82f15cb0ad0de95ca3").try_into().unwrap();
    let kx = ntlm::key_exchange_key(&base);
    let enc = ntlm::encrypt_random_session_key(&kx, &[0x55; 16]);
    assert_eq!(enc.to_vec(), hex("c5dad2544fc9799094ce1ce90bc9d03e"));
    assert_eq!(
        ntlm::exported_session_key(&kx, Some(&enc)),
        Some([0x55; 16])
    );
    assert_eq!(ntlm::exported_session_key(&kx, None), Some(kx));
    assert_eq!(ntlm::exported_session_key(&kx, Some(&enc[..8])), None);
}

#[test]
fn mic_round_trip() {
    let k = [0x55; 16];
    let m = ntlm::mic(&k, b"neg", b"chal", b"auth");
    assert_eq!(m, hmac_md5(&k, &[b"negchalauth"]));
    assert!(ntlm::verify_mic(&k, b"neg", b"chal", b"auth", &m));
    assert!(!ntlm::verify_mic(&k, b"neg", b"chal", b"auth!", &m));
}
