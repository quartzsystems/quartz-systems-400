// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Japanese tables, mixed decode and encode, substitution.

use crate::conv::*;

/// Bytes from hex digits.
pub(super) fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// Japanese text and its bytes in 930, 939 and 300 (`None`: 300 cannot
/// hold it).
pub(super) const SAMPLES: [(&str, &str, &str, Option<&str>); 4] = [
    (
        "QS/400 は日本語を変換します。",
        "D8E261F4F0F0400E449D4562456648E74446476F49AA448D44A4448E43410F",
        "D8E261F4F0F0400E449D4562456648E74446476F49AA448D44A4448E43410F",
        None,
    ),
    (
        "ｶﾀｶﾅ ABC 123",
        "8691869640C1C2C340F1F2F3",
        "6676668C40C1C2C340F1F2F3",
        None,
    ),
    (
        "東京都　千代田区",
        "0E45574575454F4040454C459A456345540F",
        "0E45574575454F4040454C459A456345540F",
        Some("45574575454F4040454C459A45634554"),
    ),
    ("abcあx", "6263640E44810FB7", "8182830E44810FA7", None),
];

#[test]
fn samples_encode_and_decode() {
    for (text, b930, b939, b300) in SAMPLES {
        for (c, bytes) in [(930, b930), (5026, b930), (939, b939), (5035, b939)] {
            assert_eq!(from_str(c, text).unwrap(), hex(bytes), "{c} {text}");
            assert_eq!(to_string(c, &hex(bytes)).unwrap(), text, "{c} {text}");
        }
        if let Some(b) = b300 {
            assert_eq!(from_str(300, text).unwrap(), hex(b));
            assert_eq!(to_string(300, &hex(b)).unwrap(), text);
        }
    }
}

#[test]
fn every_mapped_character_round_trips() {
    for c in [930, 939, 300] {
        let codec = Codec::of(c).unwrap().unwrap();
        let (sbcs, dbcs) = match codec.form {
            Form::Mixed { sbcs, dbcs } => (Some(sbcs), dbcs),
            Form::Dbcs { dbcs } => (None, dbcs),
            _ => unreachable!(),
        };
        if let Some(t) = sbcs {
            for b in 0..=255u8 {
                if t[usize::from(b)] == u32::MAX {
                    continue;
                }
                let mut chars = Vec::new();
                assert_eq!(codec.decode(&[b], &mut chars), 1);
                let mut back = Vec::new();
                assert!(codec.encode(chars[0], &mut back), "{c} {b:#04x}");
                assert_eq!(back, vec![b], "{c} {b:#04x}");
            }
        }
        let mut all = Vec::new();
        let mut text = String::new();
        let mut n = 0;
        for (p, ch) in dbcs.pairs() {
            let [h, l] = p.to_be_bytes();
            let bytes = if codec.is_mixed() {
                vec![SO, h, l, SI]
            } else {
                vec![h, l]
            };
            let mut chars = Vec::new();
            assert_eq!(codec.decode(&bytes, &mut chars), bytes.len());
            assert_eq!(chars, vec![ch], "{c} {p:04X}");
            let mut back = Vec::new();
            assert!(codec.encode(ch, &mut back), "{c} {p:04X}");
            assert_eq!(back, bytes, "{c} {p:04X}");
            all.extend_from_slice(&[h, l]);
            text.push(ch);
            n += 1;
        }
        assert!(n > 11_000, "{c}: {n} pairs");
        // The whole table at once: one SO, one SI.
        let bytes = if codec.is_mixed() {
            [vec![SO], all, vec![SI]].concat()
        } else {
            all
        };
        assert_eq!(to_string(c, &bytes).unwrap(), text, "{c}");
        assert_eq!(from_str(c, &text).unwrap(), bytes, "{c}");
    }
}

