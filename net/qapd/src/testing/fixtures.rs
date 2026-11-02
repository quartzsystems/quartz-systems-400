// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's printer output, made with QS/400's own SCS
//! encoder:
//!
//! - `QGPL/QPRINT`, 60 files, enough to page: `BOB`'s job log
//!   (`QPJOBLOG`, three pages: page 1 runs onto page 2), a 132-column
//!   listing of 30 pages (`QSYSPRT`), 27 pages whose page 26 starts by
//!   running past the page length (`OVERFLOW`), a landscape file
//!   (`LANDSCAPE`), one whose pitch and line density change part-way
//!   (`MIXED`), an AFP file (`AFPINV`, neither SCS nor
//!   readable), an open one (`OPENFILE`), one in every status, small
//!   reports of `BOB`'s, and `ANN`'s, which `BOB` may not touch;
//! - `QGPL/PRT01`, with writer `PRT01` attached: a file printing and
//!   one waiting on a message;
//! - `QUSRSYS/PRIVATE`, which only `ANN` may use;
//! - libraries `QGPL`, `QUSRSYS`, `QSYS` and `SECLIB`, which `BOB` may
//!   not read.

use qap_proto::ops::spool::{DeviceType, SpoolStatus, SpooledFileId};
use qap_proto::types::{JobId, ObjectName};
use spool::scs::{CSP, Encoder, PAGE, SCD, SLD};

use super::EPOCH;
use super::spool::{File, Queue, Spool};

fn q(library: &str, name: &str) -> ObjectName {
    ObjectName {
        library: library.into(),
        name: name.into(),
    }
}

fn job(name: &str, user: &str, number: u32) -> JobId {
    JobId {
        name: name.into(),
        user: user.into(),
        number: format!("{number:06}"),
    }
}

fn text(e: &mut Encoder, s: &str) {
    let b: Vec<u8> = s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect();
    e.text(&b);
    e.nl();
}

/// Pages of lines in one page format, a form feed between pages.
pub fn scs(width: u16, len: u16, lpi: u8, cpi: u8, pages: &[Vec<String>]) -> Vec<u8> {
    let mut e = Encoder::default();
    e.header(width, len, lpi, cpi);
    for (i, p) in pages.iter().enumerate() {
        if i > 0 {
            e.ff();
        }
        for l in p {
            text(&mut e, l);
        }
    }
    e.out
}

/// A job log of three pages: page 1 runs past 66 lines onto page 2,
/// then a form feed starts page 3.
fn joblog() -> Vec<u8> {
    let head = |n: u32| {
        vec![
            format!("QS00SS1 V1R0M0                       Job Log                    QSHQ400   11/02/26  09:14:03   Page {n:>4}"),
            " Job name . . . . : QPADEV0001  User . . : BOB         Number . . . : 001234".to_string(),
            " Job description  : QDFTJOBD    Library  : QGPL".to_string(),
            "MSGID      TYPE                    SEV  DATE      TIME             FROM PGM    TO PGM".to_string(),
        ]
    };
    let mut p1 = head(1);
    for i in 0..40 {
        p1.push(format!("CPC2102    Completion              00   11/02/26  09:14:{:02}.{:06}  QLICRLIB    QCMD", i % 60, i * 7));
        p1.push(format!("                                     Message . . . . :   Library TEST{i:02} created."));
    }
    let mut p2 = head(2);
    p2.push("CPF2111    Escape                  40   11/02/26  09:15:00.000123  QLICRLIB    QCMD".into());
    p2.push("                                     Message . . . . :   Library TEST00 already exists.".into());
    scs(132, 66, 6, 10, &[p1, p2])
}

