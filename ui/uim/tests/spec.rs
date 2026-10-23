// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code's layouts must match the tables in `docs/UIM.md`.

use std::collections::BTreeMap;
use std::path::Path;

use uim::spec::{Field, Layout, layouts};

struct SpecTable {
    size: Option<usize>,
    fields: Vec<Field>,
}

fn parse_size(s: &str) -> Option<usize> {
    if s == "variable" {
        return None;
    }
    s.strip_suffix(" bytes").and_then(|n| n.parse().ok())
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
            .and_then(parse_size);
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
                offset: cells[1].parse().ok(),
                size: cells[2].parse().ok(),
            });
            lines.next();
        }
        tables.insert(name.to_string(), SpecTable { size, fields });
    }
    tables
}

#[test]
fn layouts_match_the_spec() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/UIM.md");
    let md = std::fs::read_to_string(&path).expect("docs/UIM.md");
    let tables = parse(&md);
    let code: Vec<&Layout> = layouts();
    assert!(!code.is_empty());
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
