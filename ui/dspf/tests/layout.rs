// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Buffer layouts of compiled display files: field order, external
//! byte lengths, indicator prefixes, `INDARA` and level identifiers.

use dspf::{CompileOpts, DisplayFile, NoRefs, Source, Usage, compile_dspf};

/// A DDS line from (1-based column, text) pairs.
fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    s[5] = 'A';
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

fn kw(cond: &str, kw: &str) -> String {
    at(&[(8, cond), (45, kw)])
}

fn rec(name: &str, kw: &str) -> String {
    at(&[(17, "R"), (19, name), (45, kw)])
}

#[allow(clippy::too_many_arguments)] // one per DDS column
fn fld(
    name: &str,
    len: &str,
    ty: &str,
    dec: &str,
    use_: &str,
    row: &str,
    col: &str,
    kw: &str,
) -> String {
    at(&[
        (19, name),
        (35 - len.len(), len),
        (35, ty),
        (38 - dec.len(), dec),
        (38, use_),
        (42 - row.len(), row),
        (45 - col.len(), col),
        (45, kw),
    ])
}

fn compile(lines: &[String]) -> DisplayFile {
    let (src, d) = Source::from_text(&lines.join("\n"));
    assert!(d.is_empty());
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    c.definition
        .unwrap_or_else(|| panic!("did not compile:\n{}", c.listing))
}

fn body(indara: bool) -> Vec<String> {
    let mut v = Vec::new();
    if indara {
        v.push(kw("", "INDARA"));
    }
    v.push(kw("", "CF03(03 'Exit')"));
    v.push(rec("REC", "CHANGE(41)"));
    v.push(fld("", "", "", "", "", "1", "30", "'Title'"));
    v.push(fld("NAME", "20", "A", "", "B", "3", "2", ""));
    v.push(kw(" 50", "DSPATR(HI)"));
    v.push(fld("AMT", "7", "P", "2", "O", "4", "2", "EDTCDE(J)"));
    v.push(fld("QTY", "5", "S", "0", "I", "5", "2", "DUP(44)"));
    v.push(fld("KEY", "4", "A", "", "H", "", "", ""));
    v.push(fld("CNT", "9", "B", "0", "P", "", "", ""));
    v.push(fld("WHEN", "", "L", "", "O", "6", "2", "DATFMT(*ISO)"));
    v.push(fld("FLT", "17", "F", "0", "H", "", "", ""));
    v.push(fld("MSG", "78", "A", "", "M", "24", "2", ""));
    v
}

#[test]
fn buffers_without_indara() {
    let df = compile(&body(false));
    assert_eq!(df.indicator_area_len(), None);
    let r = df.record("REC").unwrap();
    let l = &r.layout;
    assert_eq!(l.option_inds, [50]);
    assert_eq!(l.response_inds, [3, 41, 44]);
    // Output: 1 indicator byte, then NAME 20, AMT 4, KEY 4, CNT 4, WHEN 10, FLT 8, MSG 78.
    assert_eq!(l.output.indicators, [50]);
    let names: Vec<(&str, u32, u32)> = l
        .output
        .fields
        .iter()
        .map(|b| (r.fields[b.field].name.as_deref().unwrap(), b.offset, b.len))
        .collect();
    assert_eq!(
        names,
        [
            ("NAME", 1, 20),
            ("AMT", 21, 4),
            ("KEY", 25, 4),
            ("CNT", 29, 4),
            ("WHEN", 33, 10),
            ("FLT", 43, 8),
            ("MSG", 51, 78),
        ]
    );
    assert_eq!(l.output.len, 129);
    // Input: 3 indicator bytes, then NAME 20, QTY 5, KEY 4, FLT 8.
    let names: Vec<(&str, u32, u32)> = l
        .input
        .fields
        .iter()
        .map(|b| (r.fields[b.field].name.as_deref().unwrap(), b.offset, b.len))
        .collect();
    assert_eq!(
        names,
        [
            ("NAME", 3, 20),
            ("QTY", 23, 5),
            ("KEY", 28, 4),
            ("FLT", 32, 8)
        ]
    );
    assert_eq!(l.input.len, 40);
    assert_eq!(l.level_id.len(), 13);
    let (_, qty) = r.field("QTY").unwrap();
    assert_eq!(qty.usage, Usage::Input);
    assert_eq!(qty.screen_len, 6, "an input number has a sign position");
    let (_, amt) = r.field("AMT").unwrap();
    assert_eq!(
        amt.screen_len, 10,
        "EDTCDE(J): 7 digits, 1 comma, a point, a sign"
    );
}

#[test]
fn indara_moves_the_indicators_out() {
    let df = compile(&body(true));
    assert_eq!(df.indicator_area_len(), Some(99));
    let r = df.record("REC").unwrap();
    let l = &r.layout;
    assert!(l.output.indicators.is_empty() && l.input.indicators.is_empty());
    assert_eq!(l.option_inds, [50], "the indicators are still known");
    assert_eq!(l.output.len, 128);
    assert_eq!(l.input.len, 37);
    assert_eq!(l.output.fields[0].offset, 0);
    let plain = compile(&body(false));
    assert_eq!(
        l.level_id,
        plain.record("REC").unwrap().layout.level_id,
        "INDARA is not part of the level"
    );
}

#[test]
fn level_id_tracks_the_shape_only() {
    let base = compile(&body(false))
        .record("REC")
        .unwrap()
        .layout
        .level_id
        .clone();
    let mut moved = body(false);
    moved[3] = fld("NAME", "20", "A", "", "B", "7", "40", "COLOR(RED)");
    assert_eq!(
        compile(&moved).record("REC").unwrap().layout.level_id,
        base,
        "position and attributes do not change the level"
    );
    let mut longer = body(false);
    longer[3] = fld("NAME", "21", "A", "", "B", "3", "2", "");
    assert_ne!(
        compile(&longer).record("REC").unwrap().layout.level_id,
        base
    );
    let mut usage = body(false);
    usage[3] = fld("NAME", "20", "A", "", "O", "3", "2", "");
    assert_eq!(
        compile(&usage).record("REC").unwrap().layout.level_id,
        base,
        "usage does not change the level"
    );
}
