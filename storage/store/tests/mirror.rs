// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mirrored protection: writes reach both halves, reads heal a bad
//! half, suspend and resume survive a crash in the middle, a missing
//! half is tolerated, unequal halves are truncated, and every unit
//! carries the same table.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::asp::superblock::{NO_PARTNER, Superblock};
use store::asp::unit::UnitState;
use store::obj::lock::JobHandle;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{
    FormatParams, MemJournal, PAGE_SIZE, PageType, ReclaimMode, Store, StoreConfig, StoreError,
    UnitEvent,
};

const WAL: u64 = 32;
const NOW: u64 = 1_767_225_600;

fn params() -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xA5; 16],
        unit_uuids: vec![[1; 16]],
        unit_serials: vec!["UNIT0".into()],
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

fn create_params(library: ObjectId, n: &str) -> CreateParams {
    CreateParams {
        library,
        name: qsname::Name::parse(n).unwrap(),
        type_code: types::USRSPC,
        attribute: String::new(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: false,
    }
}

/// A store on one unit with QSYS and `n` user spaces of `pages` pages.
fn store_with(d: &Arc<MemBlockDev>, n: usize, pages: u64) -> (Store, ObjectId) {
    let mut store = Store::format(boxed(&[d]), &params(), config()).unwrap();
    let qsys = {
        let mut txn = store.begin().unwrap();
        let q = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        txn.commit().unwrap();
        q
    };
    add_spaces(&mut store, qsys, 0, n, pages);
    (store, qsys)
}

fn add_spaces(store: &mut Store, qsys: ObjectId, from: usize, to: usize, pages: u64) {
    for i in from..to {
        let mut txn = store.begin().unwrap();
        let id = usrspc::create(
            &mut txn,
            &create_params(qsys, &format!("S{i}")),
            pages * 4064,
            false,
            0,
        )
        .unwrap();
        usrspc::write(&mut txn, id, 10, format!("space {i}").as_bytes()).unwrap();
        txn.commit().unwrap();
    }
}

fn superblock_on(dev: &MemBlockDev) -> Superblock {
    let mut best: Option<Superblock> = None;
    for slot in 1..=2 {
        let mut page = store::Page::zeroed();
        dev.read(slot, page.bytes_mut()).unwrap();
        if page.verify_type(slot, PageType::Superblock).is_err() {
            continue;
        }
        let sb = Superblock::decode(page.body(), slot).unwrap();
        if best.as_ref().is_none_or(|b| sb.generation > b.generation) {
            best = Some(sb);
        }
    }
    best.expect("a superblock")
}

fn assert_tables_agree(devs: &[&Arc<MemBlockDev>]) -> Superblock {
    let first = superblock_on(devs[0]);
    for d in &devs[1..] {
        let sb = superblock_on(d);
        assert_eq!(sb.generation, first.generation, "generation");
        assert_eq!(sb.units, first.units, "unit table");
    }
    first
}

/// Run a resume to completion in steps of `budget` chunks.
fn resume_all(store: &mut Store, unit: u16, journal: &mut MemJournal, budget: u64) -> u64 {
    let mut res = store.resume_unit(unit, journal).unwrap();
    let mut steps = 0;
    loop {
        let prog = store.resume_step(&mut res, journal, budget).unwrap();
        steps += 1;
        if prog.complete {
            return steps;
        }
    }
}

/// Every allocated page of the span is identical on both halves.
fn assert_halves_agree(store: &mut Store) {
    let check = store.check_pages().unwrap();
    assert_eq!(check.bad_pages.len(), 0, "{check}");
}

fn read_marker(store: &mut Store, qsys: ObjectId, name: &str) -> String {
    let mut txn = store.begin().unwrap();
    let id = ops::list(&mut txn, qsys, Some(types::USRSPC))
        .unwrap()
        .into_iter()
        .find(|(k, _)| k.name.to_string() == name)
        .map(|(_, id)| id)
        .unwrap();
    let bytes = usrspc::read(&mut txn, id, 10, 8).unwrap();
    txn.abort();
    String::from_utf8_lossy(&bytes)
        .trim_end_matches('\0')
        .to_string()
}

#[test]
fn pairing_copies_everything_and_writes_reach_both_halves() {
    let a = dev(1024, 1);
    let b = dev(1024, 2);
    let (mut store, qsys) = store_with(&a, 6, 20);
    let mut journal = MemJournal::default();
    let new = store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    assert_eq!(new, 1);
    assert_eq!(store.unit_state(1), UnitState::Resuming);
    assert_tables_agree(&[&a, &b]);
    // Writes during the resume go to both halves.
    add_spaces(&mut store, qsys, 6, 9, 20);
    let steps = resume_all(&mut store, 1, &mut journal, 1);
    assert_eq!(steps, 4, "1024 pages are four chunks");
    assert_eq!(store.unit_state(1), UnitState::Active);
    assert!(journal.saved.is_none(), "journal cleared at completion");
    let sb = assert_tables_agree(&[&a, &b]);
    assert_eq!(sb.protection, 1);
    assert_eq!(sb.units[0].mirror_partner, 1);
    assert_eq!(sb.units[1].mirror_partner, 0);
    assert_halves_agree(&mut store);
    // More writes with the pair active, then a reopen from each half alone
    // sees the same data.
    add_spaces(&mut store, qsys, 9, 12, 20);
    store.close().unwrap();
    for (only, other) in [(&a, 1u16), (&b, 0u16)] {
        let (mut s, recovery) = Store::open(boxed(&[only]), config()).unwrap();
        assert_eq!(recovery.events, vec![UnitEvent::Missing { unit: other }]);
        assert_eq!(read_marker(&mut s, qsys, "S11"), "space 11");
        drop(s);
    }
}

#[test]
fn a_bad_page_on_one_half_is_read_from_the_other_and_healed() {
    let a = dev(1024, 3);
    let b = dev(1024, 4);
    let (mut store, qsys) = store_with(&a, 4, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    store.close().unwrap();
    // Find S2's first payload page and scribble on it on half B.
    let (mut store, _) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    let page = {
        let mut txn = store.begin().unwrap();
        let id = ops::list(&mut txn, qsys, Some(types::USRSPC))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == "S2")
            .map(|(_, id)| id)
            .unwrap();
        let (_, h) = ops::read_header(&mut txn, id).unwrap();
        let p = ops::all_extents(&mut txn, &h).unwrap()[0].start;
        txn.abort();
        p
    };
    drop(store);
    let garbage = vec![0xEEu8; PAGE_SIZE];
    b.write(page, &garbage).unwrap();
    b.flush().unwrap();
    // chkstg sees the halves differ.
    let (mut store, _) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    let check = store.check_pages().unwrap();
    assert_eq!(check.bad_pages.len(), 1, "{check}");
    assert_eq!(check.bad_pages[0].page, page);
    // Reading the object heals it: whichever half serves the read,
    // reading twice covers both.
    assert_eq!(read_marker(&mut store, qsys, "S2"), "space 2");
    assert_eq!(read_marker(&mut store, qsys, "S2"), "space 2");
    let events = store.drain_io_events();
    assert!(
        events.iter().any(
            |e| matches!(e, store::asp::io::IoEvent::Healed { page: p, unit: 1 } if *p == page)
        ),
        "{events:?}"
    );
    assert_halves_agree(&mut store);
}

#[test]
fn suspend_and_resume_survive_a_crash_in_the_middle() {
    let a = dev(2048, 5);
    let b = dev(2048, 6);
    let (mut store, qsys) = store_with(&a, 4, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    store.suspend_unit(1).unwrap();
    assert_eq!(store.unit_state(1), UnitState::Suspended);
    assert_tables_agree(&[&a, &b]);
    // Writes while suspended reach half A only.
    add_spaces(&mut store, qsys, 4, 12, 30);
    // Resume, copy some chunks, then "crash": drop the store without
    // closing and reopen. The journal remembers the chunks done.
    let mut res = store.resume_unit(1, &mut journal).unwrap();
    let prog = store.resume_step(&mut res, &mut journal, 3).unwrap();
    assert!(!prog.complete);
    assert_eq!(prog.chunks_done, 3);
    assert_eq!(prog.chunks, 8);
    drop(store);
    assert_eq!(journal.saved.as_ref().unwrap().chunks_done(), 3);
    let (mut store, recovery) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    assert_eq!(recovery.events, vec![UnitEvent::Resuming { unit: 1 }]);
    assert_eq!(store.unit_state(1), UnitState::Resuming);
    let mut res = store.resume_unit(1, &mut journal).unwrap();
    assert_eq!(res.progress().chunks_done, 3, "continues from the journal");
    let prog = store.resume_step(&mut res, &mut journal, 100).unwrap();
    assert!(prog.complete);
    assert_eq!(store.unit_state(1), UnitState::Active);
    assert_halves_agree(&mut store);
    assert_eq!(read_marker(&mut store, qsys, "S11"), "space 11");
    store.close().unwrap();
    // Half B alone now carries everything.
    let (mut s, _) = Store::open(boxed(&[&b]), config()).unwrap();
    assert_eq!(read_marker(&mut s, qsys, "S11"), "space 11");
}

#[test]
fn a_missing_half_runs_suspended_and_comes_back_stale() {
    let a = dev(1024, 7);
    let b = dev(1024, 8);
    let (mut store, qsys) = store_with(&a, 2, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    store.close().unwrap();
    // Boot without B.
    let (mut store, recovery) = Store::open(boxed(&[&a]), config()).unwrap();
    assert_eq!(recovery.events, vec![UnitEvent::Missing { unit: 1 }]);
    assert_eq!(store.unit_state(1), UnitState::Missing);
    add_spaces(&mut store, qsys, 2, 5, 10);
    store.close().unwrap();
    assert_eq!(superblock_on(&a).units[1].state, UnitState::Missing);
    // B is back: stale, so suspended until resumed.
    let (mut store, recovery) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    assert_eq!(recovery.events, vec![UnitEvent::Stale { unit: 1 }]);
    assert_eq!(store.unit_state(1), UnitState::Suspended);
    resume_all(&mut store, 1, &mut journal, 100);
    assert_halves_agree(&mut store);
    assert_tables_agree(&[&a, &b]);
    store.close().unwrap();
    // Only the stale half present: the open refuses.
    let (mut store, _) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    store.suspend_unit(1).unwrap();
    store.close().unwrap();
    assert!(matches!(
        Store::open(boxed(&[&b]), config()).map(|_| ()),
        Err(StoreError::UnitMissing { unit: 0, .. })
    ));
}

#[test]
fn replacing_a_lost_half_resumes_onto_the_new_device() {
    let a = dev(1024, 9);
    let b = dev(1024, 10);
    let (mut store, qsys) = store_with(&a, 3, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    store.close().unwrap();
    let (mut store, _) = Store::open(boxed(&[&a]), config()).unwrap();
    let c = dev(1100, 11);
    store
        .replace_unit(1, Box::new(c.clone()), [3; 16], "UNIT1B")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    assert_eq!(store.unit_state(1), UnitState::Active);
    let sb = assert_tables_agree(&[&a, &c]);
    assert_eq!(sb.units[1].unit_uuid, [3; 16]);
    assert_eq!(sb.units[1].serial_text(), "UNIT1B");
    assert_eq!(sb.units[1].pages, 1100);
    assert_eq!(sb.units[1].usable_pages, 1024);
    assert_halves_agree(&mut store);
    store.close().unwrap();
    let (mut s, _) = Store::open(boxed(&[&c]), config()).unwrap();
    assert_eq!(read_marker(&mut s, qsys, "S2"), "space 2");
    // The old half B is foreign now: its UUID is not in the table.
    let (s2, _) = Store::open(boxed(&[&a, &c, &b]), config()).unwrap();
    assert_eq!(s2.units().len(), 2);
    let _ = s;
}

#[test]
fn unequal_halves_truncate_to_the_smaller() {
    let a = dev(1024, 12);
    let b = dev(600, 13);
    let (mut store, qsys) = store_with(&a, 2, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "SMALL")
        .unwrap();
    assert_eq!(store.asp().layout().total_pages(), 600);
    let sb = assert_tables_agree(&[&a, &b]);
    assert_eq!(sb.units[0].usable_pages, 600);
    assert_eq!(sb.units[0].pages, 1024);
    assert_eq!(sb.units[1].usable_pages, 600);
    resume_all(&mut store, 1, &mut journal, 100);
    assert_halves_agree(&mut store);
    // Fill past 600 pages: no space, since the span is 600 now.
    let mut txn = store.begin().unwrap();
    let created = usrspc::create(&mut txn, &create_params(qsys, "BIG"), 700 * 4064, false, 0);
    assert!(matches!(created, Err(StoreError::NoSpace { .. })));
    txn.abort();
    // Stop mirroring, keep the small half: unit 0 leaves the table.
    let gone = store.stop_mirror(1).unwrap().unwrap();
    assert_eq!(gone.blocks(), 1024);
    assert_eq!(store.units().len(), 1);
    assert_eq!(store.units()[0].entry.mirror_partner, NO_PARTNER);
    assert_eq!(store.units()[0].entry.state, UnitState::Unprotected);
    assert_eq!(store.units()[0].entry.serial_text(), "SMALL");
    assert_eq!(read_marker(&mut store, qsys, "S1"), "space 1");
    store.close().unwrap();
    assert_eq!(superblock_on(&b).units.len(), 1);
    assert_eq!(superblock_on(&b).protection, 0);
    // The wiped half is no longer a unit.
    assert!(Store::open(boxed(&[&a]), config()).is_err());
    let (s, _) = Store::open(boxed(&[&b]), config()).unwrap();
    assert_eq!(s.units().len(), 1);
}

#[test]
fn a_pair_cannot_truncate_over_data() {
    let a = dev(1024, 14);
    let b = dev(400, 15);
    let (mut store, _) = store_with(&a, 20, 30);
    let err = store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "SMALL")
        .unwrap_err();
    assert!(
        matches!(err, StoreError::UnitInUse { unit: 0, .. }),
        "{err}"
    );
    assert_eq!(store.units().len(), 1);
    assert_eq!(store.asp().layout().total_pages(), 1024);
}

#[test]
fn deleting_through_a_pair_frees_on_both_halves() {
    let a = dev(1024, 16);
    let b = dev(1024, 17);
    let (mut store, qsys) = store_with(&a, 5, 10);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    resume_all(&mut store, 1, &mut journal, 100);
    {
        let mut txn = store.begin().unwrap();
        let ids: Vec<ObjectId> = ops::list(&mut txn, qsys, Some(types::USRSPC))
            .unwrap()
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        for id in ids {
            ops::delete(&mut txn, JobHandle(1), id).unwrap();
        }
        txn.commit().unwrap();
    }
    let report = store.reclaim(ReclaimMode::Check, NOW).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
    assert_halves_agree(&mut store);
}
