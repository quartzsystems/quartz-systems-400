// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKSYSACT`'s display file `QPYSYSAC` against the primary's captures
//! (`tools/conformance/golden/phase12/perf/wrksysact*.txt`, V7R1M0):
//! each view and key set written with the capture's figures and
//! compared row by row; the copyright row by where it starts.

use std::fmt::Write as _;
use std::path::Path;

use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, NoRefs, Source, compile_dspf};
use perf::sysact::{Entry, headings, keys, legend, row};
use tn5250::screen::ScreenModel;

fn golden(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase12/perf")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// The format written with `values`.
fn written(values: &[(&str, String)]) -> Vec<String> {
    let (src, diags) = Source::from_text(dspf::sys::QPYSYSAC);
    assert!(diags.is_empty(), "{diags:?}");
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    let df = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let rec = df.record("WSA").expect("format").clone();
    let mut out = Vec::new();
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        let v = values
            .iter()
            .find(|(n, _)| f.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(n)))
            .map_or("", |(_, v)| v.as_str());
        let mut b = ccsid::to_ebcdic(&format!("{v:<w$}", w = bf.len as usize)).unwrap();
        b.truncate(bf.len as usize);
        out.extend(b);
    }
    let mut rt = Runtime::new(df, false);
    let mut m = ScreenModel::new(false);
    rt.write(&mut m, &Env::default(), "WSA", &out, &[]).unwrap();
    m.text_rows()
}

/// Every row against the capture, the copyright by where it starts.
fn compare(name: &str, got: &[String]) {
    let want = golden(name);
    let mut diff = String::new();
    for i in 0..24 {
        if i == 23 && want[i].contains("(C) COPYRIGHT") {
            continue;
        }
        let (w, o) = (want[i].trim_end(), got[i].trim_end());
        if w != o {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{o}|", i + 1);
        }
    }
    let mark = "(C) COPYRIGHT";
    assert_eq!(want[23].find(mark), got[23].find(mark), "{name}: the copyright row");
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

fn entry(name: &str, user: &str, number: &str, thread: &str, priority: u8) -> Entry {
    Entry {
        name: name.into(),
        user: user.into(),
        number: number.into(),
        thread: thread.into(),
        priority,
        ..Entry::default()
    }
}

/// The header and list values of a view and key set.
const ROWS: [&str; 6] = ["ROW01", "ROW02", "ROW03", "ROW04", "ROW05", "ROW06"];

fn values(view: usize, set: usize, avg: &str, overall: &str, entries: &[Entry]) -> Vec<(&'static str, String)> {
    let first = view == 0 && set == 0 && avg == "104.5";
    let h = headings(view);
    let (k1, k2) = keys(view, set);
    let mut v: Vec<(&'static str, String)> = vec![
        ("SYSNAME", "S062B175".into()),
        ("DATE", "MM/DD/YY".into()),
        ("TIME", "HH:MM:SS".into()),
        ("REFRESH", format!("{:>5}", 5)),
        ("FILTER", format!("{:>5}", ".10")),
        ("ELAPSED", "HH:MM:SS".into()),
        ("OVERALL", format!("{overall:>6}")),
        ("DBCPU", format!("{:>7}", ".0")),
        ("AVGRATE", format!("{avg:>7}")),
        ("CAPACITY", format!("{:>8}", "1.00")),
        ("LEGEND", legend(view).into()),
        ("HEAD1", h[0].into()),
        ("HEAD2", h[1].into()),
        ("HEAD3", h[2].into()),
        ("MORE", "Bottom".into()),
        ("KEYS1", k1),
        ("KEYS2", k2),
        ("COPYR", if first { perf::sysact::COPYRIGHT.into() } else { String::new() }),
    ];
    for (i, e) in entries.iter().enumerate() {
        v.push((ROWS[i], row(view, e)));
    }
    v
}

#[test]
fn views_one_to_three() {
    let mut a = entry("QPADEV0001", "USERNAM", "009069", "0000003B", 1);
    a.cpu = 5;
    a.other_reads = 16;
    let mut b = entry("QSLPSVR", "QSYS", "008352", "00000006", 10);
    b.cpu = 1;
    let jobs = [a, b];
    compare("wrksysact", &written(&values(0, 0, "104.5", "1.0", &jobs)));
    compare("wrksysact-f11", &written(&values(1, 0, "104.5", "1.0", &jobs)));
    compare("wrksysact-f11-2", &written(&values(2, 0, "104.5", "1.0", &jobs)));
    compare("wrksysact-f24", &written(&values(2, 1, "104.5", "1.0", &jobs)));
}

#[test]
fn view_four_and_the_last_keys() {
    let mut a = entry("QPADEV0001", "USERNAM", "009070", "0000003C", 1);
    a.allocated = 108;
    a.wait = 991;
    let mut b = entry("QSLPSVR", "QSYS", "008352", "00000006", 10);
    b.wait = 1002;
    let mut c = entry("QGLDPUBA", "QDIRSRV", "008031", "00000001", 50);
    c.allocated = 13;
    c.deallocated = 13;
    c.wait = 99_999;
    compare("wrksysact-v4", &written(&values(3, 0, "104.2", "1.1", &[a.clone(), b.clone(), c.clone()])));
    // View 5 (the second time round) is view 1 again.
    let mut v5a = a.clone();
    v5a.cpu = 5;
    v5a.other_reads = 14;
    let mut v5b = b.clone();
    v5b.cpu = 1;
    let mut v5c = c.clone();
    v5c.cpu = 1;
    v5c.other_writes = 12;
    v5c.async_io = 2;
    compare("wrksysact-v5", &written(&values(0, 0, "104.2", "1.1", &[v5a, v5b, v5c])));
    let mut f24c = c;
    f24c.async_io = 2;
    let f24: Vec<Entry> = [a, b, f24c]
        .into_iter()
        .map(|mut e| {
            e.other_reads = 0;
            e
        })
        .collect();
    compare("wrksysact-f24-2", &written(&values(2, 2, "104.2", "1.1", &f24)));
}
