// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Long operations that pause ([`db::Pause`]): a path built, records
//! converted for a new format, a rollback undone and the copy and
//! reorganize commands, each committing what it did so far at its
//! pauses, give what they give without pauses; and one ended part way
//! (a crash after a pause) leaves nothing a rebuild or the IPL's
//! recovery does not put right.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use std::ops::Bound;

use common::{customer_def, fld, key, lf, name, open_new, pf, rec, record, reopen};
use db::cmt::LckLvl;
use db::file::{self, CreateFileOpts, MemberOpt};
use db::journal::{recover_all, strjrnpf};
use db::odp::{self, OpenOpts, Session, Usage, session::resolve_in};
use db::{DbError, Member};
use dds::{FileRef, MapRefs};
use jrn::journal::{CrtJrnOpts, crtjrn};
use jrn::rcv::{CrtJrnRcvOpts, crtjrnrcv};
use qsname::MemberSpec;
use store::Store;
use store::obj::{JobHandle, ObjectId};

const NOW: u64 = 1_767_225_600;
const ROWS: i64 = 700;

/// CUST with `ROWS` customers, named in reverse of their numbers.
fn customers(store: &mut Store, lib: ObjectId) -> ObjectId {
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(&mut t, lib, name("CUST"), &customer_def(), &CreateFileOpts { now: NOW, ..CreateFileOpts::default() })
        .unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    for n in 1..=ROWS {
        m.write_new(&mut t, &record(n, &format!("N{:05}", ROWS - n)), &[0]).unwrap();
    }
    t.commit().unwrap();
    cust
}

