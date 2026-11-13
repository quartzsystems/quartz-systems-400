// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPMFSINF` (`QUIDSPMFS`) against the primary's displays
//! (`tools/conformance/golden/nfs/dspmfsinf-*`, V7R1M0) character for
//! character on rows 1 to 23, filled with the values the primary showed
//! (the message line, IBM's copyright notice there, is not ours).

use std::path::Path;

use uim::render::{MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, compile};

fn golden(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/nfs")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The values the capture shows, by the panel's variables.
fn values(rows: &[String]) -> Vec<(&'static str, String)> {
    let vars = [
        (2, "OBJ"),
        (4, "FSTYPE"),
        (6, "BLKSIZE"),
        (7, "TOTBLK"),
        (8, "FREEBLK"),
        (9, "OBJLNK"),
        (10, "DIRLNK"),
        (11, "NAMEMAX"),
        (12, "PATHMAX"),
        (13, "CHOWN"),
        (14, "NOTRUNC"),
        (15, "CASE"),
    ];
    vars.iter()
        .map(|(r, v)| (*v, rows[*r].get(36..).unwrap_or("").trim().to_string()))
        .collect()
}

#[test]
#[allow(clippy::many_single_char_names)] // rows, columns, values
fn dspmfsinf_matches_the_primary() {
    let text = uim::shipped::source("QUIDSPMFS").expect("shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{}", c.listing);
    let g = c.group.expect("a group");
    let mut bad = Vec::new();
    for name in [
        "dspmfsinf-root",
        "dspmfsinf-qsyslib",
        "dspmfsinf-qopensys",
        "dspmfsinf-homeqstest",
    ] {
        let want = golden(name);
        let mut d = MapDialog::new();
        for (k, v) in values(&want) {
            d.set(k, &v);
        }
        let s = render(&g, "DSPMFSINF", &d, &Options::default()).expect("renders");
        let got: Vec<String> = s
            .text_rows()
            .iter()
            .map(|r| r.trim_end().to_string())
            .collect();
        for r in 0..23 {
            let w = want.get(r).map_or("", String::as_str);
            let o = got.get(r).map_or("", String::as_str);
            if w != o {
                bad.push(format!("{name} row {}:\n  want {w:?}\n  got  {o:?}", r + 1));
            }
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}
