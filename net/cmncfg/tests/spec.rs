// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*LIND` payload's layout must match its table in
//! `docs/TCPIP.md` ("Communications objects").

use std::path::Path;

/// `(offset, size, name)` of each row of the table under `heading`.
fn table(md: &str, heading: &str) -> Vec<(usize, usize, String)> {
    let mut lines = md.lines().skip_while(|l| l.trim() != heading);
    assert!(lines.next().is_some(), "no {heading} in docs/TCPIP.md");
    lines
        .skip_while(|l| !l.starts_with("| Offset"))
        .skip(2)
        .take_while(|l| l.starts_with('|'))
        .map(|row| {
            let c: Vec<&str> = row.split('|').map(str::trim).collect();
            (
                c[1].parse().expect("offset"),
                c[2].parse().expect("size"),
                c[3].to_string(),
            )
        })
        .collect()
}

#[test]
fn line_description_matches_the_spec() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/TCPIP.md");
    let md = std::fs::read_to_string(&path).expect("docs/TCPIP.md");
    let spec = table(&md, "#### `LineDescription` (272 bytes)");
    let code: Vec<(usize, usize, String)> = cmncfg::lind::LAYOUT
        .fields
        .iter()
        .map(|f| (f.offset.expect("fixed"), f.size.expect("fixed"), f.name.to_string()))
        .collect();
    assert_eq!(spec, code);
    assert_eq!(cmncfg::lind::LAYOUT.size, Some(272));
    let end = code.last().map(|(o, s, _)| o + s);
    assert_eq!(end, Some(272));
}