/// A pause that commits what was done so far and counts; past `stop`
/// pauses it ends the operation.
fn pausing(count: &mut u32, stop: u32) -> impl FnMut(&mut store::Txn<'_>) -> db::Result<bool> + '_ {
    move |t| {
        t.commit_part()?;
        *count += 1;
        if *count > stop {
            return Err(DbError::Ended);
        }
        Ok(true)
    }
}

fn by_name(t: &mut store::Txn<'_>, lfid: ObjectId) -> Vec<u64> {
    let lm = Member::open_by(t, lfid, MemberSpec::First).unwrap();
    lm.keys(t, Bound::Unbounded, Bound::Unbounded, 100_000).unwrap().iter().map(|k| k.rrn).collect()
}

#[test]
fn a_path_built_with_pauses_and_one_ended_part_way() {
    let (d, mut store, lib) = open_new();
    let cust = customers(&mut store, lib);
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let def = lf(&[rec("CUSBYNAME", "PFILE(QSTEST/CUST)"), key("NAME", "")], &refs);
    let want: Vec<u64> = (1..=ROWS).rev().map(|n| u64::try_from(n).unwrap()).collect();
    let opts = CreateFileOpts { now: NOW, member: MemberOpt::None, ..CreateFileOpts::default() };
    // Built with a pause every batch, each committing the part before.
    let mut t = store.begin().unwrap();
    let lfid = file::create_lf(&mut t, lib, name("BYNAME"), &def, &mut |_| Ok(cust), &opts).unwrap();
    let mut pauses = 0;
    file::add_lf_member_paused(&mut t, lfid, name("BYNAME"), "", None, NOW, &mut pausing(&mut pauses, u32::MAX)).unwrap();
    t.commit().unwrap();
    assert_eq!(pauses, 2, "{ROWS} records, a pause every 256");
    let mut t = store.begin().unwrap();
    assert_eq!(by_name(&mut t, lfid), want);
    drop(t);
    // Ended at its second pause, the rest never written: what the first
    // committed is a member whose path is marked invalid, built again
    // when it is next opened.
    let mut t = store.begin().unwrap();
    let lf2 = file::create_lf(&mut t, lib, name("BYNAME2"), &def, &mut |_| Ok(cust), &opts).unwrap();
    let mut pauses = 0;
    let e = file::add_lf_member_paused(&mut t, lf2, name("BYNAME2"), "", None, NOW, &mut pausing(&mut pauses, 1)).unwrap_err();
    assert!(matches!(e, DbError::Ended), "{e:?}");
    drop(t);
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let mut lm = Member::open_by(&mut t, lf2, MemberSpec::First).unwrap();
    assert!(lm.attr().path_invalid);
    assert!(lm.ensure_path(&mut t, &mut |_, _| {}).unwrap());
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    assert_eq!(by_name(&mut t, lf2), want);
}

/// A path over many records: its load pauses too, and one ended in the
/// load is rebuilt whole.
#[test]
fn a_large_path_is_loaded_in_steps() {
    let (d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(&mut t, lib, name("CUST"), &customer_def(), &CreateFileOpts { now: NOW, ..CreateFileOpts::default() })
        .unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let big: i64 = 12_000;
    for n in 1..=big {
        m.write_new(&mut t, &record(n, &format!("N{:06}", big - n)), &[0]).unwrap();
    }
    t.commit().unwrap();
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let def = lf(&[rec("CUSBYNAME", "PFILE(QSTEST/CUST)"), key("NAME", "")], &refs);
    let want: Vec<u64> = (1..=big).rev().map(|n| u64::try_from(n).unwrap()).collect();
    let opts = CreateFileOpts { now: NOW, member: MemberOpt::None, ..CreateFileOpts::default() };
    let read_pauses = u32::try_from(big).unwrap() / 256;
    // Whole, with pauses in the reading and in the load.
    let mut t = store.begin().unwrap();
    let lfid = file::create_lf(&mut t, lib, name("BYNAME"), &def, &mut |_| Ok(cust), &opts).unwrap();
    let mut pauses = 0;
    file::add_lf_member_paused(&mut t, lfid, name("BYNAME"), "", None, NOW, &mut pausing(&mut pauses, u32::MAX)).unwrap();
    t.commit().unwrap();
    assert!(pauses >= read_pauses + 2, "{pauses} pauses: the load takes some");
    let mut t = store.begin().unwrap();
    assert_eq!(by_name(&mut t, lfid), want);
    drop(t);
    // Ended at a pause in the load, the tree part loaded and committed:
    // rebuilt whole when next opened.
    let mut t = store.begin().unwrap();
    let lf2 = file::create_lf(&mut t, lib, name("BYNAME2"), &def, &mut |_| Ok(cust), &opts).unwrap();
    let mut n = 0;
    let e = file::add_lf_member_paused(&mut t, lf2, name("BYNAME2"), "", None, NOW, &mut pausing(&mut n, read_pauses + 1)).unwrap_err();
    assert!(matches!(e, DbError::Ended), "{e:?}");
    drop(t);
    drop(store);
    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let mut lm = Member::open_by(&mut t, lf2, MemberSpec::First).unwrap();
    assert!(lm.attr().path_invalid);
    assert!(lm.ensure_path(&mut t, &mut |_, _| {}).unwrap());
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    assert_eq!(by_name(&mut t, lf2), want);
}

#[test]
fn records_converted_with_pauses() {
    let (_d, mut store, lib) = open_new();
    let cust = customers(&mut store, lib);
    // NAME widened, BAL dropped, a column added: every record converted.
    let new = pf(&[
        rec("CUSREC", ""),
        fld("CUSNO", "7", "P", "0", ""),
        fld("NAME", "40", "A", "", ""),
        fld("EXTRA", "5", "A", "", ""),
    ]);
    let mut t = store.begin().unwrap();
    let mut pauses = 0;
    db::chg::chgpf_srcfile_paused(&mut t, JobHandle(1), cust, &new, &mut pausing(&mut pauses, u32::MAX)).unwrap();
    t.commit().unwrap();
    assert_eq!(pauses, 2);
    let mut t = store.begin().unwrap();
    let m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    let fmt = m.format().clone();
    assert_eq!(m.attr().record_count, u64::try_from(ROWS).unwrap());
    let (_, nf) = fmt.format.field(&name("NAME")).unwrap();
    let rows = m.scan(&mut t, 1, 10_000).unwrap();
    let names: Vec<String> = rows.iter().map(|s| ccsid::decode_trimmed(db::record::field_bytes(nf, &s.record))).collect();
    let want: Vec<String> = (1..=ROWS).map(|n| format!("N{:05}", ROWS - n)).collect();
    assert_eq!(names, want);
}

/// Names of a member's records in the field `NAME`, by RRN.
fn names_of(store: &mut store::Store, file: ObjectId) -> Vec<String> {
    let mut t = store.begin().unwrap();
    let m = Member::open_by(&mut t, file, MemberSpec::First).unwrap();
    let fmt = m.format().clone();
    let (_, nf) = fmt.format.field(&name("NAME")).unwrap();
    m.scan(&mut t, 1, 100_000).unwrap().iter().map(|s| ccsid::decode_trimmed(db::record::field_bytes(nf, &s.record))).collect()
}

#[test]
fn a_rebuild_cut_short_loses_nothing() {
    let (d, mut store, lib) = open_new();
    let cust = customers(&mut store, lib);
    let want: Vec<String> = (1..=ROWS).map(|n| format!("N{:05}", ROWS - n)).collect();
    let new = pf(&[
        rec("CUSREC", ""),
        fld("CUSNO", "7", "P", "0", ""),
        fld("NAME", "40", "A", "", ""),
        fld("EXTRA", "5", "A", "", ""),
    ]);
    let level = |store: &mut store::Store| {
        let mut t = store.begin().unwrap();
        file::describe(&mut t, cust).unwrap().formats[0].level_id.clone()
    };
    let before = level(&mut store);
    let member = |store: &mut store::Store| {
        let mut t = store.begin().unwrap();
        file::describe(&mut t, cust).unwrap().members[0].object
    };
    let old_member = member(&mut store);
    // A crash at its second pause, one part committed: the process goes
    // (a panic here, the transaction dropped with the rest).
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut t = store.begin().unwrap();
        let mut n = 0;
        let _ = db::chg::chgpf_srcfile_paused(&mut t, JobHandle(1), cust, &new, &mut |t| {
            n += 1;
            assert!(n < 2, "the crash");
            t.commit_part()?;
            Ok(true)
        });
    }));
    assert!(crashed.is_err());
    drop(store);
    let mut store = reopen(&d);
    // The records and format are as they were; the member names what the
    // rebuild made, deleted by the next one.
    assert_eq!(level(&mut store), before);
    assert_eq!(member(&mut store), old_member);
    assert_eq!(names_of(&mut store, cust), want);
    let shadow = {
        let mut t = store.begin().unwrap();
        db::member::Core::load(&mut t, old_member).unwrap().attr.rebuild
    };
    assert_ne!(shadow, ObjectId::NONE);
    // Ended without a crash: what it made is gone at once.
    let mut t = store.begin().unwrap();
    let mut pauses = 0;
    let e = db::chg::chgpf_srcfile_paused(&mut t, JobHandle(1), cust, &new, &mut pausing(&mut pauses, 1)).unwrap_err();
    assert!(matches!(e, DbError::Ended), "{e:?}");
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    assert!(store::obj::ops::read_header(&mut t, shadow).is_err(), "the crash's rebuild is deleted");
    assert_eq!(db::member::Core::load(&mut t, old_member).unwrap().attr.rebuild, ObjectId::NONE);
    drop(t);
    assert_eq!(names_of(&mut store, cust), want);
    // Whole: the new member in the old one's place, in the new format.
    let mut t = store.begin().unwrap();
    let mut pauses = 0;
    db::chg::chgpf_srcfile_paused(&mut t, JobHandle(1), cust, &new, &mut pausing(&mut pauses, u32::MAX)).unwrap();
    t.commit().unwrap();
    assert_ne!(level(&mut store), before);
    assert_ne!(member(&mut store), old_member);
    assert_eq!(names_of(&mut store, cust), want);
    let mut t = store.begin().unwrap();
    assert!(store::obj::ops::read_header(&mut t, old_member).is_err(), "the old member is deleted");
}

