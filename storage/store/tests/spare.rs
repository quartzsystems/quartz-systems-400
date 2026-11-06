// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hot spares: start and stop, kept across an open, and taken
//! automatically in the place of a mirrored half or a parity member
//! that fails while running or is missing at open.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::asp::io::IoEvent;
use store::asp::parity::Level;
use store::asp::unit::UnitState;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{FormatParams, MemJournal, Refusal, Store, StoreConfig, StoreError, UnitEvent};

const WAL: u64 = 32;
const NOW: u64 = 1_767_225_600;

fn params(n: usize) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xA5; 16],
        unit_uuids: (0..n).map(|i| [u8::try_from(i).unwrap() + 1; 16]).collect(),
        unit_serials: (0..n).map(|i| format!("UNIT{i}")).collect(),
        wal_pages: WAL,
        now: NOW,
    }
}

fn config() -> StoreConfig {
    StoreConfig {
        cache_pages: 16,
        checkpoint_txns: 8,
        checkpoint_secs: 30,
    }
}

fn dev(pages: u64, seed: u64) -> Arc<MemBlockDev> {
    Arc::new(MemBlockDev::with_seed(pages, seed))
}

fn boxed(devs: &[&Arc<MemBlockDev>]) -> Vec<Box<dyn BlockDev>> {
    devs.iter()
        .map(|d| Box::new((*d).clone()) as Box<dyn BlockDev>)
        .collect()
}

fn add_spaces(store: &mut Store, qsys: ObjectId, from: usize, to: usize) {
    for i in from..to {
        let mut txn = store.begin().unwrap();
        let p = CreateParams {
            library: qsys,
            name: qsname::Name::parse(&format!("S{i}")).unwrap(),
            type_code: types::USRSPC,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        };
        let id = usrspc::create(&mut txn, &p, 8 * 4064, false, 0).unwrap();
        usrspc::write(&mut txn, id, 10, format!("space {i}").as_bytes()).unwrap();
        txn.commit().unwrap();
    }
}

fn read_marker(store: &mut Store, qsys: ObjectId, i: usize) -> String {
    let name = format!("S{i}");
    let mut txn = store.begin().unwrap();
    let id = ops::list(&mut txn, qsys, Some(types::USRSPC))
        .unwrap()
        .into_iter()
        .find(|(k, _)| k.name.to_string() == name)
        .map(|(_, id)| id)
        .unwrap();
    let bytes = usrspc::read(&mut txn, id, 10, 8).unwrap();
    txn.abort();
    String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_string()
}

fn assert_spaces(store: &mut Store, qsys: ObjectId, count: usize) {
    for i in 0..count {
        assert_eq!(read_marker(store, qsys, i), format!("space {i}"));
    }
    let check = store.check_pages().unwrap();
    assert!(check.bad_pages.is_empty(), "{check}");
}

fn format(devs: &[&Arc<MemBlockDev>], spaces: usize) -> (Store, ObjectId) {
    let mut store = Store::format(boxed(devs), &params(devs.len()), config()).unwrap();
    let qsys = {
        let mut txn = store.begin().unwrap();
        let q = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        txn.commit().unwrap();
        q
    };
    add_spaces(&mut store, qsys, 0, spaces);
    (store, qsys)
}

fn resume_all(store: &mut Store, unit: u16) {
    let mut journal = MemJournal::default();
    let mut res = store.resume_unit(unit, &mut journal).unwrap();
    while !store.resume_step(&mut res, &mut journal, 100).unwrap().complete {}
}

fn finish_parity(store: &mut Store) {
    let mut steps = 0;
    while store.parity_step(1000).unwrap().is_some() {
        steps += 1;
        assert!(steps < 10_000, "work does not end");
        let busy = store.parity_sets().iter().any(|s| {
            s.sync_percent.is_some() || s.members.iter().any(|m| m.rebuild_percent.is_some())
        });
        if !busy {
            break;
        }
    }
}

fn serial(uuid: &[u8; 16]) -> String {
    format!("SPARE{:02X}", uuid[0])
}

