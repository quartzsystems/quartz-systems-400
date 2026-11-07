// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing a small PE32+ image: the signature checks, any change to the
//! image breaks it, signing is deterministic and replaces an old
//! signature.

use pesign::keys::{esl, generate};
use pesign::{DbKey, pe};

/// A PE32+ image with one section and a few trailing bytes.
fn image() -> Vec<u8> {
    let mut b = vec![0u8; 0x400 + 3];
    b[..2].copy_from_slice(b"MZ");
    b[0x3C..0x40].copy_from_slice(&0x40u32.to_le_bytes());
    b[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    b[coff..coff + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    b[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
    b[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes());
    let opt = coff + 20;
    b[opt..opt + 2].copy_from_slice(&0x020Bu16.to_le_bytes());
    b[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
    b[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
    let sec = opt + 240;
    b[sec..sec + 5].copy_from_slice(b".text");
    b[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes());
    b[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes());
    for (i, v) in b[0x200..].iter_mut().enumerate() {
        *v = u8::try_from(i % 251).unwrap();
    }
    b
}

#[test]
fn signs_and_checks() {
    let key = DbKey::development().unwrap();
    assert!(key.development);
    let signed = pesign::sign(&image(), &key).unwrap();
    assert_eq!(signed.len() % 8, 0);
    pesign::verify(&signed, &key.cert).unwrap();
    let l = pe::Layout::of(&signed).unwrap();
    assert_eq!(l.cert_table.0, 0x408);
    // Deterministic, and a signed image signs to the same bytes.
    assert_eq!(pesign::sign(&image(), &key).unwrap(), signed);
    assert_eq!(pesign::sign(&signed, &key).unwrap(), signed);
    // A changed section, header or trailing byte breaks it.
    for at in [0x210, 0x40 + 4 + 20 + 8, 0x401] {
        let mut bad = signed.clone();
        bad[at] ^= 1;
        assert!(pesign::verify(&bad, &key.cert).is_err(), "byte {at:#x}");
    }
    // Another key's certificate does not verify it.
    let other = generate::new_pair(&generate::Spec {
        subject: "CN=other",
        not_before: 1_767_225_600,
        years: 1,
    })
    .unwrap();
    assert!(pesign::verify(&signed, &other.cert).is_err());
    let by_other = pesign::sign(&image(), &other).unwrap();
    pesign::verify(&by_other, &other.cert).unwrap();
    assert!(pesign::verify(&by_other, &key.cert).is_err());
}

#[test]
fn refuses_what_is_not_pe32_plus() {
    let key = DbKey::development().unwrap();
    assert!(pesign::sign(b"not a PE", &key).is_err());
    let mut pe32 = image();
    pe32[0x44 + 20] = 0x0B;
    pe32[0x44 + 21] = 0x01;
    assert!(pesign::sign(&pe32, &key).is_err());
    assert!(pesign::verify(&image(), &key.cert).is_err());
}

#[test]
fn the_certificate_follows_from_the_key() {
    let key = DbKey::development().unwrap();
    let spec = generate::Spec {
        subject: "CN=QS/400 Secure Boot db DEVELOPMENT ONLY,O=Quartz Systems",
        not_before: 1_767_225_600,
        years: 30,
    };
    assert_eq!(generate::certificate(&key.key, &spec).unwrap(), key.cert);
    let l = esl::list(&key.cert);
    assert_eq!(&l[..16], &esl::CERT_X509);
    assert_eq!(&l[44..], key.cert.as_slice());
}
