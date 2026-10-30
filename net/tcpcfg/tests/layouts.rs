// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The record formats marked IBM's against the oracle's `DSPFFD` of
//! `QUSRSYS` (pub400, V7R5M0): format name, record length, and every
//! field's name, offset, length, digits and type.

use std::path::Path;

use tcpcfg::files::{Ty, defs};

/// `(format, record length, [(field, offset 1-based, bytes, digits, type)])`.
type Captured = (String, usize, Vec<(String, usize, usize, u32, char)>);

fn captured(file: &str) -> Captured {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/net/pub400/config-file-fields.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut format = String::new();
    let mut len = 0;
    let mut fields = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 13 || f[0] != "QUSRSYS" || f[1] != file {
            continue;
        }
        format = f[3].to_string();
        len = f[4].parse().unwrap();
        fields.push((
            f[5].to_string(),
            f[6].parse().unwrap(),
            f[7].parse().unwrap(),
            f[8].parse().unwrap(),
            f[10].chars().next().unwrap(),
        ));
    }
    assert!(!fields.is_empty(), "{file} is not in the capture");
    (format, len, fields)
}

#[test]
fn ibm_formats_match_the_oracle() {
    let mut checked = 0;
    for l in defs::ALL.iter().filter(|l| l.ibm) {
        let (format, len, fields) = captured(l.file);
        assert_eq!(l.format, format, "{}", l.file);
        assert_eq!(l.record_len(), len, "{} record length", l.file);
        assert_eq!(l.fields.len(), fields.len(), "{} field count", l.file);
        let mut at = 1;
        for (ours, theirs) in l.fields.iter().zip(&fields) {
            let ty = match ours.ty {
                Ty::A => 'A',
                Ty::B => 'B',
                Ty::G => 'G',
            };
            let digits = if ours.ty == Ty::B { ours.len } else { 0 };
            assert_eq!(
                (ours.name.to_string(), at, ours.bytes(), digits, ty),
                theirs.clone(),
                "{} field {}",
                l.file,
                ours.name
            );
            at += ours.bytes();
        }
        checked += 1;
    }
    assert_eq!(checked, 10);
}

#[test]
fn our_formats_are_named_apart() {
    for l in defs::ALL.iter().filter(|l| !l.ibm) {
        assert!(l.file.len() <= 10 && l.format.len() <= 10, "{}", l.file);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/oracle/net/pub400/config-file-fields.txt");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(
            !text.contains(&format!("|{}|", l.file)),
            "{} is an IBM file; ours must not take its name",
            l.file
        );
    }
}
