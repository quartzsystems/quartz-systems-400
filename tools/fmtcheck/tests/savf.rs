// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Streams the system's writer makes pass the frozen save format's
//! second reading; a PTF that is not whole records, and fields the
//! document reserves, do not.

#![allow(clippy::cast_possible_truncation)]

use std::collections::BTreeMap;

use fmtcheck::savf::{self, crc32c};
use savfmt::object::{NO_OWNER, flags};
use savfmt::{AuthorityEntry, Compression, MediaHeader, ObjectDescriptor, Part, SaveKind, Writer};

fn header(kind: SaveKind, c: Compression) -> MediaHeader {
    MediaHeader {
        release: (1, 1, 0),
        target: (1, 1, 0),
        compression: c,
        kind,
        created: 1_793_000_000,
        stream_id: *b"QS00042 \0\0\0\0\x6a\xf1\x14\xda",
        system: "QS400".into(),
        serial: String::new(),
        command: "QSCRTPTF".into(),
        user: "QSYS".into(),
    }
}

fn stream(kind: SaveKind, c: Compression) -> Vec<u8> {
    let mut w = Writer::new(Vec::new(), &header(kind, c)).unwrap();
    w.parameters("QSCRTPTF PTF(QS00042)").unwrap();
    for i in 0..6u8 {
        let mut parts = BTreeMap::new();
        parts.insert(Part::ATTR, vec![i; 300]);
        // Over a chunk, half compressible.
        let mut space: Vec<u8> = (0..1_300_000u32).map(|n| (n % 251) as u8).collect();
        space.truncate(1_300_000 - usize::from(i) * 1000);
        parts.insert(Part::SPACE, space);
        let auths = if i == 2 {
            vec![AuthorityEntry {
                profile: savfmt::name10("BOB"),
                bits: 7,
            }]
        } else {
            Vec::new()
        };
        let mut f = flags::SIGNED;
        if !auths.is_empty() {
            f |= flags::PRIVATE_AUTH;
        }
        let d = ObjectDescriptor {
            owner_index: if i == 3 { 2 } else { NO_OWNER },
            type_code: 0x0019,
            flags: f,
            library: savfmt::name10("QSYS"),
            name: savfmt::name10(&format!("OBJ{i}")),
            size_bytes: parts.values().map(|p| p.len() as u64).sum(),
            part_count: parts.len() as u16,
            ..ObjectDescriptor::default()
        };
        w.begin_object(d, &[]).unwrap();
        if !auths.is_empty() {
            w.authorities(&auths).unwrap();
        }
        for (p, b) in &parts {
            w.part(*p, b).unwrap();
        }
        w.end_object().unwrap();
        w.signature([1; 8], [2; 64]).unwrap();
    }
    w.finish(0).unwrap().1
}

/// `s` with byte `at` set, the media header's CRC made good again.
fn patched(mut s: Vec<u8>, at: usize, v: u8) -> Vec<u8> {
    s[at] = v;
    let c = crc32c(&s[..124]);
    s[124..128].copy_from_slice(&c.to_be_bytes());
    s
}

#[test]
fn every_compression_passes() {
    for c in [Compression::None, Compression::Low, Compression::Medium] {
        let s = stream(SaveKind::Library, c);
        let sum = savf::check(&s).unwrap_or_else(|v| panic!("{c:?}: {v}"));
        assert_eq!((sum.version, sum.kind, sum.objects), (2, 0, 6));
    }
}

#[test]
fn a_ptf_must_be_whole_records() {
    let s = stream(SaveKind::Ptf, Compression::Medium);
    let v = savf::check(&s).unwrap_err();
    assert!(v.what.contains("whole 528-byte records"), "{v}");
    let padded = savfmt::pad::pad(&s, 528).unwrap();
    assert_eq!(savf::check(&padded).unwrap().kind, savf::KIND_PTF);
}

#[test]
fn reserved_and_damaged_fields_are_named() {
    let s = stream(SaveKind::Library, Compression::Low);
    let v = savf::check(&patched(s.clone(), 11, 1)).unwrap_err();
    assert_eq!(v.offset, 10, "{v}");
    assert!(v.what.contains("0x0001"), "{v}");
    assert_eq!(savf::check(&patched(s.clone(), 9, 3)).unwrap_err().offset, 8);
    assert_eq!(savf::check(&patched(s.clone(), 100, 1)).unwrap_err().offset, 86);
    let mut magic = s.clone();
    magic[0] = b'X';
    assert_eq!(savf::check(&magic).unwrap_err().what, "magic is not QS400SAV");
    // A payload byte: its record's CRC.
    let mut torn = s.clone();
    torn[128 + 24] ^= 1;
    assert_eq!(savf::check(&torn).unwrap_err().offset, 128 + 16);
    // Bytes after the trailer.
    let mut long = s;
    long.push(0);
    assert!(savf::check(&long).unwrap_err().what.contains("after the trailer"));
}
