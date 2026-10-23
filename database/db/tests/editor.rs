// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A source member as SEU reads and writes it: whole, with each
//! record's own sequence number and date, the member added when new.

mod common;

use common::{name, open_new};
use db::file::{self, CreateFileOpts};
use db::source::{read_member, write_member};
use qsname::MemberSpec;
use store::obj::JobHandle;
use store::txn::Txn;

const JOB: JobHandle = JobHandle(1);
const NOW: u64 = 1_767_225_600;

#[test]
fn read_and_write_whole_members() {
    let (_d, mut store, lib) = open_new();
    let mut t: Txn<'_> = store.begin().unwrap();
    let src = file::create_srcpf(
        &mut t,
        lib,
        name("QCLSRC"),
        92,
        &CreateFileOpts {
            now: NOW,
            max_members: 0,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    let lines = vec![
        (100, 260_101, "             PGM".to_string()),
        (150, 261_023, "             DSPLIB".to_string()),
        (300, 0, "             ENDPGM".to_string()),
    ];
    let n = write_member(&mut t, JOB, src, name("HELLO"), &lines, Some(name("CLLE")), "Hello", NOW).unwrap();
    assert_eq!(n, 3);
    let m = read_member(&mut t, src, MemberSpec::Name(name("HELLO"))).unwrap();
    assert_eq!(m.srctype, "CLLE");
    assert_eq!(m.text, "Hello");
    assert_eq!(m.width, 80);
    assert_eq!(m.records, lines);
    // Written again: replaced, the numbers kept as given.
    let fewer = vec![(500, 261_024, "PGM".to_string())];
    write_member(&mut t, JOB, src, name("HELLO"), &fewer, Some(name("CLLE")), "Again", NOW).unwrap();
    let m = read_member(&mut t, src, MemberSpec::Name(name("HELLO"))).unwrap();
    assert_eq!(m.records, fewer);
    assert_eq!(m.text, "Again");
    assert!(t.store().locks().holders(src).is_empty());
    assert!(read_member(&mut t, src, MemberSpec::Name(name("NONE"))).is_err());
    t.commit().unwrap();
}
