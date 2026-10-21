// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The I/O instructions over a store: open, keyed and relative reads,
//! writes, updates, deletes, the status codes and the information
//! data structure; data areas and data queues; system pointers.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_collect
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use db::odp::{KeyArg, Odp, OpenOpts};
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::exec::Program;
use mi::io::store::StoreIo;
use mi::io::{DataAreaValue, MemData};
use mi::{Job, MemSource};
use qsname::{MemberSpec, Name};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

fn rec(name: &str) -> String {
    at(&[(6, "A"), (17, "R"), (19, name)])
}

fn key(name: &str) -> String {
    at(&[(6, "A"), (17, "K"), (19, name)])
}

fn fld(name: &str, len: &str, ty: &str, dec: &str) -> String {
    at(&[
        (6, "A"),
        (19, name),
        (35 - len.len(), len),
        (35, ty),
        (38 - dec.len(), dec),
    ])
}

/// `CUSNO packed(7,0), NAME char(30), BAL packed(9,2)`: 39 bytes.
fn record(cusno: i64, who: &str, bal: &str) -> Vec<u8> {
    let mut r = Vec::new();
    r.extend(
        qdecimal::Packed::new(7, 0)
            .unwrap()
            .encode(
                &qdecimal::Decimal::new(i128::from(cusno), 0).unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    let mut nm = vec![0x40u8; 30];
    ccsid::encode_fixed(who, &mut nm).unwrap();
    r.extend(nm);
    r.extend(
        qdecimal::Packed::new(9, 2)
            .unwrap()
            .encode(
                &qdecimal::Decimal::parse(bal).unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    r
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

/// A store with `QGPL/CUST` holding 1 Alice, 3 Carol, 3 Bob, 7 Grace,
/// 10 Judy, keyed on `CUSNO` then `NAME`.
fn customers() -> (Rc<RefCell<Store>>, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    let lines = [
        rec("CUSREC"),
        fld("CUSNO", "7", "P", "0"),
        fld("NAME", "30", "A", ""),
        fld("BAL", "9", "P", "2"),
        key("CUSNO"),
        key("NAME"),
    ];
    let (source, _) = Source::from_text(&lines.join("\n"));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let def = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let id = db::file::create_pf(
        &mut t,
        lib,
        name("CUST"),
        &def,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let mut m = db::Member::open_by(&mut t, id, MemberSpec::First).unwrap();
    for (n, w) in [
        (1, "Alice"),
        (3, "Carol"),
        (3, "Bob"),
        (7, "Grace"),
        (10, "Judy"),
    ] {
        m.write_new(&mut t, &record(n, w, "12.50"), &[]).unwrap();
    }
    t.commit().unwrap();
    (Rc::new(RefCell::new(store)), id)
}

fn attach(job: &mut Job, store: &Rc<RefCell<Store>>, cust: ObjectId) {
    let resolve = db::odp::session::resolve_in(name("QGPL"), vec![(name("CUST"), cust)]);
    job.io = Some(Box::new(StoreIo::new(
        Box::new(Rc::clone(store)),
        JobHandle(1),
        Box::new(resolve),
    )));
}

fn who(rec: &[u8]) -> String {
    ccsid::decode_trimmed(&rec[4..34])
}

#[test]
fn records_are_read_written_updated_and_deleted() {
    let (store, cust) = customers();
    let text = format!(
        "
module IOT
file CUST file(CUST) lib(QGPL) usage(*ALL) keyed fmt(CUSREC)
static $rec: char(39)
static $infds: char(410)
static $k3: packed(7,0) inz(3)
static $k9: packed(7,0) inz(9)
static $new: char(39) inz(x\"{}\")
static $new2: char(39) inz(x\"{}\")
static $nm: char(30)
proc IOT() entry {{
L0:
  debug.stmt 10
  %o = dbopen CUST infds($infds)
  dsply %o
  %c = dbio CUST CHAIN rec($rec) key($k3)
  dsply %c
  str.subst $nm, $rec, 5, 30
  dsply $nm
  %e = dbio CUST READE rec($rec) key($k3)
  dsply %e
  str.subst $nm, $rec, 5, 30
  dsply $nm
  %e2 = dbio CUST READE rec($rec) key($k3)
  dsply %e2
  %r = dbio CUST READ rec($rec)
  dsply %r
  str.subst $nm, $rec, 5, 30
  dsply $nm
  %n = dbio CUST CHAIN rec($rec) key($k9)
  dsply %n
  %w = dbio CUST WRITE rec($new)
  dsply %w
  %w2 = dbio CUST WRITE rec($new)
  dsply %w2
  %c2 = dbio CUST CHAIN rec($rec) key($k9)
  dsply %c2
  %u = dbio CUST UPDATE rec($new2)
  dsply %u
  %d = dbio CUST DELETE
  dsply %d
  debug.stmt 11
  %rr = dbio CUST CHAIN rec($rec) rrn(1)
  dsply %rr
  str.subst $nm, $rec, 5, 30
  dsply $nm
  %x = dbclose CUST
  dsply %x
  %z = dbio CUST READ rec($rec)
  dsply %z
  ret
}}
",
        hex(&record(9, "Zed", "1.00")),
        hex(&record(9, "Zoe", "2.00")),
    );
    let mut p = Program::from_text(&text).unwrap_or_else(|e| panic!("{e}"));
    attach(p.job(), &store, cust);
    p.call_entry().unwrap();
    let j = p.job_ref();
    assert!(!j.pending(), "{:?}", j.exception);
    let console: Vec<&str> = j.console.iter().map(String::as_str).collect();
    let statuses: Vec<&str> = console
        .iter()
        .copied()
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
        .collect();
    assert_eq!(
        statuses,
        vec![
            "0", "0", "0", "11", "0", "12", "0", "0", "0", "0", "1221", "0", "0", "1211"
        ],
        "{console:?}"
    );
    let names: Vec<&str> = console
        .iter()
        .copied()
        .filter(|s| s.chars().any(char::is_alphabetic))
        .collect();
    assert_eq!(
        names,
        vec!["Bob", "Carol", "Grace", "Alice"],
        "keyed order is CUSNO then NAME"
    );
    let rec = p.static_bytes("rec").unwrap();
    assert_eq!(
        who(&rec),
        "Alice",
        "the last CHAIN by relative record number"
    );
    let infds = p.static_bytes("infds").unwrap();
    assert_eq!(ccsid::decode_trimmed(&infds[0..8]), "CUST");
    assert_eq!(ccsid::decode_trimmed(&infds[8..9]), "0", "closed");
    assert_eq!(ccsid::decode_trimmed(&infds[10..15]), "00000");
    assert_eq!(ccsid::decode_trimmed(&infds[15..21]), "CLOSE");
    assert_eq!(
        ccsid::decode_trimmed(&infds[29..37]),
        "11",
        "the statement of the close"
    );
    assert_eq!(ccsid::decode_trimmed(&infds[37..45]), "CUSREC");
    assert_eq!(ccsid::decode_trimmed(&infds[128..138]), "CUST", "member");

    // The store saw the write, the update and no delete.
    let mut s = store.borrow_mut();
    let mut t = s.begin().unwrap();
    let mut o = Odp::open(
        &mut t,
        db::odp::new_shared(),
        cust,
        OpenOpts::input(JobHandle(2)),
    )
    .unwrap();
    // Two records were written with key 9; the first became Zoe.
    for nm in ["Zed", "Zoe"] {
        let mut padded = vec![0x40u8; 30];
        ccsid::encode_fixed(nm, &mut padded).unwrap();
        let r = o
            .chain(
                &mut t,
                &KeyArg::Values(vec![
                    db::Value::Num(qdecimal::Decimal::new(9, 0).unwrap()),
                    db::Value::Char(padded),
                ]),
                false,
            )
            .unwrap()
            .unwrap_or_else(|| panic!("{nm} is there"));
        assert_eq!(who(&r.data), nm);
    }
    o.close(&mut t);
    t.commit().unwrap();
}

#[test]
fn a_missing_file_escapes_at_open_and_closed_files_report_01211() {
    let (store, cust) = customers();
    let mut p = Program::from_text(
        "
module NOF
file NOPE file(NOPE) lib(QGPL) usage(*INPUT)
file CUST file(CUST) lib(QGPL) usage(*INPUT) keyed
static $rec: char(39)
proc NOF() entry {
L0:
  monitor M, CPF9801 -> GONE
  %o = dbopen NOPE
  dsply %o
  endmon M
  ret
GONE:
  dsply \"no file\"
  %r = dbio CUST READ rec($rec)
  dsply %r
  ret
}
",
    )
    .unwrap();
    attach(p.job(), &store, cust);
    p.call_entry().unwrap();
    let j = p.job_ref();
    assert_eq!(j.console, vec!["no file", "1211"]);
    assert!(!j.pending());
    assert_eq!(j.handled[0].msgid, "CPF9801");
}

#[test]
fn data_areas_and_data_queues() {
    let mut p = Program::from_text(
        "
module DTA
static $lda: char(1024)
static $n: packed(5,0)
static $q: char(10)
static $qn: char(10) inz(\"Q1\")
static $lib: char(10) inz(\"QGPL\")
proc DTA() entry {
L0:
  dtaara.lock \"*LDA\", \"*LIBL\"
  dtaara.read \"*LDA\", \"*LIBL\", $lda
  mov $lda, \"hello lda\"
  dtaara.write \"*LDA\", \"*LIBL\", $lda
  dtaara.unlock \"*LDA\", \"*LIBL\"
  dtaara.read \"COUNTER\", \"QGPL\", $n
  dsply $n
  dec.add $n, $n, 1 prec(5,0)
  dtaara.write \"COUNTER\", \"QGPL\", $n
  dtaq.snd $qn, $lib, \"abc\", 3
  %l = dtaq.rcv $qn, $lib, $q, 10, 0
  dsply %l
  dsply $q
  %l2 = dtaq.rcv $qn, $lib, $q, 10, 0
  dsply %l2
  monitor M, CPF1015 -> NOAREA
  dtaara.read \"NOSUCH\", \"QGPL\", $lda
  dsply \"not reached\"
  endmon M
  ret
NOAREA:
  dsply \"no area\"
  ret
}
",
    )
    .unwrap();
    let mut data = MemData::default();
    let mut counter = DataAreaValue::decimal(5, 0);
    counter.value = qdecimal::Packed::new(5, 0)
        .unwrap()
        .encode(
            &qdecimal::Decimal::new(41, 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap();
    data.add_area("QGPL", "COUNTER", counter);
    data.add_queue("QGPL", "Q1");
    p.job().data = Some(Box::new(data));
    p.call_entry().unwrap();
    let j = p.job_ref();
    assert_eq!(j.console, vec!["41", "3", "abc", "0", "no area"]);
    assert!(!j.pending());
    assert_eq!(
        p.static_bytes("n").unwrap(),
        vec![0x00, 0x04, 0x2C],
        "42 after the add"
    );
}

#[test]
fn system_pointers_resolve_and_call_programs() {
    let sub = "module SUB\nproc SUB() entry {\nL0:\n  dsply \"sub ran\"\n  ret\n}\n";
    let main = "module SPT\nstatic $name: char(10) inz(\"SUB\")\nproc SPT() entry {\nL0:\n  %p = rslvsp *PGM, $name, \"QGPL\"\n  callp %p()\n  monitor M, MCH3401 -> GONE\n  %q = rslvsp *PGM, \"NOSUCH\", \"QGPL\"\n  dsply \"not reached\"\n  endmon M\n  ret\nGONE:\n  dsply \"no object\"\n  ret\n}\n";
    let program = |text: &str| {
        let m = ModuleObject::compile(qir::parse(text).unwrap(), CreateData::default(), 0).unwrap();
        let mut opts = BindOptions::new(&m.name.clone(), PgmKind::Pgm);
        opts.attr.actgrp = ActGrpAttr::Caller;
        ile::bind(BindInput {
            modules: vec![(m, "QGPL".into())],
            bnddirs: Vec::new(),
            lookup: &|_| None,
            options: opts,
        })
        .unwrap()
        .0
    };
    let mut src = MemSource::default();
    src.add("QGPL", program(sub));
    src.add("QGPL", program(main));
    let mut job = Job::boxed(Box::new(src));
    job.call_program("SPT", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["sub ran", "no object"]);
    assert!(!job.pending());
}
