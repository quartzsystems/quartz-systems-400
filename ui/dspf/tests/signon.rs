// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped sign-on displays against IBM's: `QDSIGNON` (password
//! level 0 and 1) row by row against the 7.1 capture, text and
//! attributes through a terminal; `QDSIGNON2` (level 2 and 3) against
//! the same capture with IBM's `QDSIGNON2` geometry applied (its DDS,
//! read on the 7.1 system: the 128-character password at row 7 column
//! 53, the program, menu and library rows at 10 to 12).

use std::fmt::Write as _;
use std::path::Path;

use dspf::rt::{Env, Runtime};
use dspf::{CompileOpts, NoRefs, Source, compile_dspf};
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Encoding, Read, ScreenModel};
use tn5250::term::Terminal;

fn golden(ext: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/oracle71/signon")
        .join(format!("sign-on.{ext}"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// The SIGNON format written with the capture's values.
fn written(text: &str) -> ScreenModel {
    let (src, diags) = Source::from_text(text);
    assert!(diags.is_empty(), "{diags:?}");
    let c = compile_dspf(&src, &NoRefs, &CompileOpts::default());
    let df = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let rec = df.record("SIGNON").expect("format").clone();
    let values = [
        ("QSYSNAME", "SYSNAME"),
        ("SBSNAME", "QBASE"),
        ("DEVNAME", "QPADEV0001"),
    ];
    let mut out = Vec::new();
    for bf in &rec.layout.output.fields {
        let f = &rec.fields[bf.field];
        let v = values
            .iter()
            .find(|(n, _)| f.name.as_deref() == Some(*n))
            .map_or("", |(_, v)| v);
        let mut b = ccsid::to_ebcdic(&format!("{v:<w$}", w = bf.len as usize)).unwrap();
        b.truncate(bf.len as usize);
        out.extend(b);
    }
    let mut rt = Runtime::new(df, false);
    let mut m = ScreenModel::new(false);
    rt.write(&mut m, &Env::default(), "SIGNON", &out, &[])
        .unwrap();
    m
}

fn terminal() -> Terminal {
    Terminal::new(Capabilities {
        model: Model::M3179,
        gui: false,
        color: true,
    })
}

/// The SIGNON format as a terminal shows it: text rows and attribute
/// rows.
fn shown(text: &str) -> (Vec<String>, Vec<String>, Terminal) {
    let o = written(text).render(Read::MdtFields);
    let mut t = terminal();
    t.apply(&Record::new(o.opcode, o.data)).unwrap();
    (t.text_rows(), t.attr_rows(), t)
}

/// Level 2 and 3: IBM's `QDSIGNON2` moves rows 8 to 10 down two.
fn level2(rows: &[String]) -> Vec<String> {
    let mut v = rows.to_vec();
    v[9..12].clone_from_slice(&rows[7..10]);
    v[7] = " ".repeat(80);
    v[8] = " ".repeat(80);
    v
}

/// Rows 1 to 12 of text; the copyright row by where it starts.
fn compare_text(want: &[String], got: &[String]) {
    let mut diff = String::new();
    for i in 0..12 {
        let (w, o) = (want[i].trim_end(), got[i].trim_end());
        if w != o {
            let _ = writeln!(diff, "row {}:\n  want |{w}|\n  got  |{o}|", i + 1);
        }
    }
    let mark = "(C) COPYRIGHT";
    if want[23].find(mark) != got[23].find(mark) {
        let _ = writeln!(diff, "row 24:\n  want |{}|\n  got  |{}|", want[23], got[23]);
    }
    assert!(diff.is_empty(), "the sign-on differs:\n{diff}");
}

/// The attributes of the label rows, columns 16 to 62: the label's
/// attribute byte, the label, the field's attribute byte and its first
/// ten cells (IBM leaves no attribute after a field, so what follows
/// is not compared).
fn compare_attrs(want: &[String], got: &[String], rows: &[usize]) {
    let mut diff = String::new();
    for &r in rows {
        let (w, o) = (&want[r - 1][15..62], &got[r - 1][15..62]);
        if w != o {
            let _ = writeln!(diff, "row {r}:\n  want |{w}|\n  got  |{o}|");
        }
    }
    assert!(diff.is_empty(), "the sign-on's attributes differ:\n{diff}");
}

#[test]
fn qdsignon_matches_the_71_capture() {
    let (text, attrs, t) = shown(dspf::sys::QDSIGNON);
    compare_text(&golden("txt"), &text);
    compare_attrs(&golden("attr"), &attrs, &[6, 7, 8, 9, 10]);
    let lens: Vec<usize> = t.fields().iter().map(|f| f.len).collect();
    assert_eq!(lens, [10, 10, 10, 10, 10]);
}

#[test]
fn qdsignon2_has_ibms_level_2_geometry() {
    let (text, attrs, t) = shown(dspf::sys::QDSIGNON2);
    let want_text = level2(&golden("txt"));
    compare_text(&want_text, &text);
    let want_attrs = level2(&golden("attr"));
    compare_attrs(&want_attrs, &attrs, &[6, 10, 11, 12]);
    // The password: nondisplay from row 7 column 53 through row 9
    // column 20, and nothing of it on a label.
    let pw = &t.fields()[1];
    assert_eq!((pw.start, pw.len), (6 * 80 + 52, 128));
    assert_eq!(&attrs[6][15..52], &want_attrs[6][15..52]);
    assert!(attrs[6][52..].chars().all(|c| c == '7'));
    assert!(attrs[7].chars().all(|c| c == '7'));
    assert!(attrs[8][..20].chars().all(|c| c == '7'));
    assert!(text[7].trim().is_empty() && text[8].trim().is_empty());
}

/// The sign-on record compressed and plain: the same screen, the same
/// fields, the same cursor, in far fewer bytes.
#[test]
fn the_sign_on_record_is_compressed() {
    for src in [dspf::sys::QDSIGNON, dspf::sys::QDSIGNON2] {
        let mut zip = written(src);
        let mut plain = zip.clone();
        let a = zip.render_with(Read::MdtFields, Encoding::Compressed);
        let b = plain.render_with(Read::MdtFields, Encoding::Plain);
        let (mut ta, mut tb) = (terminal(), terminal());
        ta.apply(&Record::new(a.opcode, a.data.clone())).unwrap();
        tb.apply(&Record::new(b.opcode, b.data.clone())).unwrap();
        assert_eq!(ta.cells(), tb.cells());
        assert_eq!(ta.fields(), tb.fields());
        assert_eq!(ta.cursor(), tb.cursor());
        assert_eq!(ta.key_switches(), tb.key_switches());
        println!(
            "sign-on: {} bytes compressed, {} plain",
            a.data.len(),
            b.data.len()
        );
        assert!(
            a.data.len() * 2 < b.data.len(),
            "{} vs {}",
            a.data.len(),
            b.data.len()
        );
    }
}