/// A mirrored pair on `a` and `b` with spaces, resumed.
fn pair(a: &Arc<MemBlockDev>, b: &Arc<MemBlockDev>) -> (Store, ObjectId) {
    let (mut store, qsys) = format(&[a], 4);
    store.start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1").unwrap();
    resume_all(&mut store, 1);
    (store, qsys)
}

#[test]
fn spares_start_stop_and_survive_an_open() {
    let a = dev(1024, 1);
    let s1 = dev(1024, 2);
    let s2 = dev(1100, 3);
    let (mut store, _) = format(&[&a], 1);
    store.start_hot_spare(Box::new(s1.clone()), [0x51; 16]).unwrap();
    store.start_hot_spare(Box::new(s2.clone()), [0x52; 16]).unwrap();
    assert!(matches!(
        store.start_hot_spare(Box::new(dev(8, 6)), [0x55; 16]),
        Err(StoreError::UnitTooSmall { .. })
    ));
    store.start_hot_spare(Box::new(dev(1024, 4)), [0x53; 16]).unwrap();
    assert!(matches!(
        store.start_hot_spare(Box::new(dev(1024, 5)), [0x54; 16]),
        Err(StoreError::Refused { reason: Refusal::SparesFull, .. })
    ));
    store.close().unwrap();
    // The third spare is not attached at the next open.
    let (mut store, rec) = Store::open(boxed(&[&s2, &a, &s1]), config()).unwrap();
    assert!(rec.events.is_empty());
    let spares = store.hot_spares();
    assert_eq!(spares.len(), 3);
    assert_eq!((spares[0].unit_uuid, spares[0].pages, spares[0].attached), ([0x51; 16], 1024, true));
    assert_eq!((spares[1].pages, spares[1].attached), (1100, true));
    assert!(!spares[2].attached);
    assert_eq!(store.units().len(), 1, "spares are in no span");
    // Stop: the device comes back wiped and the list forgets it.
    let back = store.stop_hot_spare(&[0x51; 16]).unwrap();
    assert!(back.is_some());
    assert!(store.stop_hot_spare(&[0x51; 16]).is_err());
    store.stop_hot_spare(&[0x53; 16]).unwrap();
    assert_eq!(store.hot_spares().len(), 1);
    // Nothing failed: nothing used.
    assert!(store.use_hot_spares(&serial).unwrap().is_empty());
    store.close().unwrap();
    let mut page = store::Page::zeroed();
    s1.read(0, page.bytes_mut()).unwrap();
    assert!(page.bytes().iter().all(|&b| b == 0), "header wiped");
    let (store, _) = Store::open(boxed(&[&a, &s1, &s2]), config()).unwrap();
    assert_eq!(store.hot_spares().len(), 1);
    assert_eq!(store.asp().superblock().spares, vec![[0x52; 16]]);
}

#[test]
fn a_half_that_fails_while_running_is_replaced_by_the_spare() {
    let a = dev(1024, 10);
    let b = dev(1024, 11);
    let small = dev(600, 12);
    let spare = dev(1100, 13);
    let (mut store, qsys) = pair(&a, &b);
    store.start_hot_spare(Box::new(small.clone()), [0x60; 16]).unwrap();
    store.start_hot_spare(Box::new(spare.clone()), [0x61; 16]).unwrap();
    // Half B dies.
    b.fail_reads(true);
    b.fail_after(Some(b.stats().writes));
    add_spaces(&mut store, qsys, 4, 8);
    let events = store.drain_io_events();
    assert!(
        events.iter().any(|e| matches!(e, IoEvent::UnitFailed { unit: 1, .. })),
        "{events:?}"
    );
    assert_eq!(store.unit_state(1), UnitState::NotOperational);
    // The spare large enough takes its place; the small one stays.
    let used = store.use_hot_spares(&serial).unwrap();
    assert_eq!(used.len(), 1);
    assert_eq!((used[0].unit, used[0].unit_uuid, used[0].parity), (1, [0x61; 16], false));
    assert_eq!(store.unit_state(1), UnitState::Resuming);
    assert_eq!(store.units()[1].entry.serial_text(), "SPARE61");
    assert_eq!(store.hot_spares().len(), 1);
    assert_eq!(store.hot_spares()[0].unit_uuid, [0x60; 16]);
    resume_all(&mut store, 1);
    assert_eq!(store.unit_state(1), UnitState::Active);
    assert_spaces(&mut store, qsys, 8);
    store.close().unwrap();
    // The spare now carries the half alone.
    let (mut s, rec) = Store::open(boxed(&[&spare, &small]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::Missing { unit: 0 }]);
    for i in 0..8 {
        assert_eq!(read_marker(&mut s, qsys, i), format!("space {i}"));
    }
}

