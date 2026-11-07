// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every single-byte table against the primary oracle's 256-byte sweep
//! (`tools/conformance/oracle/ccsid/sweep.txt`): each byte to UTF-16 and
//! back as the oracle converted it.

use std::collections::BTreeMap;

use ccsid::conv::{Codec, convert};

/// The sweep: per CCSID, the UTF-16 unit and the round trip of each byte.
fn sweep() -> BTreeMap<u16, Vec<(u16, u8)>> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/conformance/oracle/ccsid/sweep.txt"
    ))
    .expect("sweep.txt");
    let mut out = BTreeMap::new();
    let (mut cur, mut per, mut u, mut b) = (0u16, false, String::new(), String::new());
    let flush =
        |cur: u16, u: &mut String, b: &mut String, out: &mut BTreeMap<u16, Vec<(u16, u8)>>| {
            if u.len() == 1024 && b.len() == 512 {
                let v = (0..256)
                    .map(|i| {
                        let unit = u16::from_str_radix(&u[i * 4..i * 4 + 4], 16).expect("unit");
                        let back = u8::from_str_radix(&b[i * 2..i * 2 + 2], 16).expect("byte");
                        (unit, back)
                    })
                    .collect();
                out.insert(cur, v);
            }
            u.clear();
            b.clear();
        };
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = line.strip_prefix("ccsid ") {
            flush(cur, &mut u, &mut b, &mut out);
            let mut f = rest.split_whitespace();
            cur = f.next().and_then(|n| n.parse().ok()).expect("ccsid");
            per = rest.contains("per byte");
            if per {
                out.insert(cur, vec![(0, 0); 256]);
            }
        } else if let Some(h) = t.strip_prefix("u ") {
            u.push_str(h);
        } else if let Some(h) = t.strip_prefix("b ") {
            b.push_str(h);
        } else if let Some(p) = t.strip_prefix("p ").filter(|_| per) {
            let f: Vec<&str> = p.split_whitespace().collect();
            let byte = usize::from_str_radix(f[0], 16).expect("byte");
            let unit = u16::from_str_radix(f[1], 16).expect("unit");
            let back = u8::from_str_radix(f[2], 16).expect("back");
            out.get_mut(&cur).expect("section")[byte] = (unit, back);
        }
    }
    flush(cur, &mut u, &mut b, &mut out);
    out
}

#[test]
fn every_swept_table_matches_the_oracle() {
    let s = sweep();
    let want = [
        37u16, 273, 277, 278, 280, 284, 285, 297, 420, 424, 500, 870, 871, 875, 1025, 1026, 1140,
        1141, 1142, 1143, 1144, 1145, 1146, 1147, 1148, 1149,
    ];
    for c in want {
        let rows = s.get(&c).unwrap_or_else(|| panic!("{c} not in the sweep"));
        assert!(matches!(Codec::of(c), Ok(Some(_))), "{c}");
        for (b, &(unit, back)) in rows.iter().enumerate() {
            let byte = u8::try_from(b).expect("byte");
            let u16be = convert(c, 1200, &[byte]).expect("convert");
            let got = u16::from_be_bytes([u16be[0], u16be[1]]);
            assert_eq!(got, unit, "{c} byte {byte:02X} to UTF-16");
            let again = convert(1200, c, &u16be).expect("convert back");
            assert_eq!(again, [back], "{c} byte {byte:02X} back");
        }
    }
}

#[test]
fn unicode_ccsids_the_oracle_converts() {
    for c in [1200u16, 13488, 61952] {
        assert_eq!(convert(37, c, &[0xC1]).unwrap(), [0x00, 0x41], "{c}");
    }
    assert_eq!(convert(37, 1208, &[0xC1]).unwrap(), [0x41]);
    // Refused on the oracle with SQL0189.
    for c in [1202u16, 1232, 1234, 1236] {
        assert!(Codec::of(c).is_err(), "{c}");
    }
}
