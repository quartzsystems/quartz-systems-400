// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The front end and the compilers over the same source with an
//! indicator in the wrong columns: the same message at the same line,
//! for physical, logical, printer and display files.

use dds::analysis::{Kind, Options, Source};
use dds::{CompileOpts, MapRefs};

/// An 80-column DDS line with `A` in 6 and each text at its column.
fn line(parts: &[(usize, &str)]) -> String {
    let mut l: Vec<char> = "     A".chars().collect();
    l.resize(80, ' ');
    for (col, text) in parts {
        for (i, c) in text.chars().enumerate() {
            l[col - 1 + i] = c;
        }
    }
    l.into_iter().collect::<String>().trim_end().to_string()
}

/// The source, the 0-based line the bad indicator is on.
fn source(kind: &str) -> (String, usize) {
    // `51` in 8-9 where an indicator takes 9-10.
    let bad = (8, "51");
    let lines = match kind {
        "PF" => vec![
            line(&[(17, "R"), (19, "CUSTR")]),
            line(&[bad, (19, "CUSTNO"), (34, "7P 0")]),
        ],
        "LF" => vec![
            line(&[(17, "R"), (19, "CUSTR"), (45, "PFILE(CUST)")]),
            line(&[bad, (19, "CUSTNO")]),
        ],
        "PRTF" => vec![
            line(&[(17, "R"), (19, "LINE")]),
            line(&[bad, (19, "FLD"), (33, "10A"), (41, "2"), (44, "5")]),
        ],
        _ => vec![
            line(&[(45, "DSPSIZ(24 80 *DS3)")]),
            line(&[(17, "R"), (19, "FMT1")]),
            line(&[(19, "FLD"), (33, "10A"), (38, "B"), (41, "5"), (43, "10")]),
            line(&[bad, (45, "DSPATR(RI PC)")]),
        ],
    };
    let at = lines.len() - 1;
    (lines.join("\n"), at)
}

/// What the compiler reports on line `at`: IDs.
fn compiled(kind: &str, text: &str, at: usize, refs: &MapRefs) -> Vec<String> {
    let (src, _) = dds::Source::from_text(text);
    let o = CompileOpts::default();
    let diags = match kind {
        "PF" => dds::compile_pf(&src, refs, &o).diagnostics,
        "LF" => dds::compile_lf(&src, refs, &o).diagnostics,
        "PRTF" => dds::compile_prtf(&src, refs, &o).diagnostics,
        _ => dspf::compile_dspf(&src, refs, &o).diagnostics,
    };
    let mut ids: Vec<String> = diags
        .iter()
        .filter(|d| d.line == Some(at))
        .map(|d| d.id.to_string())
        .collect();
    ids.sort();
    ids
}

/// What the front end reports on line `at` (1-based there), with the
/// column every diagnostic of that line carries.
fn front(a: &dds::analysis::Analysis, at: usize) -> (Vec<String>, Vec<u16>) {
    let on: Vec<_> = a
        .diagnostics
        .iter()
        .filter(|d| d.line as usize == at + 1)
        .collect();
    let mut ids: Vec<String> = on.iter().map(|d| d.id.clone()).collect();
    ids.sort();
    (ids, on.iter().map(|d| d.column).collect())
}

fn member(text: &str) -> Source {
    Source::member("QGPL", "QDDSSRC", "BAD", text)
}

#[test]
fn front_end_and_compilers_agree() {
    // The physical file the logical file is over.
    let cust = [
        line(&[(17, "R"), (19, "CUSTR")]),
        line(&[(19, "CUSTNO"), (34, "7P 0")]),
    ]
    .join("\n");
    let (src, _) = dds::Source::from_text(&cust);
    let pf = dds::compile_pf(&src, &MapRefs::default(), &CompileOpts::default());
    let mut refs = MapRefs::default();
    refs.insert_pf("QGPL", "CUST", &pf.definition.expect("CUST compiles"));
    for (kind, want) in [
        ("PF", "DDS0114"),
        ("LF", "DDS0114"),
        ("PRTF", "DDS6101"),
        ("DSPF", "DDS6101"),
    ] {
        let (text, at) = source(kind);
        let compiler = compiled(kind, &text, at, &refs);
        assert!(compiler.contains(&want.to_string()), "{kind}: {compiler:?}");
        let opts = Options {
            kind: Kind::of_srctype(kind).unwrap(),
            refs: Some(&refs),
        };
        let (ids, cols) = front(&dds::analysis::analyze(&member(&text), &opts), at);
        if kind == "DSPF" {
            // The `dds` front end makes only the checks it shares with
            // CRTDSPF; `dspf::analysis` runs CRTDSPF.
            let shared: Vec<String> = compiler
                .iter()
                .filter(|i| i.starts_with("DDS610"))
                .cloned()
                .collect();
            assert_eq!(ids, shared, "{kind}");
            let (full, _) = front(&dspf::analysis::analyze(&member(&text), Some(&refs)), at);
            assert_eq!(full, compiler, "{kind}");
        } else {
            assert_eq!(ids, compiler, "{kind}");
        }
        // The line's first non-blank column, as the compiler's listing
        // places it.
        assert!(cols.iter().all(|c| *c == 6), "{kind}: {cols:?}");
    }
}

#[test]
fn the_message_reads_as_crtdspf_sends_it() {
    let (text, at) = source("DSPF");
    let opts = Options {
        kind: Kind::Display,
        refs: None,
    };
    let a = dds::analysis::analyze(&member(&text), &opts);
    let d = a
        .diagnostics
        .iter()
        .find(|d| d.id == "DDS6101")
        .expect("DDS6101");
    assert_eq!(d.line as usize, at + 1);
    assert!(
        d.text
            .starts_with("Conditioning \"51       \" is not valid"),
        "{}",
        d.text
    );
}
