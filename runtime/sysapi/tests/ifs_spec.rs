// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS API structures, constants and attribute sizes must match
//! the tables of docs/IFS.md "APIs".

use std::collections::BTreeMap;
use std::path::Path;

/// Offset, size, field name.
type Row = (usize, usize, String);

fn section() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/IFS.md");
    let md = std::fs::read_to_string(path).expect("docs/IFS.md").replace("\r\n", "\n");
    let start = md.find("\n## APIs").expect("an APIs section");
    let rest = &md[start + 1..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    rest[..end].to_string()
}

fn cells(row: &str) -> Vec<String> {
    row.trim()
        .trim_matches('|')
        .split('|')
        .map(|c| c.trim().trim_matches('`').to_string())
        .collect()
}

/// The rows of the table after a heading.
fn table(s: &str, heading: &str) -> Vec<Vec<String>> {
    let at = s.find(heading).unwrap_or_else(|| panic!("{heading}"));
    s[at..]
        .lines()
        .skip_while(|l| !l.starts_with('|'))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(cells)
        .collect()
}

#[test]
fn constants_match_the_spec() {
    let s = section();
    let mut doc = BTreeMap::new();
    for c in table(&s, "### Constants") {
        let v: i64 = c[1].parse().unwrap_or_else(|_| panic!("value of {}", c[0]));
        assert!(doc.insert(c[0].clone(), v).is_none(), "{} twice", c[0]);
    }
    let all = sysapi::ifs::consts::ALL;
    let code: BTreeMap<String, i64> = all.iter().map(|(n, v)| ((*n).to_string(), *v)).collect();
    assert_eq!(code.len(), all.len(), "names unique in the code");
    assert_eq!(doc, code);
}

#[test]
fn attribute_sizes_match_the_spec() {
    let s = section();
    let doc: Vec<(String, u32, usize)> = table(&s, "### Attribute data")
        .into_iter()
        .map(|c| (c[0].clone(), c[1].parse().expect("id"), c[2].parse().expect("size")))
        .collect();
    let code: Vec<(String, u32, usize)> = sysapi::ifs::ATTR_DATA
        .iter()
        .map(|(n, i, z)| ((*n).to_string(), *i, *z))
        .collect();
    assert_eq!(doc, code);
}

#[test]
fn layouts_match_the_spec() {
    let s = section();
    let mut doc: BTreeMap<String, (usize, Vec<Row>)> = BTreeMap::new();
    let mut lines = s.lines().peekable();
    while let Some(l) = lines.next() {
        let Some(rest) = l.strip_prefix("#### `") else { continue };
        let (name, tail) = rest.split_once('`').expect("closing backtick");
        let size: usize = tail
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(" bytes)")
            .parse()
            .unwrap_or_else(|_| panic!("size of {name}"));
        let mut fields = Vec::new();
        while let Some(n) = lines.peek() {
            if n.starts_with("#### ") || n.starts_with("### ") || n.starts_with("## ") {
                break;
            }
            let n = lines.next().expect("peeked");
            if !n.starts_with('|') || n.starts_with("| Offset") || n.starts_with("| ---") {
                continue;
            }
            let c = cells(n);
            fields.push((c[0].parse().expect("offset"), c[1].parse().expect("size"), c[2].clone()));
        }
        doc.insert(name.to_string(), (size, fields));
    }
    let code = sysapi::ifs::layout::layouts();
    assert_eq!(doc.len(), code.len(), "one table per layout");
    for l in code {
        let (size, fields) = doc.get(l.name).unwrap_or_else(|| panic!("no table for {}", l.name));
        assert_eq!(*size, l.size, "{}", l.name);
        let want: Vec<Row> = l.fields.iter().map(|f| (f.offset, f.size, f.name.to_string())).collect();
        assert_eq!(fields, &want, "{}", l.name);
    }
}
