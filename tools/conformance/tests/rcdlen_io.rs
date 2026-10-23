// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A program opens a program-described file (`CRTPF RCDLEN`) for update
//! and reads its record through the store-backed I/O, as the QEMU
//! suite's locker and checker programs do.

use conformance::qir::{Case, customers, program};
use db::file::CreateFileOpts;
use mi::io::store::StoreIo;
use mi::{Job, MemSource};
use qsname::{MemberSpec, Name};
use store::obj::JobHandle;

const CHECKER: &str = "\
module CHECKER
file F file(WMLCK) lib(QGPL) usage(*UPDATE)
static $rec: char(10)
proc CHECKER() entry {
L0:
  %o = dbopen F
  dsply %o
  %r = dbio F READ rec($rec)
  dsply %r
  dsply $rec
  %c = dbclose F
  ret
}
";

#[test]
fn update_open_of_a_program_described_file() {
    let (store, cust) = customers().unwrap();
    let wmlck = {
        let mut s = store.borrow_mut();
        let mut t = s.begin().unwrap();
        let lib = store::obj::ops::read_header(&mut t, cust).unwrap().1.library;
        let id = db::file::create_pf_rcdlen(
            &mut t,
            lib,
            Name::parse("WMLCK").unwrap(),
            10,
            &CreateFileOpts {
                now: 1,
                ..CreateFileOpts::default()
            },
        )
        .unwrap();
        let mut m = db::Member::open_by(&mut t, id, MemberSpec::First).unwrap();
        m.write_new(&mut t, &ccsid::to_ebcdic("LOCKME    ").unwrap(), &[])
            .unwrap();
        t.commit().unwrap();
        id
    };
    let mut src = MemSource::default();
    src.add("QGPL", program(CHECKER).unwrap());
    let mut job = Job::boxed(Box::new(src));
    let resolve = db::odp::session::resolve_in(
        Name::parse("QGPL").unwrap(),
        vec![
            (Name::parse("CUST").unwrap(), cust),
            (Name::parse("WMLCK").unwrap(), wmlck),
        ],
    );
    job.io = Some(Box::new(StoreIo::new(
        Box::new(store),
        JobHandle(1),
        Box::new(resolve),
    )));
    let _ = Case::parse("CHK", CHECKER);
    // A debugger with nothing under debug (the job process keeps one)
    // does not guard production files.
    job.debug = Some(mi::debug::Debugger::default());
    let r = job.call_program("CHECKER", "QGPL", &[]);
    let console = job.console.join("\n");
    let raised: Vec<String> = job.raised.iter().map(|e| e.msgid.clone()).collect();
    assert!(
        r.is_ok() && raised.is_empty(),
        "call {r:?}; raised {raised:?}; console:\n{console}"
    );
    assert!(console.contains("LOCKME"), "console:\n{console}");
    // With a program under debug and UPDPROD(*NO), the update open is
    // refused with CPF4188.
    job.console.clear();
    job.raised.clear();
    if let Some(d) = job.debug.as_mut() {
        d.programs.push("QGPL/OTHER".into());
    }
    let _ = job.call_program("CHECKER", "QGPL", &[]);
    assert!(
        job.raised.iter().any(|e| e.msgid == "CPF4188"),
        "raised {:?}",
        job.raised.iter().map(|e| e.msgid.clone()).collect::<Vec<_>>()
    );
}
