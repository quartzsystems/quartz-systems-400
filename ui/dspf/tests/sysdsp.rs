// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system displays shipped as display files against the pub400
//! goldens: each format written with the golden's visible values and
//! compared row by row. The copyright row counts by where it starts;
//! the message row is the oracle's reaction to the capture's keys and
//! is not compared.

use std::fmt::Write as _;
use std::path::Path;

use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, NoRefs, Source, compile_dspf};
use tn5250::screen::ScreenModel;

fn golden_rows(name: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/pub400")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A shipped file's format written with its output fields set.
fn written(text: &str, format: &str, values: &[(&str, &str)], inds: &[u8]) -> Vec<String> {
    let (src, diags) = Source::from_text(text);
    assert!(diags.is_empty(), "{diags:?}");
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    let df = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let rec = df.record(format).expect("format").clone();
    let mut out = Vec::new();
    for n in &rec.layout.output.indicators {
        out.push(if inds.contains(n) { 0xF1 } else { 0xF0 });
    }
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        let v = values
            .iter()
            .find(|(n, _)| f.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(n)))
            .map_or("", |(_, v)| v);
        let mut b = ccsid::to_ebcdic(&format!("{v:<w$}", w = bf.len as usize)).unwrap();
        b.truncate(bf.len as usize);
        out.extend(b);
    }
    let mut rt = Runtime::new(df, false);
    let mut m = ScreenModel::new(false);
    rt.write(&mut m, &Env::default(), format, &out, &[]).unwrap();
    m.text_rows()
}

/// Rows 1 to `upto` against the golden; a copyright row by its start.
fn compare(golden: &str, got: &[String], upto: usize) {
    let want = golden_rows(golden);
    let mut diff = String::new();
    for i in 0..upto {
        let (w, o) = (want[i].trim_end(), got[i].trim_end());
        let mark = "(C) COPYRIGHT";
        if w.contains(mark) && w.find(mark) == o.find(mark) {
            continue;
        }
        if w != o {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{o}|", i + 1);
        }
    }
    assert!(diff.is_empty(), "{golden} differs:\n{diff}");
}

#[test]
fn sysreq_menu() {
    let rows = written(dspf::sys::QWSSYSRQ, "SYSRQ", &[("SYSNAME", "SYSNAME")], &[]);
    compare("sysreq-menu", &rows, 23);
}

/// The second format: the same rows, the group and secondary options
/// between 7 and 80.
#[test]
fn sysreq_menu_with_group_options() {
    let rows = written(dspf::sys::QWSSYSRQ, "SYSRQ2", &[("SYSNAME", "S1")], &[50, 51, 52]);
    let want = golden_rows("sysreq-menu");
    for i in [0, 2, 4, 10, 17, 18, 21] {
        assert_eq!(rows[i].trim_end(), want[i].trim_end().replace("SYSNAME", "S1"));
    }
    assert!(rows[11].contains("13."));
    assert!(rows[13].contains("15."));
    assert!(rows[15].contains("80."));
    assert!(rows[16].contains("90."));
    let only_group = written(dspf::sys::QWSSYSRQ, "SYSRQ2", &[], &[50, 51]);
    assert!(only_group[11].contains("13.") && !only_group[13].contains("15."));
}