#[test]
fn a_rollback_ended_part_way_is_finished_by_recovery() {
    let (d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let cust = file::create_pf(&mut t, lib, name("CUST"), &customer_def(), &CreateFileOpts { now: NOW, ..CreateFileOpts::default() })
        .unwrap();
    let rcv = crtjrnrcv(&mut t, lib, name("QSTRCV001"), &CrtJrnRcvOpts { now: NOW, ..CrtJrnRcvOpts::default() }).unwrap();
    let jid = crtjrn(&mut t, lib, name("QSTJRN"), rcv, &CrtJrnOpts { now: NOW, ..CrtJrnOpts::default() }).unwrap();
    strjrnpf(&mut t, JobHandle(1), cust, jid, true, true, NOW).unwrap();
    t.commit().unwrap();
    let count = |store: &mut Store| {
        let mut t = store.begin().unwrap();
        Member::open_by(&mut t, cust, MemberSpec::First).unwrap().attr().record_count
    };
    for stop in [u32::MAX, 1] {
        // A cycle of ROWS inserts, then its rollback with pauses.
        let mut t = store.begin().unwrap();
        let mut s = Session::new(JobHandle(1), odp::new_shared());
        s.strcmtctl(&mut t, LckLvl::Chg, None, NOW).unwrap();
        let mut resolve = resolve_in(name("QSTEST"), vec![(name("CUST"), cust)]);
        let opts = OpenOpts { usage: Usage::ALL, commit: true, ..OpenOpts::input(JobHandle(0)) };
        let o = s.open(&mut t, &FileRef::parse("CUST").unwrap(), &mut resolve, opts).unwrap();
        for n in 1..=ROWS {
            o.borrow_mut().write(&mut t, &record(n, "New"), &[0]).unwrap();
        }
        t.commit().unwrap();
        let mut t = store.begin().unwrap();
        let mut pauses = 0;
        let r = s.rollbck_paused(&mut t, Some(NOW), &mut pausing(&mut pauses, stop));
        if stop == u32::MAX {
            assert_eq!(r.unwrap().applied, u64::try_from(ROWS).unwrap());
            t.commit().unwrap();
            assert_eq!(pauses, 2);
            assert_eq!(count(&mut store), 0);
            drop(o);
            s.end(&mut store.begin().unwrap());
            continue;
        }
        // Ended at the second pause: the first part's undoing is
        // committed, the rest is not; the job goes away (a crash).
        assert!(matches!(r, Err(DbError::Ended)));
        drop(t);
        drop(o);
        drop(s);
        drop(store);
        store = reopen(&d);
        let left = count(&mut store);
        assert!(left > 0 && left < u64::try_from(ROWS).unwrap(), "{left} part undone");
        let mut t = store.begin().unwrap();
        let qsys = file::describe(&mut t, cust).unwrap().header.library;
        let qsys = store::obj::ops::read_header(&mut t, qsys).unwrap().1.library;
        let r = recover_all(&mut t, qsys, NOW + 100).unwrap();
        assert_eq!(r.cycles_rolled_back, 1);
        t.commit().unwrap();
        assert_eq!(count(&mut store), 0, "the rest undone at recovery");
    }
}

#[test]
fn a_unique_path_over_repeated_keys_is_not_built() {
    let (_d, mut store, lib) = open_new();
    let cust = customers(&mut store, lib);
    // Two customers with one name.
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    m.write_new(&mut t, &record(ROWS + 1, "N00007"), &[0]).unwrap();
    t.commit().unwrap();
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "CUST", &customer_def());
    let def = lf(&[common::file_kw("UNIQUE"), rec("CUSBYNAME", "PFILE(QSTEST/CUST)"), key("NAME", "")], &refs);
    let opts = CreateFileOpts { now: NOW, ..CreateFileOpts::default() };
    // CRTLF: refused with the duplicate, nothing made.
    let mut t = store.begin().unwrap();
    let e = file::create_lf(&mut t, lib, name("BYNAME"), &def, &mut |_| Ok(cust), &opts).unwrap_err();
    assert!(matches!(e, DbError::DuplicateKeys { .. }), "{e:?}");
    assert_eq!(e.msgid(), Some("CPF5090"));
    t.abort();
    // Built with pauses committing parts: the error comes at the end, the
    // path still marked invalid, for the caller to delete.
    let mut t = store.begin().unwrap();
    let parts = CreateFileOpts { now: NOW, member: MemberOpt::None, ..CreateFileOpts::default() };
    let lfid = file::create_lf(&mut t, lib, name("BYNAME"), &def, &mut |_| Ok(cust), &parts).unwrap();
    let mut pauses = 0;
    let e = file::add_lf_member_paused(&mut t, lfid, name("BYNAME"), "", None, NOW, &mut pausing(&mut pauses, u32::MAX)).unwrap_err();
    assert!(matches!(e, DbError::DuplicateKeys { .. }), "{e:?}");
    assert!(pauses >= 2);
    file::delete_file(&mut t, JobHandle(1), lfid).unwrap();
    t.commit().unwrap();
    // The same keys without the repeat build.
    let mut t = store.begin().unwrap();
    let mut m = Member::open_by(&mut t, cust, MemberSpec::First).unwrap();
    m.delete(&mut t, u64::try_from(ROWS + 1).unwrap()).unwrap();
    let lfid = file::create_lf(&mut t, lib, name("BYNAME"), &def, &mut |_| Ok(cust), &opts).unwrap();
    t.commit().unwrap();
    let mut t = store.begin().unwrap();
    assert_eq!(by_name(&mut t, lfid).len(), usize::try_from(ROWS).unwrap());
}

