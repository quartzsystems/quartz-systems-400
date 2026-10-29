// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The constants and layouts must match the tables of docs/TCPIP.md
//! "Sockets".

use std::collections::BTreeMap;
use std::path::Path;

/// Offset, size, field name.
type Row = (usize, usize, String);

fn section() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/TCPIP.md");
    let md = std::fs::read_to_string(path).expect("docs/TCPIP.md");
    let start = md.find("\n## Sockets").expect("a Sockets section");
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

#[test]
fn constants_match_the_spec() {
    let s = section();
    let at = s.find("### Constants").expect("constants table");
    let mut doc = BTreeMap::new();
    for row in s[at..].lines().skip_while(|l| !l.starts_with("| Name")).skip(2) {
        if !row.starts_with('|') {
            break;
        }
        let c = cells(row);
        let v: i64 = c[1].parse().unwrap_or_else(|_| panic!("value of {}", c[0]));
        assert!(doc.insert(c[0].clone(), v).is_none(), "{} twice", c[0]);
    }
    let code: BTreeMap<String, i64> = sockets::consts::ALL.iter().map(|(n, v)| ((*n).to_string(), *v)).collect();
    assert_eq!(code.len(), sockets::consts::ALL.len(), "names unique in the code");
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
            if n.starts_with("#### ") || n.starts_with("## ") {
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
    let code = sockets::layout::layouts();
    assert_eq!(doc.len(), code.len(), "one table per layout");
    for l in code {
        let (size, fields) = doc.get(l.name).unwrap_or_else(|| panic!("no table for {}", l.name));
        assert_eq!(*size, l.size, "{}", l.name);
        let want: Vec<Row> = l.fields.iter().map(|f| (f.offset, f.size, f.name.to_string())).collect();
        assert_eq!(fields, &want, "{}", l.name);
    }
}
