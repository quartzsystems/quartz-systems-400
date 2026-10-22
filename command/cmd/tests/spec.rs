// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code's `*CMD` header layout and the block's constants must match
//! the tables in `docs/CMD.md`.

use std::path::Path;

use cmd::block::{ATR_DEFAULT, ATR_NULL, ATR_SPECIFIED};
use cmd::payload::{HDR_LEN, LAYOUT};

fn doc() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/CMD.md");
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
}

/// The table under the heading of `name`: the declared size and the
/// cells of each row, trimmed.
fn table(md: &str, name: &str) -> (Option<usize>, Vec<Vec<String>>) {
    let mut lines = md.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("### `") else {
            continue;
        };
        let (heading, tail) = rest.split_once('`').expect("closing backtick");
        if heading != name {
            continue;
        }
        let size = tail
            .trim()
            .strip_prefix('(')
            .and_then(|t| t.strip_suffix(" bytes)"))
            .and_then(|n| n.parse().ok());
        for l in lines.by_ref() {
            if l.starts_with("| ") {
                break;
            }
        }
        lines.next(); // the separator
        let mut rows = Vec::new();
        while let Some(row) = lines.peek() {
            if !row.starts_with('|') {
                break;
            }
            let cells: Vec<String> = row
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().trim_matches('`').to_string())
                .collect();
            rows.push(cells);
            lines.next();
        }
        return (size, rows);
    }
    panic!("docs/CMD.md has no table `{name}`");
}

#[test]
fn header_matches_the_spec() {
    let md = doc();
    let (size, rows) = table(&md, "CmdHeader");
    assert_eq!(size, Some(HDR_LEN), "the header's size");
    assert_eq!(rows.len(), LAYOUT.len(), "the header's field count");
    let mut end = 0;
    for (row, (name, offset, len)) in rows.iter().zip(LAYOUT) {
        let doc_offset: usize = row[0].parse().expect("an offset");
        let doc_size: usize = row[1].parse().expect("a size");
        assert_eq!(row[2], *name, "field name at offset {offset}");
        assert_eq!(doc_offset, *offset, "{name}: offset");
        assert_eq!(doc_size, *len, "{name}: size");
        assert_eq!(*offset, end, "{name}: fields are contiguous");
        end = offset + len;
    }
    assert_eq!(end, HDR_LEN, "the fields fill the header");
}

#[test]
fn block_constants_match_the_spec() {
    let md = doc();
    let (_, rows) = table(&md, "Block");
    let value = |name: &str| -> u8 {
        let row = rows
            .iter()
            .find(|r| r[0] == name)
            .unwrap_or_else(|| panic!("no row {name}"));
        u8::from_str_radix(row[1].trim_start_matches("0x"), 16).expect("hex")
    };
    assert_eq!(value("ATR_SPECIFIED"), ATR_SPECIFIED);
    assert_eq!(value("ATR_DEFAULT"), ATR_DEFAULT);
    assert_eq!(value("ATR_NULL"), ATR_NULL);
}
