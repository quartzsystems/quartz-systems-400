// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Single-byte tables and the Unicode forms.

use crate::conv::*;

#[test]
fn every_ccsid_is_known() {
    for c in [
        37, 273, 277, 278, 280, 284, 285, 297, 500, 871, 1140, 1141, 819, 850, 1252, 367, 300, 930, 939,
        5026, 5035, 1208, 1200, 13488, 65535,
    ] {
        assert!(Codec::of(c).is_ok(), "{c}");
        assert!(supported().contains(&c), "{c}");
    }
    assert_eq!(Codec::of(1234), Err(Unsupported(1234)));
}

#[test]
fn sbcs_tables_round_trip() {
    for c in supported() {
        let Ok(Some(codec)) = Codec::of(c) else {
            continue;
        };
        let Form::Sbcs { table, .. } = codec.form else {
            continue;
        };
        for b in 0..=255u8 {
            if table[usize::from(b)] == u32::MAX {
                continue;
            }
            let mut chars = Vec::new();
            codec.decode(&[b], &mut chars);
            let mut back = Vec::new();
            assert!(codec.encode(chars[0], &mut back), "{c} {b:#04x}");
            assert_eq!(back, vec![b], "{c} {b:#04x}");
        }
    }
}

#[test]
fn national_variants_differ() {
    // '@' is 0x7C in 37 but 0xB5 in 273 (German).
    assert_eq!(from_str(37, "@").unwrap(), vec![0x7C]);
    assert_eq!(from_str(273, "@").unwrap(), vec![0xB5]);
    // 1140 is 37 with the euro sign where 37 has the currency sign.
    assert_eq!(from_str(1140, "\u{20ac}").unwrap(), vec![0x9F]);
    assert_eq!(from_str(37, "\u{a4}").unwrap(), vec![0x9F]);
    assert_eq!(from_str(1140, "\u{a4}").unwrap(), vec![0x3F]);
    assert_eq!(from_str(285, "£").unwrap(), vec![0x5B]);
    assert_eq!(from_str(819, "é").unwrap(), vec![0xE9]);
    assert_eq!(from_str(850, "é").unwrap(), vec![0x82]);
    assert_eq!(from_str(1252, "€").unwrap(), vec![0x80]);
    assert_eq!(from_str(367, "é").unwrap(), vec![0x1A]);
    assert_eq!(from_str(37, "€").unwrap(), vec![0x3F]);
}

#[test]
fn line_ends_convert_as_characters() {
    assert_eq!(
        convert(819, 37, b"a\r\nb").unwrap(),
        vec![0x81, 0x0D, 0x25, 0x82]
    );
    assert_eq!(convert(37, 1208, &[0x15]).unwrap(), "\u{85}".as_bytes());
}

#[test]
fn unicode_forms() {
    let s = "Grüße 😀";
    let u16 = from_str(1200, s).unwrap();
    assert_eq!(&u16[..2], &[0, b'G']);
    assert_eq!(to_string(13488, &u16).unwrap(), s);
    assert_eq!(convert(1200, 1208, &u16).unwrap(), s.as_bytes());
    assert_eq!(convert(65535, 37, b"raw").unwrap(), b"raw");
}

#[test]
fn streaming_carries_partial_characters() {
    let s = "aé😀b".as_bytes();
    let mut c = Converter::new(1208, 1200).unwrap();
    let mut out = Vec::new();
    for b in s {
        c.feed(std::slice::from_ref(b), &mut out);
    }
    c.finish(&mut out);
    assert_eq!(out, from_str(1200, "aé😀b").unwrap());
    let mut c = Converter::new(1208, 37).unwrap();
    let mut out = Vec::new();
    c.feed(&[0xC3], &mut out);
    assert_eq!(c.pending(), 1);
    c.finish(&mut out);
    assert_eq!(out, vec![0x3F]);
    assert_eq!(c.substituted, 1);
}