#[test]
fn a_half_missing_at_open_is_replaced_by_the_spare() {
    let a = dev(1024, 20);
    let b = dev(1024, 21);
    let spare = dev(1024, 22);
    let (mut store, qsys) = pair(&a, &b);
    store.start_hot_spare(Box::new(spare.clone()), [0x70; 16]).unwrap();
    store.close().unwrap();
    let (mut store, rec) = Store::open(boxed(&[&a, &spare]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::Missing { unit: 1 }]);
    let used = store.use_hot_spares(&serial).unwrap();
    assert_eq!(used.len(), 1);
    resume_all(&mut store, 1);
    assert_spaces(&mut store, qsys, 4);
    store.close().unwrap();
    // The old half is foreign now; the table names the spare.
    let (store, rec) = Store::open(boxed(&[&a, &b, &spare]), config()).unwrap();
    assert!(rec.events.is_empty());
    assert_eq!(store.units()[1].entry.unit_uuid, [0x70; 16]);
    assert!(store.hot_spares().is_empty());
}

#[test]
fn a_parity_member_that_fails_is_rebuilt_onto_the_spare() {
    let d: Vec<Arc<MemBlockDev>> = (0..4).map(|i| dev(2048, 30 + i)).collect();
    let spare = dev(2048, 40);
    let refs: Vec<&Arc<MemBlockDev>> = d.iter().collect();
    let (mut store, qsys) = format(&refs, 6);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_parity(&mut store);
    store.start_hot_spare(Box::new(spare.clone()), [0x80; 16]).unwrap();
    d[2].fail_reads(true);
    d[2].fail_after(Some(0));
    assert_spaces(&mut store, qsys, 6);
    add_spaces(&mut store, qsys, 6, 8);
    let events = store.drain_io_events();
    assert!(
        events.iter().any(|e| matches!(e, IoEvent::UnitFailed { unit: 2, .. })),
        "{events:?}"
    );
    let used = store.use_hot_spares(&serial).unwrap();
    assert_eq!(used.len(), 1);
    assert!(used[0].parity);
    assert_eq!(store.parity_sets()[0].members[2].status, "DPY/Rebuild");
    finish_parity(&mut store);
    assert_eq!(store.parity_sets()[0].members[2].status, "DPY/Active");
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 8);
    store.close().unwrap();
    // The spare stands in for the dead unit.
    let (mut s, rec) =
        Store::open(boxed(&[&d[0], &d[1], &spare, &d[3]]), config()).unwrap();
    assert!(rec.events.is_empty(), "{:?}", rec.events);
    assert_spaces(&mut s, qsys, 8);
}

#[test]
fn a_parity_member_missing_at_open_is_rebuilt_onto_the_spare() {
    let d: Vec<Arc<MemBlockDev>> = (0..4).map(|i| dev(2048, 50 + i)).collect();
    let spare = dev(2100, 60);
    let refs: Vec<&Arc<MemBlockDev>> = d.iter().collect();
    let (mut store, qsys) = format(&refs, 6);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_parity(&mut store);
    store.start_hot_spare(Box::new(spare.clone()), [0x90; 16]).unwrap();
    store.close().unwrap();
    let (mut store, rec) = Store::open(boxed(&[&d[0], &spare, &d[1], &d[2]]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::ParityExposed { unit: 3 }]);
    assert_eq!(store.use_hot_spares(&serial).unwrap().len(), 1);
    finish_parity(&mut store);
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 6);
}
