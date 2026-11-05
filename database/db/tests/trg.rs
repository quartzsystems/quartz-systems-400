// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Triggers: adding, replacing, removing and disabling them as the
//! primary does, the trigger buffer byte for byte against the primary's
//! capture, and firing through the open data path in each mode.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::{fld, lf, name, open_new, pf, rec};
use db::file::{self, CreateFileOpts};
use db::odp::{self, Odp, OpenOpts, Usage, status};
use db::trg::fire::Resume;
use db::trg::ops::{self, NewTrg, Pick, Refusal};
use db::trg::{Call, Caller, Event, Failure, Mode, Time};
use db::DbError;
use dds::MapRefs;
use qsname::Name;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

/// The primary's buffer for `INSERT INTO TRGT VALUES ('AAA', 1.5, 'hi',
/// NULL)`, the before trigger (`oracle/trigger/p1.out`, entry 1).
const TRGT_BEFORE_INSERT: &str = concat!(
    "E3D9C7E3404040404040D8E2E3C5E2E340404040E3D9C7E3404040404040F1F2",
    "F000000000000025000000000000000000000060000000160000007600000004",
    "0000009000000016000000A60000000400000000000000000000000000000000",
    "4040404040404040404040404040404040404040404040404040404040404040",
    "40404040404040404040404040404040C1C1C100150F00028889404040404040",
    "404000000000F0F0F0F140404040404040404040404040404040404040404040",
);

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// TRGT as the probe made it: C1 CHAR(3) NOT NULL, N1 DECIMAL(5,2), V1
/// VARCHAR(10), I1 INTEGER.
fn trgt(t: &mut Txn<'_>, lib: ObjectId) -> ObjectId {
    let p = pf(&[
        rec("TRGT", ""),
        fld("C1", "3", "A", "", ""),
        fld("N1", "5", "P", "2", "ALWNULL"),
        fld("V1", "10", "A", "", "VARLEN ALWNULL"),
        fld("I1", "9", "B", "0", "ALWNULL"),
    ]);
    file::create_pf(t, lib, name("TRGT"), &p, &CreateFileOpts { now: 1, ..CreateFileOpts::default() }).unwrap()
}

/// A TRGT record: 'AAA'-like text, packed 1.50, 'hi', I1 null.
fn trgt_rec(c1: &str) -> (Vec<u8>, Vec<u8>) {
    let mut r = ccsid::to_ebcdic(c1).unwrap();
    r.extend([0x00, 0x15, 0x0F, 0x00, 0x02, 0x88, 0x89]);
    r.extend([0x40; 8]);
    r.extend([0, 0, 0, 0]);
    (r, vec![0b100])
}

fn pgm() -> (Name, Name) {
    (name("QSTEST"), name("TRGDUMP"))
}

/// A caller that keeps every buffer, refuses a new record starting X
/// and changes a leading R to Z, as the probe's CL program does.
#[derive(Default)]
struct Dump {
    seen: Vec<Call>,
}

impl Caller for Dump {
    fn call(&mut self, _txn: &mut Txn<'_>, call: &mut Call) -> Result<(), Failure> {
        self.seen.push(call.clone());
        let off = u32::from_be_bytes(call.buffer[64..68].try_into().unwrap()) as usize;
        let first = call.buffer.get(off).copied();
        // A new image is there unless the event has none (blanks).
        if call.event == Event::Insert || call.event == Event::Update {
            if first == Some(0xE7) {
                return Err(Failure { msgid: "CPF9898".into(), text: "refused".into(), not_found: false });
            }
            if first == Some(0xD9) {
                call.buffer[off] = 0xE9;
            }
        }
        Ok(())
    }
}

fn open(t: &mut Txn<'_>, locks: &odp::SharedLocks, file: ObjectId) -> Odp {
    let opts = OpenOpts { usage: Usage::ALL, keyed: false, ..OpenOpts::input(JobHandle(1)) };
    Odp::open(t, locks.clone(), file, opts).unwrap()
}