#[test]
fn the_tables_differ_where_the_ccsids_do() {
    // 0x4260: minus sign in 930, full-width hyphen-minus in 939 and 300.
    assert_eq!(to_string(930, &hex("0E42600F")).unwrap(), "\u{2212}");
    assert_eq!(to_string(939, &hex("0E42600F")).unwrap(), "\u{FF0D}");
    assert_eq!(to_string(300, &hex("4260")).unwrap(), "\u{FF0D}");
    // Each still encodes the other one there.
    assert_eq!(from_str(930, "\u{FF0D}").unwrap(), hex("0E42600F"));
    assert_eq!(from_str(939, "\u{2212}").unwrap(), hex("0E42600F"));
    // The single-byte halves: Katakana with lowercase in 930, Latin in 939.
    assert_eq!(from_str(930, "a\\").unwrap(), hex("62B2"));
    assert_eq!(from_str(939, "a\\").unwrap(), hex("81E0"));
    assert_eq!(to_string(930, &[0x5B]).unwrap(), "¥");
    assert_eq!(to_string(939, &[0x5B]).unwrap(), "$");
}

#[test]
fn codec_kinds() {
    for c in [930, 939, 5026, 5035] {
        let k = Codec::of(c).unwrap().unwrap();
        assert!(k.is_mixed() && k.is_ebcdic(), "{c}");
    }
    let k = Codec::of(300).unwrap().unwrap();
    assert!(!k.is_mixed() && k.is_ebcdic());
    assert!(!Codec::of(37).unwrap().unwrap().is_mixed());
    let id = |a, b| Converter::new(a, b).unwrap().is_identity();
    assert!(id(930, 5026) && id(939, 5035) && id(300, 300) && id(930, 65535));
    assert!(!id(930, 939) && !id(939, 300) && !id(930, 37));
}

#[test]
fn spaces_and_controls() {
    // The double-byte space; line ends stay single-byte.
    assert_eq!(
        from_str(930, "日\u{3000}\n").unwrap(),
        hex("0E456240400F25")
    );
    assert_eq!(DBCS_SPACE, 0x4040);
    assert_eq!(
        to_string(939, &hex("0E40400F15")).unwrap(),
        "\u{3000}\u{85}"
    );
}

#[test]
fn substitution() {
    // A Latin-1 character has the single-byte substitution, others the
    // double-byte one, each in its own state.
    let mut c = Converter::new(1208, 930).unwrap();
    let mut out = Vec::new();
    c.feed("é😀日é".as_bytes(), &mut out);
    c.finish(&mut out);
    assert_eq!(out, hex("3F0EFEFE45620F3F"));
    assert_eq!(c.substituted, 3);
    assert_eq!(from_str(300, "A日").unwrap(), hex("FEFE4562"));
    // Unmapped pairs and lone bytes decode to U+FFFD, single bytes to SUB.
    assert_eq!(to_string(930, &hex("0EFEFE0F")).unwrap(), "\u{FFFD}");
    assert_eq!(
        to_string(930, &hex("0E45620E450FC1")).unwrap(),
        "日\u{FFFD}A"
    );
    assert_eq!(to_string(300, &hex("FEFE")).unwrap(), "\u{FFFD}");
    // Through a mixed target the width is kept.
    assert_eq!(
        convert(939, 930, &hex("0EFEFE0F")).unwrap(),
        hex("0EFEFE0F")
    );
    // SO and SI themselves are not characters a mixed target can hold.
    assert_eq!(convert(37, 930, &[0x0E, 0xC1]).unwrap(), hex("3FC1"));
}

#[test]
fn redundant_shifts_are_dropped_on_conversion() {
    assert_eq!(
        convert(930, 939, &hex("0E0E45620F0F0E0FC1")).unwrap(),
        hex("0E45620FC1")
    );
    // A pass-through keeps them.
    assert_eq!(
        convert(930, 5026, &hex("0E0E45620F0F")).unwrap(),
        hex("0E0E45620F0F")
    );
}
