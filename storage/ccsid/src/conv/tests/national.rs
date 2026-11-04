// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The national CCSIDs a job runs under: 37 (US), 500 (international),
//! 273 (Germany), 1141 (273 with the euro) and 1208 (UTF-8), each
//! against bytes of its table.

use crate::conv::*;

/// `(character, byte)` pairs where the code pages differ.
const CP37: &[(char, u8)] = &[('@', 0x7C), ('[', 0xBA), (']', 0xBB), ('!', 0x5A), ('¢', 0x4A), ('|', 0x4F), ('¤', 0x9F), ('Ä', 0x63)];
const CP500: &[(char, u8)] = &[('@', 0x7C), ('[', 0x4A), (']', 0x5A), ('!', 0x4F), ('|', 0xBB), ('¤', 0x9F), ('Ä', 0x63)];
const CP273: &[(char, u8)] = &[
    ('Ä', 0x4A),
    ('Ü', 0x5A),
    ('@', 0xB5),
    ('§', 0x7C),
    ('ä', 0xC0),
    ('ö', 0x6A),
    ('ü', 0xD0),
    ('ß', 0xA1),
    ('[', 0x63),
    ('¤', 0x9F),
];

fn check(ccsid: u16, pairs: &[(char, u8)]) {
    for (c, b) in pairs {
        assert_eq!(from_str(ccsid, &c.to_string()).unwrap(), [*b], "{ccsid} {c}");
        assert_eq!(to_string(ccsid, &[*b]).unwrap(), c.to_string(), "{ccsid} {b:#04x}");
    }
}

#[test]
fn each_national_ccsid_matches_its_table() {
    check(37, CP37);
    check(500, CP500);
    check(273, CP273);
    let mut euro: Vec<(char, u8)> = CP273.iter().copied().filter(|(c, _)| *c != '¤').collect();
    euro.push(('€', 0x9F));
    check(1141, &euro);
    assert_eq!(from_str(1208, "Ä€").unwrap(), "Ä€".as_bytes());
}

#[test]
fn text_moves_between_them() {
    let s = "Prüfung [1] @ 100 ¤";
    let de = from_str(273, s).unwrap();
    let us = convert(273, 37, &de).unwrap();
    assert_eq!(to_string(37, &us).unwrap(), s);
    assert_eq!(convert(37, 500, &from_str(37, "[a]").unwrap()).unwrap(), [0x4A, 0x81, 0x5A]);
    // UTF-8 and back through each.
    for c in [37, 500, 273, 1141] {
        let e = from_str(c, "Größe: 7 Stück").unwrap();
        assert_eq!(to_string(c, &convert(1208, c, &convert(c, 1208, &e).unwrap()).unwrap()).unwrap(), "Größe: 7 Stück");
    }
    // The euro has no byte in 37: the substitute.
    assert_eq!(convert(1141, 37, &[0x9F]).unwrap(), [0x3F]);
}

#[test]
fn every_byte_round_trips_through_utf8() {
    for c in [37u16, 500, 273, 1141] {
        for b in 0x40..=0xFEu8 {
            let u = convert(c, 1208, &[b]).unwrap();
            assert_eq!(convert(1208, c, &u).unwrap(), [b], "{c} {b:#04x}");
        }
    }
}
