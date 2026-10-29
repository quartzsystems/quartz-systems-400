// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host print transform against golden files under
//! `tests/golden/`: one small report through each model and option the
//! transform distinguishes. The files show the bytes readably (`<ESC>`,
//! `<CR>`, `<LF>` then a line break, `<FF>`, `<xHH>`); they were
//! checked by hand against the PCL5 command reference, not against
//! IBM's transform. `QS400_UPDATE_GOLDEN=1` rewrites them.

use std::fmt::Write as _;
use std::path::PathBuf;

use print::job::{Duplex, PrintJob};
use print::transform::{Model, Transform};
use print::wscst::Wscst;
use spool::{Control, PageFormat, Printer};

/// A two-page report: a heading, a skipped-to total line, an accented
/// name, a line longer than the page.
fn report(width: u16, len: u16) -> Vec<u8> {
    let e = |s: &str| ccsid::to_ebcdic(s).unwrap();
    let mut p = Printer::new(PageFormat {
        len,
        width,
        ovrflw: len.saturating_sub(6),
        ..PageFormat::default()
    });
    p.write(
        &e("QS/400 PAYROLL REGISTER                     PAGE 1"),
        Control {
            skip_before: 2,
            ..Control::default()
        },
    );
    p.write(
        &e("EMPLOYEE    NAME              AMOUNT"),
        Control {
            space_before: 2,
            ..Control::default()
        },
    );
    p.write_line(&e("000101      Ren\u{e9} Dupr\u{e9}       1,234.50"));
    p.write_line(&e("000102      J\u{fc}rgen M\u{fc}ller     987.00"));
    p.write_line(&e(&"*".repeat(usize::from(width) + 10)));
    p.write(
        &e("TOTAL                         2,221.50"),
        Control {
            skip_before: 2,
            ..Control::default()
        },
    );
    p.take()
}

/// Bytes as the golden files show them.
fn readable(b: &[u8]) -> String {
    let mut out = String::new();
    for &c in b {
        match c {
            0x1b => out.push_str("<ESC>"),
            b'\r' => out.push_str("<CR>"),
            b'\n' => out.push_str("<LF>\n"),
            0x0c => out.push_str("<FF>\n"),
            0x20..=0x7e => out.push(char::from(c)),
            _ => {
                let _ = write!(out, "<x{c:02X}>");
            }
        }
    }
    out
}

fn check(name: &str, got: &[u8]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    let text = readable(got);
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_GOLDEN=1 writes it)", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(text, want, "{name} differs from its golden file");
}

fn run(name: &str, t: &Transform, job: &PrintJob, width: u16, len: u16) {
    let out = t.apply(job, &report(width, len));
    check(name, &out.data);
}

#[test]
fn hp4_wide_report_reduced_to_letter_landscape() {
    let job = PrintJob::default();
    run("hp4-132x66", &Transform::Pcl(Model::Hp4), &job, 132, 66);
}

#[test]
fn hpii_narrow_report_pcl4_fonts() {
    let job = PrintJob {
        page_width: 80,
        ..PrintJob::default()
    };
    run("hpii-80x66", &Transform::Pcl(Model::HpII), &job, 80, 66);
}

#[test]
fn hp5si_duplex_tray_copies() {
    let job = PrintJob {
        page_width: 80,
        page_len: 84,
        copies: 2,
        drawer: 2,
        duplex: Duplex::Tumble,
        ..PrintJob::default()
    };
    run("hp5si-80x84-tumble", &Transform::Pcl(Model::Hp5SI), &job, 80, 84);
}

#[test]
fn hp5_font_and_chrid() {
    let job = PrintJob {
        page_width: 132,
        font: "252".into(),
        lpi: 8,
        chrid: "695 1252".into(),
        ..PrintJob::default()
    };
    run("hp5-font252-cp1252", &Transform::Pcl(Model::Hp5), &job, 132, 66);
}

#[test]
fn hpplus_a4_by_cpi() {
    let job = PrintJob {
        page_width: 96,
        page_len: 70,
        cpi: 12,
        chrid: "697 850".into(),
        ..PrintJob::default()
    };
    run("hpplus-96x70-cpi12", &Transform::Pcl(Model::HpPlus), &job, 96, 70);
}

#[test]
fn ascii_text() {
    let job = PrintJob {
        page_width: 80,
        ..PrintJob::default()
    };
    run("ascii-80x66", &Transform::Ascii, &job, 80, 66);
    let job = PrintJob {
        page_width: 80,
        chrid: "697 819".into(),
        ..PrintJob::default()
    };
    run("ascii-80x66-cp819", &Transform::Ascii, &job, 80, 66);
}

#[test]
fn wscst_sample() {
    let w = Wscst::parse(Wscst::sample_pcl()).unwrap();
    let job = PrintJob {
        page_width: 80,
        copies: 3,
        duplex: Duplex::Yes,
        ..PrintJob::default()
    };
    run("wscst-sample-80x66", &Transform::Custom(Box::new(w)), &job, 80, 66);
}

#[test]
fn pdf_is_the_capture_emitter() {
    let job = PrintJob::default();
    let out = Transform::Pdf.apply(&job, &report(132, 66));
    let pages = spool::pdf::check(&out.data).unwrap();
    assert_eq!(pages, 2);
}