#[test]
fn adding_replacing_removing_and_disabling() {
    let (_d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let f = trgt(&mut t, lib);
    let a1 = NewTrg { name: Some("A1".into()), ..NewTrg::new(Time::After, Event::Insert, pgm()) };
    assert_eq!(ops::add(&mut t, f, &a1, 2).unwrap(), "A1");
    let e = ops::add(&mut t, f, &a1, 2).unwrap_err();
    assert!(matches!(e, DbError::Trigger { op: 1, reason: Refusal::Exists, .. }), "{e:?}");
    assert_eq!(e.msgid(), Some("CPF32C6"));
    let a2 = NewTrg { name: Some("A2".into()), on_change: true, ..NewTrg::new(Time::After, Event::Update, pgm()) };
    ops::add(&mut t, f, &a2, 2).unwrap();
    ops::add(&mut t, f, &NewTrg { replace: true, alwrepchg: true, ..a1.clone() }, 3).unwrap();
    let a3 = NewTrg { name: Some("A3".into()), ..NewTrg::new(Time::After, Event::Delete, pgm()) };
    ops::add(&mut t, f, &a3, 2).unwrap();
    // The primary's numbering: a replace keeps its number and uses one.
    let got: Vec<(String, u32)> = db::trg::list(&mut t, f).unwrap().into_iter().map(|t| (t.name, t.number)).collect();
    assert_eq!(got, [("A1".to_string(), 1), ("A2".into(), 2), ("A3".into(), 4)]);
    assert!(db::trg::list(&mut t, f).unwrap()[0].alwrepchg);
    // A generated name.
    let g = ops::add(&mut t, f, &NewTrg::new(Time::Before, Event::Delete, pgm()), 2).unwrap();
    assert_eq!(g, "QSYS_TRIG_QSTEST_____TRGT_______000001");
    // DSPFD TYPE(*TRG).
    let text = db::dsp::dspfd(&mut t, f).unwrap().render(db::dsp::FdType::Trg);
    assert!(text.contains("Number of trigger programs"), "{text}");
    assert!(text.contains("QSYS_TRIG_QSTEST_____TRGT_______000001"), "{text}");
    assert!(text.contains("*CHANGE"), "{text}");
    // CHGPFTRG and RMVPFTRG.
    assert_eq!(ops::set_state(&mut t, f, &Pick::When(None, None), false, 4).unwrap(), 4);
    assert!(db::trg::list(&mut t, f).unwrap().iter().all(|t| !t.enabled));
    let e = ops::set_state(&mut t, f, &Pick::Named("NOSUCH".into(), None), true, 4).unwrap_err();
    assert!(matches!(e, DbError::Trigger { op: 3, reason: Refusal::NotFound, .. }), "{e:?}");
    let gone = ops::remove(&mut t, f, &Pick::When(Some(Time::After), Some(Event::Insert)), 5).unwrap();
    assert_eq!(gone, ["A1"]);
    let e = ops::remove(&mut t, f, &Pick::Named("NOSUCH".into(), None), 5).unwrap_err();
    assert!(matches!(e, DbError::Trigger { op: 2, reason: Refusal::NotFound, .. }), "{e:?}");
    assert_eq!(ops::remove(&mut t, f, &Pick::When(None, None), 5).unwrap().len(), 3);
    let e = ops::remove(&mut t, f, &Pick::When(None, None), 5).unwrap_err();
    assert!(matches!(e, DbError::Trigger { op: 2, reason: Refusal::NoneThere, .. }), "{e:?}");
    t.commit().unwrap();
}

#[test]
fn the_buffer_is_the_primarys_and_triggers_fire_in_order() {
    let (_d, mut store, lib) = open_new();
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let f = trgt(&mut t, lib);
    let add = |t: &mut Txn<'_>, n: &str, time, event, alw| {
        let nt = NewTrg { name: Some(n.into()), alwrepchg: alw, ..NewTrg::new(time, event, pgm()) };
        ops::add(t, f, &nt, 2).unwrap();
    };
    add(&mut t, "BEFINS", Time::Before, Event::Insert, true);
    add(&mut t, "AFTINS", Time::After, Event::Insert, false);
    add(&mut t, "AFTUPD", Time::After, Event::Update, false);
    let chg = NewTrg { name: Some("AFTUPDC".into()), on_change: true, ..NewTrg::new(Time::After, Event::Update, pgm()) };
    ops::add(&mut t, f, &chg, 2).unwrap();
    add(&mut t, "AFTDLT", Time::After, Event::Delete, false);
    add(&mut t, "AFTRD", Time::After, Event::Read, false);
    t.commit().unwrap();

    let dump = Rc::new(RefCell::new(Dump::default()));
    let mut t = store.begin().unwrap();
    let mut o = open(&mut t, &locks, f);
    o.set_trigger_mode(Mode::Call(dump.clone()));
    let (r, n) = trgt_rec("AAA");
    assert_eq!(o.write(&mut t, &r, &n).unwrap(), 1);
    {
        let d = dump.borrow();
        assert_eq!(d.seen.len(), 2);
        assert_eq!(d.seen[0].buffer, hex(TRGT_BEFORE_INSERT), "before insert");
        assert_eq!(d.seen[0].trigger, "BEFINS");
        // The after one: time 1, record 1.
        assert_eq!(&d.seen[1].buffer[30..32], &[0xF1, 0xF1]);
        assert_eq!(&d.seen[1].buffer[40..44], &[0, 0, 0, 1]);
    }
    // ALWREPCHG: R becomes Z before the after trigger and the file see it.
    let (r, n) = trgt_rec("RRR");
    assert_eq!(o.write(&mut t, &r, &n).unwrap(), 2);
    assert_eq!(dump.borrow().seen[3].buffer[144], 0xE9);
    // A refusal: CPF502B, status 01023, nothing written.
    let (r, n) = trgt_rec("XXX");
    let e = o.write(&mut t, &r, &n).unwrap_err();
    assert!(matches!(&e, DbError::TriggerFailed { before: true, trigger, .. } if trigger == "BEFINS"), "{e:?}");
    assert_eq!(e.msgid(), Some("CPF502B"));
    assert_eq!(o.fb.status, status::TRIGGER_BEFORE);
    dump.borrow_mut().seen.clear();
    // Read for update: the read trigger; update to the same: AFTUPDC is not called.
    let got = o.chain(&mut t, &db::KeyArg::Rrn(2), true).unwrap().unwrap();
    assert_eq!(got.data[0], 0xE9);
    o.update(&mut t, &got.data, &got.null_map).unwrap();
    let names: Vec<String> = dump.borrow().seen.iter().map(|c| c.trigger.clone()).collect();
    assert_eq!(names, ["AFTRD", "AFTUPD"]);
    // Delete: the old record, no new one (blanks).
    dump.borrow_mut().seen.clear();
    o.chain(&mut t, &db::KeyArg::Rrn(1), true).unwrap().unwrap();
    o.delete(&mut t).unwrap();
    let d = dump.borrow();
    let del = d.seen.last().unwrap();
    assert_eq!(del.trigger, "AFTDLT");
    assert_eq!(&del.buffer[30..32], &[0xF2, 0xF1]);
    assert_eq!(&del.buffer[96..99], &[0xC1, 0xC1, 0xC1]);
    assert!(del.buffer[144..170].iter().all(|&b| b == 0x40));
    drop(d);
    o.close(&mut t);
    t.commit().unwrap();
}

