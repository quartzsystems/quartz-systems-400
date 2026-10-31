// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The payload layouts must match the tables in `docs/LICPGM.md`, and
//! the encoders must write what the layouts say.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

use std::collections::BTreeMap;
use std::path::Path;

use licpgm::catalog;
use licpgm::payload::spec::{Field, layouts};
use licpgm::payload::{prddfn, prdlod};

#[derive(Debug)]
struct SpecTable {
    size: Option<usize>,
    fields: Vec<Field>,
}

fn num(s: &str) -> Option<usize> {
    s.parse().ok()
}

fn parse(md: &str) -> BTreeMap<String, SpecTable> {
    let mut tables = BTreeMap::new();
    let mut lines = md.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("### `") else {
            continue;
        };
        let (name, tail) = rest.split_once('`').expect("closing backtick");
        let size = tail
            .trim()
            .strip_prefix('(')
            .and_then(|t| t.strip_suffix(')'))
            .and_then(|t| t.strip_suffix(" bytes"))
            .and_then(num);
        for l in lines.by_ref() {
            if l.starts_with("| Offset") {
                break;
            }
        }
        lines.next();
        let mut fields = Vec::new();
        while let Some(row) = lines.peek() {
            if !row.starts_with('|') {
                break;
            }
            let cells: Vec<&str> = row.split('|').map(str::trim).collect();
            fields.push(Field {
                name: Box::leak(cells[3].to_string().into_boxed_str()),
                offset: num(cells[1]),
                size: num(cells[2]),
            });
            lines.next();
        }
        tables.insert(name.to_string(), SpecTable { size, fields });
    }
    tables
}

#[test]
fn layouts_match_the_spec() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/LICPGM.md");
    let md = std::fs::read_to_string(&path).expect("docs/LICPGM.md");
    let tables = parse(&md);
    let code = layouts();
    for layout in &code {
        let table = tables
            .get(layout.name)
            .unwrap_or_else(|| panic!("spec has no table for {}", layout.name));
        assert_eq!(table.size, layout.size, "{}: total size", layout.name);
        assert_eq!(table.fields, layout.fields, "{}: fields", layout.name);
        if let Some(total) = layout.size {
            let mut at = 0;
            for f in layout.fields {
                assert_eq!(f.offset, Some(at), "{}.{}: offset", layout.name, f.name);
                at += f.size.expect("fixed size");
            }
            assert_eq!(at, total, "{}: fields cover the size", layout.name);
        }
    }
    let unspecified: Vec<_> = tables
        .keys()
        .filter(|n| !code.iter().any(|l| l.name == n.as_str()))
        .collect();
    assert!(
        unspecified.is_empty(),
        "spec tables with no code layout: {unspecified:?}"
    );
}

fn ebcdic(s: &str) -> Vec<u8> {
    licpgm::api::fixed(s, s.len())
}

#[test]
fn encoders_follow_the_layouts() {
    let cat = catalog::shipped();
    let p = cat.product("QS00WDS").unwrap();
    let b = prddfn::encode(&p.definition).unwrap();
    let field = |name: &str| {
        prddfn::HEADER
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.offset.unwrap()..f.offset.unwrap() + f.size.unwrap())
            .unwrap()
    };
    assert_eq!(&b[field("magic")], b"QSPRDDFN");
    assert_eq!(&b[field("product_id")], ebcdic("QS00WDS").as_slice());
    assert_eq!(&b[field("ibm_alias")], ebcdic("5770WDS").as_slice());
    let n_opt = u16::from_be_bytes(b[field("option_count")].try_into().unwrap());
    assert_eq!(usize::from(n_opt), p.definition.options.len());
    // The first option entry follows the header; option 31 is third.
    let opt = 192 + 2 * 80;
    assert_eq!(u16::from_be_bytes([b[opt], b[opt + 1]]), 31);
    assert_eq!(&b[opt + 11..opt + 15], ebcdic("5102").as_slice());

    let (_, l) = &p.loads[0];
    let b = prdlod::encode(l).unwrap();
    let field = |name: &str| {
        prdlod::HEADER
            .fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.offset.unwrap()..f.offset.unwrap() + f.size.unwrap())
            .unwrap()
    };
    assert_eq!(&b[field("magic")], b"QSPRDLOD");
    assert_eq!(&b[field("state")], ebcdic(l.state.code()).as_slice());
    assert_eq!(&b[field("load_id")], ebcdic("5001").as_slice());
    // The principal library record follows the header.
    assert_eq!(&b[160 + 10..160 + 14], ebcdic("QSYS").as_slice());
}