/// Every record of a member, by RRN.
fn slots_of(store: &mut Store, file: ObjectId) -> Vec<db::Slot> {
    let mut t = store.begin().unwrap();
    Member::open_by(&mut t, file, MemberSpec::First).unwrap().scan(&mut t, 1, 100_000).unwrap()
}

#[test]
fn copies_and_a_reorganize_with_pauses() {
    use db::util::{self, CpyfOpts, ImpfOpts, KeyFile, MbrOpt, Target};
    let job = JobHandle(1);
    let mut without = Vec::new();
    for paused in [false, true] {
        let (_d, mut store, lib) = open_new();
        let cust = customers(&mut store, lib);
        let mut pauses = 0;
        let mut counting = pausing(&mut pauses, u32::MAX);
        let mut none = db::no_pause;
        let pause: db::Pause<'_> = if paused { &mut counting } else { &mut none };
        // CPYF into a new file, then again over it with *REPLACE.
        let mut t = store.begin().unwrap();
        let opts = CpyfOpts { crtfile: true, now: NOW, ..CpyfOpts::default() };
        let to = Target::Create { library: lib, name: name("COPY") };
        let r = util::cpyf_paused(&mut t, job, cust, &to, &opts, pause).unwrap();
        assert_eq!(r.copied, u64::try_from(ROWS).unwrap());
        let copy = r.created.unwrap();
        let opts = CpyfOpts { mbropt: MbrOpt::Replace, now: NOW, ..CpyfOpts::default() };
        let r = util::cpyf_paused(&mut t, job, cust, &Target::File(copy), &opts, pause).unwrap();
        assert_eq!(r.copied, u64::try_from(ROWS).unwrap());
        t.commit().unwrap();
        assert_eq!(slots_of(&mut store, copy), slots_of(&mut store, cust));
        // Out as text and back in.
        let mut t = store.begin().unwrap();
        let text = util::cpytoimpf_paused(&mut t, job, cust, MemberSpec::First, &ImpfOpts::default(), pause).unwrap();
        let back = file::create_pf(&mut t, lib, name("BACK"), &customer_def(), &CreateFileOpts { now: NOW, ..CreateFileOpts::default() })
            .unwrap();
        let r = util::cpyfrmimpf_paused(&mut t, job, back, MemberSpec::First, &text, &ImpfOpts::default(), pause).unwrap();
        assert_eq!((r.records, r.errors), (u64::try_from(ROWS).unwrap(), 0));
        t.commit().unwrap();
        assert_eq!(slots_of(&mut store, back), slots_of(&mut store, cust));
        // Every third record deleted, then the rest squeezed together in
        // key order.
        let mut t = store.begin().unwrap();
        let mut m = Member::open_by(&mut t, copy, MemberSpec::First).unwrap();
        for rrn in (3..=u64::try_from(ROWS).unwrap()).step_by(3) {
            m.delete(&mut t, rrn).unwrap();
        }
        let r = util::rgzpfm_paused(&mut t, job, copy, MemberSpec::First, &KeyFile::File, pause).unwrap();
        assert_eq!(r.records, u64::try_from(ROWS - ROWS / 3).unwrap());
        t.commit().unwrap();
        let left: Vec<String> = slots_of(&mut store, copy).iter().map(|s| format!("{:?}", s.record)).collect();
        assert_eq!(left.len(), usize::try_from(ROWS - ROWS / 3).unwrap());
        drop(counting);
        if paused {
            assert!(pauses >= 10, "{pauses} pauses");
            assert_eq!(left, without, "the same records in the same order");
        } else {
            without = left;
        }
    }
}