#[test]
fn deferred_refused_and_through_a_logical_file() {
    let (_d, mut store, lib) = open_new();
    let locks = odp::new_shared();
    let mut t = store.begin().unwrap();
    let f = trgt(&mut t, lib);
    let b = NewTrg { name: Some("B".into()), alwrepchg: true, ..NewTrg::new(Time::Before, Event::Insert, pgm()) };
    ops::add(&mut t, f, &b, 2).unwrap();
    let a = NewTrg { name: Some("A".into()), ..NewTrg::new(Time::After, Event::Insert, pgm()) };
    ops::add(&mut t, f, &a, 2).unwrap();
    t.commit().unwrap();

    let mut t = store.begin().unwrap();
    // Refused: nothing written.
    let mut o = open(&mut t, &locks, f);
    let (r, n) = trgt_rec("AAA");
    let e = o.write(&mut t, &r, &n).unwrap_err();
    assert!(matches!(e, DbError::TriggerNotCallable { .. }), "{e:?}");
    assert_eq!(o.fb.status, status::TRIGGER_BEFORE);
    // Deferred: the before calls, then the write resumed with the buffer
    // the program left, then the after calls to make.
    o.set_trigger_mode(Mode::Defer);
    let DbError::TriggersDue { mut calls } = o.write(&mut t, &r, &n).unwrap_err() else {
        panic!("calls due")
    };
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].buffer, hex(TRGT_BEFORE_INSERT));
    calls[0].buffer[144] = 0xE9;
    o.resume_triggers(Resume { buffer: Some(calls[0].buffer.clone()), failed: None });
    assert_eq!(o.write(&mut t, &r, &n).unwrap(), 1);
    let due = o.take_due();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].trigger, "A");
    assert_eq!(due[0].buffer[144], 0xE9, "the changed record was written");
    // A failure reported back: the write fails, nothing written.
    let DbError::TriggersDue { .. } = o.write(&mut t, &r, &n).unwrap_err() else { panic!() };
    let fail = Failure { msgid: "CPF9898".into(), text: "refused".into(), not_found: false };
    o.resume_triggers(Resume { buffer: None, failed: Some(("B".into(), fail)) });
    let e = o.write(&mut t, &r, &n).unwrap_err();
    assert!(matches!(e, DbError::TriggerFailed { before: true, .. }), "{e:?}");
    o.close(&mut t);
    // Through a logical file: the physical file's triggers.
    let mut refs = MapRefs::default();
    refs.insert_pf("QSTEST", "TRGT", &pf(&[
        rec("TRGT", ""),
        fld("C1", "3", "A", "", ""),
        fld("N1", "5", "P", "2", "ALWNULL"),
        fld("V1", "10", "A", "", "VARLEN ALWNULL"),
        fld("I1", "9", "B", "0", "ALWNULL"),
    ]));
    let def = lf(&[rec("TRGT", "PFILE(QSTEST/TRGT)")], &refs);
    let opts = CreateFileOpts { now: 1, ..CreateFileOpts::default() };
    let l = file::create_lf(&mut t, lib, name("TRGL"), &def, &mut |_| Ok(f), &opts).unwrap();
    let dump = Rc::new(RefCell::new(Dump::default()));
    let mut o = open(&mut t, &locks, l);
    o.set_trigger_mode(Mode::Call(dump.clone()));
    let (r, n) = trgt_rec("RRR");
    o.write(&mut t, &r, &n).unwrap();
    let d = dump.borrow();
    assert_eq!(d.seen.len(), 2);
    assert_eq!(&d.seen[0].buffer[0..4], &[0xE3, 0xD9, 0xC7, 0xE3], "the physical file's name");
    assert_eq!(d.seen[1].buffer[144], 0xE9);
    drop(d);
    o.close(&mut t);
    t.commit().unwrap();
}
