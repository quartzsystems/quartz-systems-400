// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product manifests build into signed product loads.

use std::io::Cursor;
use std::path::Path;

use ed25519_dalek::SigningKey;
use licpgm::plan::Media;
use lpstore::SavfMedia;
use savfmt::{Reader, SaveKind, Scan};

const NOW: u64 = 1_767_225_600;

fn manifest(id: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../products/{id}.toml"));
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn the_manifests_match_the_catalog() {
    let opts = licpgm::pkg::Options {
        allow_quartz: true,
        catalog: false,
    };
    for p in licpgm::catalog::shipped().shipped() {
        let id = p.definition.product.as_str();
        let m = licpgm::pkg::parse(&manifest(id), opts).unwrap();
        assert_eq!(m.definition, p.definition, "{id}");
    }
}

fn check(id: &str, stream: &[u8], key: &SigningKey) -> usize {
    let public = key.verifying_key().to_bytes();
    let mut r = Reader::new(Cursor::new(stream)).unwrap();
    assert_eq!(r.header().kind, SaveKind::LicPgm);
    let mut n = 0;
    while let Some(s) = r.next_object().unwrap() {
        let Scan::Object(o) = s else {
            panic!("{id}: a damaged object in the stream")
        };
        let sig = o.signature.as_ref().unwrap_or_else(|| {
            panic!(
                "{id}: {} {} type {:#x} unsigned",
                savfmt::text(&o.descriptor.library),
                savfmt::text(&o.descriptor.name),
                o.descriptor.type_code
            )
        });
        assert_ne!(o.descriptor.flags & savfmt::object::flags::SIGNED, 0);
        assert_eq!(sig.key_id, saverst::sign::key_id(&public));
        assert!(
            saverst::sign::verify(&public, &o.digest, &sig.signature),
            "{id}: bad signature"
        );
        n += 1;
    }
    n
}

#[test]
fn products_build_signed() {
    let key = SigningKey::from_bytes(&[42; 32]);
    for id in ["QS00WDS", "QS00TC1", "QS00ST1"] {
        let b = lpbuild::build(&manifest(id), &key, NOW).unwrap();
        assert_eq!(b.product, id);
        let n = check(id, &b.stream, &key);
        assert!(n >= 2, "{id}: {n} objects");
        let media = SavfMedia::read(b.stream).unwrap();
        assert!(!media.loads().is_empty());
    }
}

#[test]
fn the_operating_system_load_holds_its_objects() {
    let key = SigningKey::from_bytes(&[42; 32]);
    let b = lpbuild::build(&manifest("QS00SS1"), &key, NOW).unwrap();
    let n = check("QS00SS1", &b.stream, &key);
    assert!(n > 300, "{n} objects");
    let media = SavfMedia::read(b.stream.clone()).unwrap();
    let base = media
        .loads()
        .into_iter()
        .find(|m| m.load.key.option.is_base() && m.load.key.load_type == licpgm::LoadType::Code)
        .unwrap();
    assert!(
        base.load
            .objects
            .iter()
            .any(|o| o.name == "QSTRUP" && o.typ == "*PGM")
    );
    assert!(
        base.load
            .objects
            .iter()
            .any(|o| o.name == "CRTLIB" && o.typ == "*CMD")
    );
    // Built twice, the same objects carry the same digests.
    let again = lpbuild::build(&manifest("QS00SS1"), &key, NOW).unwrap();
    let digests = |s: &[u8]| {
        let mut r = Reader::new(Cursor::new(s)).unwrap();
        let mut v = Vec::new();
        while let Some(Scan::Object(o)) = r.next_object().unwrap() {
            v.push(o.digest);
        }
        v
    };
    assert_eq!(digests(&b.stream), digests(&again.stream));
}
