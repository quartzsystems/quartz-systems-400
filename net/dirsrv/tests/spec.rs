// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry store's layouts must match the tables of docs/DIRSRV.md
//! §5.2, as storage/store/tests/spec.rs does for the object store.

use std::collections::BTreeMap;
use std::path::Path;

use dirsrv::store::format::layouts;
use store::spec::{Field, Layout};

#[derive(Debug)]
struct SpecTable {
    size: Option<usize>,
    fields: Vec<Field>,
}

/// The tables of §5.2: a level-four heading naming the layout and its
/// size, then the table.
fn parse(md: &str) -> BTreeMap<String, SpecTable> {
    let start = md.find("### 5.2 Layouts").expect("§5.2");
    let rest = &md[start..];
    let end = rest[4..].find("\n## ").map_or(rest.len(), |e| e + 4);
    let mut tables = BTreeMap::new();
    let mut lines = rest[..end].lines().peekable();
    while let Some(line) = lines.next() {
        let Some(head) = line.strip_prefix("#### `") else {
            continue;
        };
        let (name, tail) = head.split_once('`').expect("closing backtick");
        let size = tail
            .trim()
            .strip_prefix('(')
            .and_then(|t| t.strip_suffix(" bytes)"))
            .and_then(|n| n.parse().ok());
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
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/DIRSRV.md");
    let md = std::fs::read_to_string(&path).expect("docs/DIRSRV.md");
    let tables = parse(&md);
    let code: Vec<&Layout> = layouts();
    assert_eq!(code.len(), tables.len(), "tables: {:?}", tables.keys());
    for layout in &code {
        let table = tables
            .get(layout.name)
            .unwrap_or_else(|| panic!("§5.2 has no table for {}", layout.name));
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
}
