// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code's layouts must match the tables in `docs/MEDIA.md`.

use std::collections::BTreeMap;
use std::path::Path;

use tape::spec::{Field, Layout, layouts};

/// A table from the spec: declared size and fields.
struct Table {
    size: usize,
    fields: Vec<(String, usize, usize)>,
}

/// Every "### `name` (N bytes)" table in the document.
fn parse(md: &str) -> BTreeMap<String, Table> {
    let mut tables = BTreeMap::new();
    let mut lines = md.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("### `") else {
            continue;
        };
        let (name, tail) = rest.split_once('`').expect("closing backtick");
        let Some(size) = tail
            .trim()
            .strip_prefix('(')
            .and_then(|t| t.strip_suffix(" bytes)"))
            .and_then(|n| n.parse().ok())
        else {
            continue;
        };
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
            fields.push((
                cells[3].to_string(),
                cells[1].parse().expect("offset"),
                cells[2].parse().expect("size"),
            ));
            lines.next();
        }
        tables.insert(name.to_string(), Table { size, fields });
    }
    tables
}

fn tiles(l: &Layout) {
    let mut at = 0;
    for f in l.fields {
        assert_eq!(
            f.offset, at,
            "{}: {} does not follow the field before",
            l.name, f.name
        );
        at += f.size;
    }
    assert_eq!(at, l.size, "{}: fields do not fill it", l.name);
}

#[test]
fn layouts_match_the_spec() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/MEDIA.md");
    let md = std::fs::read_to_string(&path).expect("docs/MEDIA.md");
    let tables = parse(&md);
    let code = layouts();
    assert_eq!(code.len(), 5);
    for layout in code {
        tiles(layout);
        let t = tables
            .get(layout.name)
            .unwrap_or_else(|| panic!("MEDIA.md has no table for {}", layout.name));
        assert_eq!(t.size, layout.size, "{} size", layout.name);
        let ours: Vec<(String, usize, usize)> = layout
            .fields
            .iter()
            .map(|&Field { name, offset, size }| (name.to_string(), offset, size))
            .collect();
        assert_eq!(t.fields, ours, "{} fields", layout.name);
    }
}

#[test]
fn encoded_structures_have_their_layout_sizes() {
    use tape::label::{Code, FileLabel, LabelSet, Vol1};
    use tape::virt::format::{FRAME, HEADER, encode_frame};
    let v = Vol1 {
        volume_id: "A".into(),
        owner: String::new(),
    };
    assert_eq!(
        v.encode(Code::Ebcdic).unwrap().len(),
        tape::label::layout::VOL1.size
    );
    let (h1, h2) = FileLabel::default()
        .encode(LabelSet::Header, Code::Ebcdic)
        .unwrap();
    assert_eq!((h1.len(), h2.len()), (80, 80));
    assert_eq!(encode_frame(1, 1, b"x").len(), FRAME.size);
    assert_eq!(HEADER.size, tape::virt::format::HEADER_LEN);
}