/// A 132-column listing of 30 pages.
fn listing() -> Vec<u8> {
    let pages: Vec<Vec<String>> = (1..=30)
        .map(|p| {
            let mut v = vec![format!("{:<118}Page {p:>8}", "QS/400 Test Listing  QSYSPRT  132 columns"), String::new()];
            for l in 1..=60 {
                let row = format!("{:06} {:<40} {:>12.2} {:>12} ", (p - 1) * 60 + l, format!("Item {p}-{l}"), f64::from(p * l) * 1.25, p + l);
                v.push(format!("{row:-<132}"));
            }
            v
        })
        .collect();
    scs(132, 66, 6, 10, &pages)
}

/// 27 pages: form feeds start pages 2 to 25 and 27, and page 25 runs
/// past the page length, so page 26, the first of a second batch of
/// 25, is an overflow page.
fn overflow() -> Vec<u8> {
    let mut e = Encoder::default();
    e.header(132, 66, 6, 10);
    for p in 1..=25 {
        if p > 1 {
            e.ff();
        }
        let lines = if p == 25 { 70 } else { 10 };
        for l in 1..=lines {
            text(&mut e, &format!("OVERFLOW PAGE {p} LINE {l}"));
        }
    }
    e.ff();
    text(&mut e, "OVERFLOW LAST PAGE");
    e.out
}

/// A landscape file: wider than it is long, 15 pitch, 8 lines per inch.
fn landscape() -> Vec<u8> {
    let pages: Vec<Vec<String>> = (1..=3)
        .map(|p| (1..=40).map(|l| format!("LANDSCAPE PAGE {p} LINE {l:02} {}", "=".repeat(170))).collect())
        .collect();
    scs(198, 51, 8, 15, &pages)
}

/// Pitch and line density changed part-way: 10 pitch, 6 lpi on pages 1
/// and 2; 15 pitch, 8 lpi from page 3, whose lines run onto page 4.
pub fn mixed() -> Vec<u8> {
    let mut e = Encoder::default();
    e.header(132, 40, 6, 10);
    for p in 1..=2 {
        for l in 1..=20 {
            text(&mut e, &format!("10 PITCH PAGE {p} LINE {l}"));
        }
        e.ff();
    }
    e.out.extend_from_slice(&[CSP, PAGE, 4, SCD, 0, 15]);
    e.out.extend_from_slice(&[CSP, SLD, 2, 9]);
    for l in 1..=60 {
        text(&mut e, &format!("15 PITCH LINE {l} {}", "*".repeat(100)));
    }
    e.out
}

/// One page of `lines`.
fn short(title: &str, lines: usize) -> Vec<u8> {
    let page: Vec<String> = std::iter::once(title.to_string())
        .chain((1..=lines).map(|i| format!("  {title} detail line {i}")))
        .collect();
    scs(132, 66, 6, 10, &[page])
}

