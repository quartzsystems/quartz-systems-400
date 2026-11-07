// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Trigger programs of record operations called in the job: the before
//! calls a store-backed source defers, the resumed operation with the
//! record a program changed, after calls, and the statuses 01023 and
//! 01024 of a program ending in an escape.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use db::trg::ops::NewTrg;
use db::trg::{Event, Time};
use dds::{CompileOpts, NoRefs, Source, compile_pf};
use mi::io::store::StoreIo;
use mi::io::trigger::{self, FAILED_AFTER, FAILED_BEFORE};
use mi::io::{IoOp, IoRequest, KeyRef, OpenSpec};
use mi::{Exception, Job, MemSource};
use qsname::{MemberSpec, Name};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

/// `QGPL/TRGF` (C1 char 5) with a before insert trigger allowed to change
/// the record and an after insert trigger, both `QSYS/TRGNAT`.
fn setup() -> (Rc<RefCell<Store>>, ObjectId) {
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
    let src = [
        "     A          R TRGREC".to_string(),
        "     A            C1             5A".to_string(),
    ]
    .join("\n");
    let (source, _) = Source::from_text(&src);
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let def = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let id = db::file::create_pf(
        &mut t,
        lib,
        name("TRGF"),
        &def,
        &CreateFileOpts {
            now: 1,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let pgm = (name("QSYS"), name("TRGNAT"));
    let b = NewTrg {
        name: Some("B".into()),
        alwrepchg: true,
        ..NewTrg::new(Time::Before, Event::Insert, pgm)
    };
    db::trg::ops::add(&mut t, id, &b, NOW).unwrap();
    let a = NewTrg {
        name: Some("A".into()),
        ..NewTrg::new(Time::After, Event::Insert, pgm)
    };
    db::trg::ops::add(&mut t, id, &a, NOW).unwrap();
    t.commit().unwrap();
    (Rc::new(RefCell::new(store)), id)
}

/// The native trigger program: keeps each buffer; a new record starting
/// X ends in CPF9898, one starting R is changed to start with Z, and on
/// an after call one starting Y ends in CPF9898.
fn native(job: &mut Job, log: &Rc<RefCell<Vec<Vec<u8>>>>) {
    let log = Rc::clone(log);
    job.natives.insert(
        "TRGNAT".into(),
        Rc::new(move |j: &mut Job, args: &[u64]| {
            let len =
                u32::from_be_bytes(mi::storage::view(args[1], 4).try_into().unwrap()) as usize;
            let buf = mi::storage::view(args[0], len);
            log.borrow_mut().push(buf.to_vec());
            let off = u32::from_be_bytes(buf[64..68].try_into().unwrap()) as usize;
            let after = buf[31] == 0xF1;
            match buf[off] {
                0xE7 => j.exception = Some(Exception::message("CPF9898", "QCPFMSG", Vec::new(), 0)),
                0xE8 if after => {
                    j.exception = Some(Exception::message("CPF9898", "QCPFMSG", Vec::new(), 0))
                }
                0xD9 => buf[off] = 0xE9,
                _ => {}
            }
        }),
    );
}

fn write(job: &mut Job, h: u32, text: &str) -> mi::io::IoResult {
    let rec = ccsid::to_ebcdic(text).unwrap();
    let req = IoRequest {
        op: IoOp::Write,
        record: Some(&rec),
        key: KeyRef::None,
        format: None,
        nolock: false,
        nulls: None,
        resume: None,
    };
    trigger::run(job, &req, &mut |j, r| j.io.as_mut().unwrap().io(h, r))
}

fn records(store: &Rc<RefCell<Store>>, f: ObjectId) -> Vec<String> {
    let mut s = store.borrow_mut();
    let mut t = s.begin().unwrap();
    let m = db::Member::open_by(&mut t, f, MemberSpec::First).unwrap();
    let out = m
        .scan(&mut t, 1, 100)
        .unwrap()
        .into_iter()
        .map(|r| ccsid::decode_trimmed(&r.record))
        .collect();
    t.abort();
    out
}

#[test]
fn trigger_programs_run_in_the_job() {
    let (store, f) = setup();
    let mut job = Job::boxed(Box::new(MemSource::default()));
    let resolve = db::odp::session::resolve_in(name("QGPL"), vec![(name("TRGF"), f)]);
    job.io = Some(Box::new(StoreIo::new(
        Box::new(Rc::clone(&store)),
        JobHandle(1),
        Box::new(resolve),
    )));
    let log = Rc::new(RefCell::new(Vec::new()));
    native(&mut job, &log);
    let spec = |triggers| OpenSpec {
        triggers,
        file: "TRGF".into(),
        lib: "QGPL".into(),
        usage: qir::FileUsage::All,
        keyed: false,
        lvlchk: None,
        member: None,
        actgrp: String::new(),
        commit: false,
    };
    let (h, _) = job.io.as_mut().unwrap().open(&spec(true)).unwrap();
    // Before, then the write with the changed record, then after.
    let r = write(&mut job, h, "Rabcd");
    assert_eq!(r.status, 0, "{r:?}");
    assert_eq!(log.borrow().len(), 2);
    assert_eq!(
        log.borrow()[1][128],
        0xE9,
        "the after call sees the record the before one left"
    );
    assert_eq!(records(&store, f), ["Zabcd"]);
    // A before refusal: 01023, nothing written.
    let r = write(&mut job, h, "Xnope");
    assert_eq!(
        (r.status, r.msgid.as_deref()),
        (FAILED_BEFORE, Some("CPF502B"))
    );
    assert_eq!(records(&store, f), ["Zabcd"]);
    // An after refusal: 01024, the record stays.
    let r = write(&mut job, h, "Yafter");
    assert_eq!(
        (r.status, r.msgid.as_deref()),
        (FAILED_AFTER, Some("CPF502B"))
    );
    assert_eq!(records(&store, f), ["Zabcd", "Yafte"]);
    // An opener that calls no trigger programs is refused.
    let (h2, _) = job.io.as_mut().unwrap().open(&spec(false)).unwrap();
    let r = write(&mut job, h2, "plain");
    assert_eq!(
        (r.status, r.msgid.as_deref()),
        (FAILED_BEFORE, Some("CPF502B"))
    );
    assert_eq!(records(&store, f).len(), 2);
}
