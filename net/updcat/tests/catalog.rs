// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A catalog signed here, read back under the caller's keys, and the
//! latest cumulative level of a release.

use updcat::{Catalog, CatalogGroup, Error, GroupKind, SigningKey};

fn group(name: &str, kind: GroupKind, level: u32, release: &str) -> CatalogGroup {
    CatalogGroup {
        name: name.into(),
        kind,
        level,
        release: release.into(),
        text: String::new(),
        ptfs: Vec::new(),
        related: Vec::new(),
    }
}

fn catalog() -> Catalog {
    Catalog {
        format: updcat::FORMAT,
        sequence: 7,
        generated: 1_793_318_400,
        products: Vec::new(),
        ptfs: Vec::new(),
        groups: vec![
            group("C25350110", GroupKind::Cumulative, 1, "V1R1M0"),
            group("C26100110", GroupKind::Cumulative, 2, "V1R1M0"),
            group("C26300110", GroupKind::Cumulative, 1, "V1R2M0"),
            group("QS99001", GroupKind::Group, 9, "V1R1M0"),
            group("QS99100", GroupKind::Hiper, 4, "V1R1M0"),
        ],
    }
}

#[test]
fn verified_against_the_callers_keys() {
    let key = SigningKey::from_bytes(&[11; 32]);
    let text = catalog().sign(&key);
    let trusted = [key.verifying_key().to_bytes()];
    let c = Catalog::parse(&text, &trusted).unwrap();
    assert_eq!(c.sequence, 7);
    assert_eq!(c.encode(), catalog().encode());
    // Another key, a changed byte, no signature: refused.
    let other = [SigningKey::from_bytes(&[12; 32]).verifying_key().to_bytes()];
    assert!(matches!(
        Catalog::parse(&text, &other),
        Err(Error::Verify(_))
    ));
    assert!(Catalog::parse(&text, &[]).is_err());
    let tampered = text.replace("sequence = 7", "sequence = 8");
    assert!(matches!(
        Catalog::parse(&tampered, &trusted),
        Err(Error::Verify(_))
    ));
    assert!(Catalog::parse(&catalog().encode(), &trusted).is_err());
    // Signed, but not a catalog of this format.
    let wrong = updcat::signed::sign("format = 2\nsequence = 1\ngenerated = 0\n", &key);
    assert!(matches!(
        Catalog::parse(&wrong, &trusted),
        Err(Error::Parse(_))
    ));
}

#[test]
fn the_latest_cumulative_level_of_a_release() {
    let c = catalog();
    let l = c.latest("V1R1M0").unwrap();
    assert_eq!((l.name.as_str(), l.level), ("C26100110", 2));
    assert_eq!(l.date.unwrap().to_string(), "2026-04-10");
    let l = c.latest("V1R2M0").unwrap();
    assert_eq!(l.date.unwrap().to_string(), "2026-10-27");
    assert_eq!(c.latest("V1R0M0"), None);
}