/// The output queues and files.
pub fn install(s: &mut Spool) {
    s.libraries = vec!["QSYS".into(), "QGPL".into(), "QUSRSYS".into(), "SECLIB".into()];
    s.private_libs = vec!["SECLIB".into()];
    s.profiles = vec!["BOB".into(), "WARN".into(), "ANN".into()];
    s.queue(Queue {
        outq: q("QGPL", "QPRINT"),
        text: "Default printer output".into(),
        ..Queue::default()
    });
    s.queue(Queue {
        outq: q("QGPL", "PRT01"),
        text: "Printer PRT01".into(),
        writer: "PRT01".into(),
        ..Queue::default()
    });
    s.queue(Queue {
        outq: q("QUSRSYS", "PRIVATE"),
        text: "Private output".into(),
        users: vec!["ANN".into()],
        ..Queue::default()
    });
    let mut at = i64::try_from(EPOCH).unwrap_or(0) * 1000;
    let mut add = |s: &mut Spool, name: &str, user: &str, status: SpoolStatus, outq: ObjectName, device: DeviceType, data: Vec<u8>| {
        at += 60_000;
        s.file(File {
            id: SpooledFileId {
                job: job(if user == "BOB" { "QPADEV0001" } else { "QPADEV0002" }, user, if user == "BOB" { 1234 } else { 1235 }),
                file: name.into(),
                number: u32::try_from(s.files.len() + 1).unwrap_or(0),
            },
            status,
            held_from: SpoolStatus::Ready,
            outq,
            user_data: name.chars().take(10).collect(),
            form_type: "*STD".into(),
            device,
            pages: 2,
            copies: 1,
            created: at,
            data,
        });
    };
    let qprint = || q("QGPL", "QPRINT");
    let prt01 = || q("QGPL", "PRT01");
    let (rdy, scs_dev) = (SpoolStatus::Ready, DeviceType::Scs);
    add(s, "QPJOBLOG", "BOB", rdy, qprint(), scs_dev, joblog());
    add(s, "QSYSPRT", "BOB", rdy, qprint(), scs_dev, listing());
    add(s, "OVERFLOW", "BOB", rdy, qprint(), scs_dev, overflow());
    add(s, "LANDSCAPE", "BOB", rdy, qprint(), scs_dev, landscape());
    add(s, "MIXED", "BOB", rdy, qprint(), scs_dev, mixed());
    // AFP: structured fields, not SCS.
    let afp = [0x5A, 0x00, 0x10, 0xD3, 0xA8, 0xA8, 0x00, 0x00, 0x00].repeat(8);
    add(s, "AFPINV", "BOB", rdy, qprint(), DeviceType::Afp, afp);
    add(s, "OPENFILE", "BOB", SpoolStatus::Open, qprint(), scs_dev, short("OPENFILE", 5));
    // Every status.
    for (n, st) in SpoolStatus::ALL.into_iter().enumerate() {
        let name = format!("STS{n}");
        let outq = if matches!(st, SpoolStatus::Printing | SpoolStatus::Message) { prt01() } else { qprint() };
        add(s, &name, "BOB", st, outq, scs_dev, short(&name, 3));
    }
    // ANN's, on the same queue.
    for i in 1..=10 {
        add(s, &format!("ANNRPT{i:02}"), "ANN", rdy, qprint(), scs_dev, short("ANN REPORT", 4));
    }
    add(s, "ANNPRIV", "ANN", rdy, q("QUSRSYS", "PRIVATE"), scs_dev, short("ANN PRIVATE", 2));
    // BOB's small reports, to 60 on QPRINT.
    let mut i = 0;
    while s.queues[0].files.len() < 60 {
        i += 1;
        add(s, &format!("RPT{i:03}"), "BOB", rdy, qprint(), scs_dev, short("REPORT", 2 + i % 5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_brief_asks_for() {
        let mut s = Spool::default();
        install(&mut s);
        let files = |name: &str| s.files.values().find(|f| f.id.file == name).expect(name).clone();
        assert_eq!(s.queues[0].files.len(), 60, "QPRINT pages");
        assert_eq!(s.queues[1].writer, "PRT01");
        assert_eq!(files("QSYSPRT").info().pages, 30);
        assert_eq!(files("QPJOBLOG").info().pages, 3);
        // OVERFLOW's page 26 starts at a new line, not a form feed.
        let over = files("OVERFLOW").data;
        let idx = spool::scs::index(&over);
        assert_eq!(idx.len(), 27);
        assert_eq!(over[idx[25].offset], spool::scs::NL);
        assert_eq!(over[idx[24].offset], spool::scs::FF);
        let land = spool::scs::decode(&files("LANDSCAPE").data).1;
        assert!(land.width > land.len);
        let (_, mid) = spool::scs::decode(&files("MIXED").data);
        assert_eq!((mid.cpi, mid.lpi), (15, 8));
        assert_eq!(files("AFPINV").device, DeviceType::Afp);
        assert_eq!(files("OPENFILE").status, SpoolStatus::Open);
        for st in SpoolStatus::ALL {
            assert!(s.files.values().any(|f| f.status == st), "{st:?}");
        }
        assert!(s.files.values().any(|f| f.owner() == "ANN" && f.outq.name == "QPRINT"));
    }
}
