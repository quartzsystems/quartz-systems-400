// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The table's shape and what is still unexplained.

use srcdata::{Family, Gaps, Phase, TABLE, gaps, lookup, unexplained};

#[test]
fn codes_are_unique_and_well_formed() {
    for (i, s) in TABLE.iter().enumerate() {
        assert_eq!(s.code.len(), 8, "{}", s.code);
        assert!(
            s.code.bytes().all(|b| b.is_ascii_alphanumeric()),
            "{}",
            s.code
        );
        assert!(!s.text.is_empty(), "{}", s.code);
        assert!(!s.meaning.is_empty(), "{}", s.code);
        assert!(s.cause.is_none_or(|c| !c.is_empty()), "{}", s.code);
        assert!(s.recovery.is_none_or(|r| !r.is_empty()), "{}", s.code);
        assert!(
            TABLE[..i].iter().all(|o| o.code != s.code),
            "duplicate {}",
            s.code
        );
    }
}

#[test]
fn every_family_is_known() {
    for s in TABLE {
        let f = s
            .family()
            .unwrap_or_else(|| panic!("{}: no family", s.code));
        assert!(s.code.starts_with(f.prefix()));
        assert!(!f.description().is_empty());
    }
    assert_eq!(Family::of("Z1000000"), None);
}

#[test]
fn lookup_finds_codes() {
    assert_eq!(lookup("C6001090"), Some(&srcdata::IPL_COMPLETE));
    assert_eq!(lookup("B1001030").map(|s| s.phase), Some(Phase::Fatal));
    assert_eq!(lookup("C6009999"), None);
    for s in TABLE {
        assert_eq!(lookup(s.code), Some(s));
    }
}

#[test]
fn line_and_bytes() {
    assert_eq!(srcdata::IPL_COMPLETE.line(), "SRC C6001090  IPL complete\n");
    assert_eq!(srcdata::IPL_COMPLETE.bytes(), *b"C6001090");
}

/// Every code has its cause and recovery; a new code must bring both.
#[test]
fn every_code_is_explained() {
    let want = [
        (Family::C1, 15, 0, 0),
        (Family::C6, 34, 0, 0),
        (Family::A6, 12, 0, 0),
        (Family::B1, 7, 0, 0),
        (Family::D1, 6, 0, 0),
    ];
    let got: Vec<_> = gaps()
        .iter()
        .map(|g: &Gaps| (g.family, g.codes, g.without_cause, g.without_recovery))
        .collect();
    assert_eq!(got, want);
    assert_eq!(gaps().iter().map(|g| g.codes).sum::<usize>(), TABLE.len());
    let missing: Vec<_> = unexplained().map(|s| s.code).collect();
    assert!(missing.is_empty(), "unexplained: {missing:?}");
}
