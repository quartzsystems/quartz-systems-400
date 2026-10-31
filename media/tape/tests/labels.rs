// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Labels byte for byte, in both codes, and round trips.

use tape::label::{Code, Date, Expiry, FileLabel, LabelSet, RecordFormat, Vol1};

/// CP037 for the characters these labels use, written out here so the
/// test does not lean on the code under test.
fn ebcdic(s: &str) -> Vec<u8> {
    s.bytes()
        .map(|b| match b {
            b' ' => 0x40,
            b'.' => 0x4b,
            b'/' => 0x61,
            b'0'..=b'9' => 0xf0 + (b - b'0'),
            b'A'..=b'I' => 0xc1 + (b - b'A'),
            b'J'..=b'R' => 0xd1 + (b - b'J'),
            b'S'..=b'Z' => 0xe2 + (b - b'S'),
            _ => panic!("no table entry for {b:#x}"),
        })
        .collect()
}

fn pad(s: &str, n: usize) -> String {
    format!("{s:<n$}")
}

fn sample() -> FileLabel {
    FileLabel {
        file_id: "QGPL".into(),
        set_id: "VOL001".into(),
        volume_seq: 1,
        file_seq: 3,
        generation: 1,
        version: 0,
        created: Date::from_ymd(2026, 10, 30),
        expires: Expiry::Perm,
        block_count: 1_234_567,
        system_code: "QS/400".into(),
        record_format: RecordFormat::Undefined,
        block_length: 512,
        record_length: 80,
        continued: false,
        job_step: "QPADEV0001".into(),
    }
}

#[test]
fn vol1_bytes() {
    let v = Vol1 {
        volume_id: "VOL001".into(),
        owner: "QSECOFR".into(),
    };
    let want = format!("VOL1VOL001 {}{}{}", pad("", 26), pad("QSECOFR", 14), pad("", 29));
    assert_eq!(want.len(), 80);
    assert_eq!(v.encode(Code::Ascii).unwrap().to_vec(), want.as_bytes());
    assert_eq!(v.encode(Code::Ebcdic).unwrap().to_vec(), ebcdic(&want));
    for code in [Code::Ascii, Code::Ebcdic] {
        let (back, found) = Vol1::decode(&v.encode(code).unwrap()).unwrap();
        assert_eq!((back, found), (v.clone(), code));
    }
}

#[test]
fn hdr_eof_eov_bytes() {
    let l = sample();
    let h1 = [
        "HDR1",
        &pad("QGPL", 17),
        "VOL001",
        "0001",
        "0003",
        "0001",
        "00",
        "026303",
        " 99365",
        "0",
        "000000",
        &pad("QS/400", 13),
        "   ",
        "0000",
    ]
    .concat();
    assert_eq!(h1.len(), 80, "{h1:?}");
    let h2 = format!(
        "HDR2U0051200080 0{}     {}{}",
        pad("QPADEV0001", 17),
        pad("", 31),
        pad("", 10)
    );
    assert_eq!(h2.len(), 80, "{h2:?}");
    let (r1, r2) = l.encode(LabelSet::Header, Code::Ascii).unwrap();
    assert_eq!(std::str::from_utf8(&r1).unwrap(), h1);
    assert_eq!(std::str::from_utf8(&r2).unwrap(), h2);
    let (e1, e2) = l.encode(LabelSet::Header, Code::Ebcdic).unwrap();
    assert_eq!(e1.to_vec(), ebcdic(&h1));
    assert_eq!(e2.to_vec(), ebcdic(&h2));

    // EOF1 carries the block count: low six digits, then the high four.
    let (f1, f2) = l.encode(LabelSet::EndOfFile, Code::Ascii).unwrap();
    let f1 = std::str::from_utf8(&f1).unwrap();
    assert_eq!(&f1[..4], "EOF1");
    assert_eq!(&f1[54..60], "234567");
    assert_eq!(&f1[76..80], "0001");
    assert_eq!(&f2[..4], b"EOF2");
    let (v1, _) = l.encode(LabelSet::EndOfVolume, Code::Ascii).unwrap();
    assert_eq!(&v1[..4], b"EOV1");
}

#[test]
fn large_blocks_and_switch() {
    let l = FileLabel {
        block_length: 262_144,
        record_format: RecordFormat::Fixed,
        continued: true,
        ..sample()
    };
    let (_, r2) = l.encode(LabelSet::Header, Code::Ascii).unwrap();
    let r2 = std::str::from_utf8(&r2).unwrap();
    assert_eq!(&r2[4..10], "F00000");
    assert_eq!(&r2[16..17], "1");
    assert_eq!(&r2[38..39], "B");
    assert_eq!(&r2[70..80], "0000262144");
}

#[test]
fn round_trips() {
    let plain = FileLabel {
        expires: Expiry::None,
        created: Date::from_ymd(1999, 12, 31),
        ..sample()
    };
    let dated = FileLabel {
        expires: Expiry::Date(Date::from_ymd(2107, 2, 3).unwrap()),
        block_length: 100_000,
        record_format: RecordFormat::Variable,
        continued: true,
        ..sample()
    };
    for l in [sample(), plain, dated] {
        for code in [Code::Ascii, Code::Ebcdic] {
            for set in [LabelSet::Header, LabelSet::EndOfFile, LabelSet::EndOfVolume] {
                let (r1, r2) = l.encode(set, code).unwrap();
                let (got_set, got) = FileLabel::decode(&r1, &r2, code).unwrap();
                assert_eq!(got_set, set);
                let want = FileLabel {
                    block_count: if set == LabelSet::Header { 0 } else { l.block_count },
                    ..l.clone()
                };
                assert_eq!(got, want);
            }
        }
    }
}

#[test]
fn dates() {
    let d = Date::from_ymd(2026, 10, 30).unwrap();
    assert_eq!(d.day, 303);
    assert_eq!(d.to_label(), "026303");
    assert_eq!(Date::from_ymd(1999, 1, 1).unwrap().to_label(), " 99001");
    assert_eq!(Date::from_ymd(2124, 12, 31).unwrap().to_label(), "124366");
    assert_eq!(Date::from_label("124366"), Date::from_ymd(2124, 12, 31));
    assert_eq!(Date::from_ymd(2026, 2, 29), None);
    assert_eq!(Date::from_ymd(2000, 2, 29).unwrap().day, 60);
    assert_eq!(Expiry::from_label(" 99365"), Expiry::Perm);
    assert_eq!(Expiry::from_label("099366"), Expiry::Perm);
    assert_eq!(Expiry::from_label(" 00000"), Expiry::None);
    assert!(Expiry::Perm.active(d));
    assert!(!Expiry::None.active(d));
    assert!(Expiry::Date(Date::from_ymd(2026, 10, 31).unwrap()).active(d));
    assert!(!Expiry::Date(d).active(d));
}

#[test]
fn bad_labels() {
    let l = sample();
    let (r1, r2) = l.encode(LabelSet::Header, Code::Ebcdic).unwrap();
    assert!(FileLabel::decode(&r2, &r1, Code::Ebcdic).is_err());
    let (e1, _) = l.encode(LabelSet::EndOfFile, Code::Ebcdic).unwrap();
    assert!(FileLabel::decode(&e1, &r2, Code::Ebcdic).is_err());
    assert!(Vol1::decode(&r1).is_err());
    let long = FileLabel {
        file_id: "X".repeat(18),
        ..sample()
    };
    assert!(long.encode(LabelSet::Header, Code::Ebcdic).is_err());
    let lower = Vol1 {
        volume_id: "vol\u{7f}".into(),
        owner: String::new(),
    };
    assert!(lower.encode(Code::Ascii).is_err());
}
