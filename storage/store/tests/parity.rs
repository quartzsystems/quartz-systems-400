// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Device parity protection: start over units holding data, read with a
//! unit missing, keep parity on writes, repair an epoch cut between its
//! data and its parity, rebuild a replaced unit, lose two units of a
//! RAID 6 set, include, exclude and stop.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::asp::parity::{Fault, Level, MemberState, WorkKind};
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{Asp, FormatParams, Page, PageType, Store, StoreConfig, StoreError, UnitEvent};

const WAL: u64 = 32;
const NOW: u64 = 1_767_225_600;
const PAGES: u64 = 2048;

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

fn devs(n: usize, seed: u64) -> Vec<Arc<MemBlockDev>> {
    (0..n)
        .map(|i| Arc::new(MemBlockDev::with_seed(PAGES, seed + i as u64)))
        .collect()
}

fn boxed(devs: &[Arc<MemBlockDev>], skip: &[usize]) -> Vec<Box<dyn BlockDev>> {
    devs.iter()
        .enumerate()
        .filter(|(i, _)| !skip.contains(i))
        .map(|(_, d)| Box::new(d.clone()) as Box<dyn BlockDev>)
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
        usrspc::write(&mut txn, id, pages * 4000, format!("tail {i}").as_bytes()).unwrap();
        txn.commit().unwrap();
    }
}

/// A store over `n` units with QSYS and `count` user spaces.
fn store_with(d: &[Arc<MemBlockDev>], count: usize) -> (Store, ObjectId) {
    let mut store = Store::format(boxed(d, &[]), &params(d.len()), config()).unwrap();
    let qsys = {
        let mut txn = store.begin().unwrap();
        let q = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        txn.commit().unwrap();
        q
    };
    add_spaces(&mut store, qsys, 0, count, 12);
    (store, qsys)
}

fn read_space(store: &mut Store, qsys: ObjectId, i: usize) -> (String, String) {
    let name = format!("S{i}");
    let mut txn = store.begin().unwrap();
    let id = ops::list(&mut txn, qsys, Some(types::USRSPC))
        .unwrap()
        .into_iter()
        .find(|(k, _)| k.name.to_string() == name)
        .map_or_else(|| panic!("{name} not found"), |(_, id)| id);
    let head = usrspc::read(&mut txn, id, 10, 8).unwrap();
    let tail = usrspc::read(&mut txn, id, 12 * 4000, 8).unwrap();
    txn.abort();
    let text = |b: Vec<u8>| String::from_utf8_lossy(&b).trim_end_matches('\0').to_string();
    (text(head), text(tail))
}

fn assert_spaces(store: &mut Store, qsys: ObjectId, count: usize) {
    for i in 0..count {
        let (head, tail) = read_space(store, qsys, i);
        assert_eq!(head, format!("space {i}"));
        assert_eq!(tail.trim(), format!("tail {i}"));
    }
    let check = store.check_pages().unwrap();
    assert!(check.bad_pages.is_empty(), "{check}");
}

/// Run the parity work to its end; returns the steps taken.
fn finish_work(store: &mut Store, budget: u64) -> usize {
    let mut steps = 0;
    while let Some(p) = store.parity_step(budget).unwrap() {
        steps += 1;
        assert!(steps < 10_000, "work does not end");
        if p.complete && store.parity_sets().iter().all(|s| s.sync_percent.is_none()) {
            let busy = store
                .parity_sets()
                .iter()
                .any(|s| s.members.iter().any(|m| m.rebuild_percent.is_some()));
            if !busy {
                break;
            }
        }
    }
    steps
}

#[test]
fn start_over_units_holding_data_and_keep_parity() {
    let d = devs(4, 10);
    let (mut store, qsys) = store_with(&d, 12);
    let set = store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    assert_eq!(set, 1);
    let info = &store.parity_sets()[0];
    assert_eq!(info.sync_percent, Some(0));
    assert!(info.members.iter().all(|m| m.status == "DPY/Unprotected"));
    assert_eq!(store.asp().superblock().protection & 2, 2);
    // Writes while the set synchronizes.
    add_spaces(&mut store, qsys, 12, 14, 12);
    let mut last = 0;
    let mut steps = 0;
    while let Some(p) = store.parity_step(40).unwrap() {
        assert_eq!(p.kind, WorkKind::Sync);
        assert!(p.percent >= last);
        last = p.percent;
        steps += 1;
        if p.complete {
            break;
        }
    }
    assert!(steps > 2, "the synchronization runs in steps");
    let info = &store.parity_sets()[0];
    assert!(info.members.iter().all(|m| m.status == "DPY/Active"));
    assert!(store.parity_verify(1).unwrap().is_empty());
    // Writes with the set active keep parity.
    add_spaces(&mut store, qsys, 14, 20, 12);
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 20);
    store.close().unwrap();
    let (mut store, rec) = Store::open(boxed(&d, &[]), config()).unwrap();
    assert!(rec.events.is_empty(), "{:?}", rec.events);
    let sb = store.asp().superblock().clone();
    assert_eq!(sb.parity.tags.len(), 4);
    assert!(sb.parity.tags.iter().all(|t| t.state == MemberState::Active));
    assert_eq!(sb.parity.work, None);
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 20);
}

#[test]
fn a_missing_unit_leaves_the_asp_usable_and_is_rebuilt_when_back() {
    let d = devs(4, 20);
    let (mut store, qsys) = store_with(&d, 12);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_work(&mut store, 1000);
    store.close().unwrap();
    // Unit 2 missing: exposed, not stopped.
    let (mut store, rec) = Store::open(boxed(&d, &[2]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::ParityExposed { unit: 2 }]);
    let info = store.parity_sets()[0].clone();
    assert_eq!(info.members[2].status, "DPY/Failed");
    assert_eq!(info.members[0].status, "DPY/Unprotected");
    assert!(!store.units()[2].attached);
    assert_spaces(&mut store, qsys, 12);
    // Writes while exposed, including pages that belong on the missing
    // unit: parity carries them.
    add_spaces(&mut store, qsys, 12, 18, 12);
    assert_spaces(&mut store, qsys, 18);
    store.close().unwrap();
    // A second unit missing is beyond RAID 5: B1001030.
    match Store::open(boxed(&d, &[1, 2]), config()) {
        Err(StoreError::UnitMissing { .. }) => {}
        Err(e) => panic!("wrong error {e}"),
        Ok(_) => panic!("opened without two units"),
    }
    // Unit 2 back: stale, rebuilt.
    let (mut store, rec) = Store::open(boxed(&d, &[]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::ParityStale { unit: 2 }]);
    assert_eq!(store.parity_sets()[0].members[2].status, "DPY/Rebuild");
    let p = store.parity_step(10).unwrap().unwrap();
    assert_eq!((p.kind, p.unit), (WorkKind::Rebuild, Some(2)));
    finish_work(&mut store, 50);
    assert!(store.parity_sets()[0].members.iter().all(|m| m.status == "DPY/Active"));
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 18);
    store.close().unwrap();
    // Every unit now holds its own pages: any one may go.
    for gone in 0..4 {
        let (mut s, _) = Store::open(boxed(&d, &[gone]), config()).unwrap();
        assert_spaces(&mut s, qsys, 18);
        drop(s);
        resync(&d);
    }
}

#[test]
fn a_replaced_unit_is_rebuilt_from_the_others() {
    let d = devs(4, 30);
    let (mut store, qsys) = store_with(&d, 10);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_work(&mut store, 1000);
    store.close().unwrap();
    let (mut store, _) = Store::open(boxed(&d, &[3]), config()).unwrap();
    let fresh = Arc::new(MemBlockDev::with_seed(PAGES + 16, 99));
    store
        .replace_unit(3, Box::new(fresh.clone()), [0x77; 16], "NEW3")
        .unwrap();
    assert_eq!(store.parity_sets()[0].members[3].status, "DPY/Rebuild");
    assert_eq!(store.units()[3].entry.serial_text(), "NEW3");
    // Writes during the rebuild.
    add_spaces(&mut store, qsys, 10, 13, 12);
    finish_work(&mut store, 30);
    assert!(store.parity_verify(1).unwrap().is_empty());
    store.close().unwrap();
    let mut set = vec![d[0].clone(), d[1].clone(), d[2].clone(), fresh];
    let (mut store, rec) = Store::open(boxed(&set, &[]), config()).unwrap();
    assert!(rec.events.is_empty());
    assert_spaces(&mut store, qsys, 13);
    drop(store);
    // The new unit alone carries what the old one held.
    set.remove(0);
    let (mut store, _) = Store::open(boxed(&set, &[]), config()).unwrap();
    assert_spaces(&mut store, qsys, 13);
}

/// The write hole: an epoch's data pages reach the units and its parity
/// does not, then a unit holding another page of the same stripe is
/// lost. The partial parity gives that page back.
#[test]
fn an_epoch_cut_between_data_and_parity_is_repaired_with_a_unit_missing() {
    let d = devs(4, 40);
    let mut asp = Asp::format(boxed(&d, &[]), &params(4)).unwrap();
    asp.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    asp.checkpoint().unwrap();
    while !asp.parity_step(1000).unwrap().unwrap().complete {}
    asp.checkpoint().unwrap();
    let geo = asp.parity_sets()[0].geometry;
    // A stripe well past the WAL, and two of its data pages.
    let stripe = 300 * geo.n as u64 + 1;
    let dm = geo.data_members(stripe);
    let (a, b) = (dm[0], dm[1]);
    let pno = |asp: &Asp, m: usize| {
        let unit = u16::try_from(m).unwrap();
        asp.layout().logical(asp.span_of(unit).unwrap(), geo.row_of(stripe, m))
    };
    let (pa, pb) = (pno(&asp, a), pno(&asp, b));
    let page = |fill: u8| {
        let mut p = Page::zeroed();
        p.body_mut()[..100].fill(fill);
        p
    };
    asp.io()
        .write_page(pb, &mut page(0xBB), PageType::ObjectData, ObjectId::NONE, 1)
        .unwrap();
    asp.io()
        .write_page(pa, &mut page(0xA1), PageType::ObjectData, ObjectId::NONE, 1)
        .unwrap();
    asp.io().flush().unwrap();
    assert!(asp.parity_verify(1).unwrap().is_empty());
    // Cut the next epoch after its data.
    asp.parity_fault(1, Some(Fault::StopAfterData));
    asp.io()
        .write_page(pa, &mut page(0xA2), PageType::ObjectData, ObjectId::NONE, 2)
        .unwrap();
    assert!(asp.io().flush().is_err());
    drop(asp);
    for x in &d {
        x.drop_unflushed();
    }
    // Without repair the stripe's parity is the old one: B computed from
    // it and the new A would be wrong. Open without B's unit.
    let (mut asp, events) = Asp::open(boxed(&d, &[b])).unwrap();
    assert_eq!(
        events,
        vec![UnitEvent::ParityExposed {
            unit: u16::try_from(b).unwrap()
        }]
    );
    let got_b = asp.io().read_page(pb).unwrap();
    assert_eq!(&got_b.body()[..100], &[0xBB; 100][..]);
    let got_a = asp.io().read_page(pa).unwrap();
    assert_eq!(&got_a.body()[..100], &[0xA2; 100][..]);
}

/// The same cut, all units present: the stripe's parity is made to
/// match again at open.
#[test]
fn an_epoch_cut_with_every_unit_present_is_resynchronized() {
    let d = devs(4, 50);
    let mut asp = Asp::format(boxed(&d, &[]), &params(4)).unwrap();
    asp.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    asp.checkpoint().unwrap();
    while !asp.parity_step(1000).unwrap().unwrap().complete {}
    asp.checkpoint().unwrap();
    let geo = asp.parity_sets()[0].geometry;
    let stripe = 200 * geo.n as u64 + 2;
    let m = geo.data_members(stripe)[1];
    let unit = u16::try_from(m).unwrap();
    let pno = asp.layout().logical(asp.span_of(unit).unwrap(), geo.row_of(stripe, m));
    asp.parity_fault(1, Some(Fault::StopAfterData));
    let mut p = Page::zeroed();
    p.body_mut()[..10].fill(7);
    asp.io()
        .write_page(pno, &mut p, PageType::ObjectData, ObjectId::NONE, 1)
        .unwrap();
    assert!(asp.io().flush().is_err());
    drop(asp);
    let (mut asp, _) = Asp::open(boxed(&d, &[])).unwrap();
    assert!(asp.parity_verify(1).unwrap().is_empty());
}

/// Crashes at random points of a write-heavy run: whatever was
/// committed survives, with any one unit missing at reopen.
#[test]
fn committed_data_survives_crashes_and_a_lost_unit() {
    let mut stale = 0;
    for round in 0..4u64 {
        let d = devs(4, 60 + round * 10);
        let (mut store, qsys) = store_with(&d, 6);
        store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
        finish_work(&mut store, 1000);
        // Fail one unit's writes part way into the next batch: the
        // store either commits through it (the set loses the unit) or
        // stops; what was committed before must read back.
        let victim = usize::try_from(round % 4).unwrap();
        d[victim].fail_after(Some(d[victim].stats().writes + 40 + round * 13));
        let mut committed = 6;
        for i in 6..14 {
            let Ok(mut txn) = store.begin() else {
                break;
            };
            let ok = usrspc::create(
                &mut txn,
                &create_params(qsys, &format!("S{i}")),
                12 * 4064,
                false,
                0,
            )
            .and_then(|id| {
                usrspc::write(&mut txn, id, 10, format!("space {i}").as_bytes())?;
                usrspc::write(&mut txn, id, 12 * 4000, format!("tail {i}").as_bytes())
            });
            if ok.is_err() || txn.commit().is_err() {
                break;
            }
            committed = i + 1;
        }
        drop(store);
        for x in &d {
            x.drop_unflushed();
            x.fail_after(None);
        }
        // The victim, if it failed, is known stale even though no
        // checkpoint recorded it: reopen with every unit, rebuild, then
        // lose one.
        let other = (victim + 1) % 4;
        let (mut s, rec) = Store::open(boxed(&d, &[]), config()).unwrap();
        let v = u16::try_from(victim).unwrap();
        assert!(
            rec.events.is_empty() || rec.events == vec![UnitEvent::ParityStale { unit: v }],
            "round {round}: {:?}",
            rec.events
        );
        stale += rec.events.len();
        for i in 0..committed {
            assert_eq!(read_space(&mut s, qsys, i).0, format!("space {i}"), "round {round}");
        }
        finish_work(&mut s, 1000);
        assert!(s.parity_verify(1).unwrap().is_empty());
        s.close().unwrap();
        for skip in [victim, other] {
            let (mut s, _) = Store::open(boxed(&d, &[skip]), config())
                .unwrap_or_else(|e| panic!("round {round} without {skip}: {e}"));
            for i in 0..committed {
                let (head, _) = read_space(&mut s, qsys, i);
                assert_eq!(head, format!("space {i}"), "round {round} without {skip}");
            }
            drop(s);
            resync(&d);
        }
    }
    assert!(stale > 0, "no round lost its victim");
}

#[test]
fn raid6_survives_two_lost_units() {
    let d = devs(5, 70);
    let (mut store, qsys) = store_with(&d, 10);
    store.start_parity(Level::Raid6, &[0, 1, 2, 3, 4]).unwrap();
    finish_work(&mut store, 1000);
    assert!(store.parity_verify(1).unwrap().is_empty());
    store.close().unwrap();
    for gone in [[0, 1], [1, 3], [2, 4], [0, 4]] {
        let (mut s, rec) = Store::open(boxed(&d, &gone), config()).unwrap();
        assert_eq!(rec.events.len(), 2);
        let info = s.parity_sets()[0].clone();
        assert_eq!(info.members[gone[0]].status, "DPY/Failed");
        assert_spaces(&mut s, qsys, 10);
        drop(s);
        resync(&d);
    }
    // One lost: the rest are degraded, still protected.
    let (mut s, _) = Store::open(boxed(&d, &[2]), config()).unwrap();
    assert_eq!(s.parity_sets()[0].members[0].status, "DPY/Degraded");
    add_spaces(&mut s, qsys, 10, 12, 12);
    assert_spaces(&mut s, qsys, 12);
    s.close().unwrap();
    // Three is too many.
    assert!(matches!(
        Store::open(boxed(&d, &[0, 1, 2]), config()),
        Err(StoreError::UnitMissing { .. })
    ));
}

#[test]
fn a_crash_during_synchronization_continues_from_the_mark() {
    let d = devs(4, 80);
    let (mut store, qsys) = store_with(&d, 8);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    for _ in 0..3 {
        store.parity_step(30).unwrap();
    }
    store.checkpoint().unwrap();
    let mark = store.asp().superblock().parity.work.unwrap();
    assert_eq!(mark.kind, WorkKind::Sync);
    assert_eq!(mark.done, 90);
    store.parity_step(30).unwrap();
    drop(store);
    for x in &d {
        x.drop_unflushed();
    }
    // A synchronizing set cannot lose a unit.
    assert!(matches!(
        Store::open(boxed(&d, &[1]), config()),
        Err(StoreError::UnitMissing { .. })
    ));
    let (mut store, _) = Store::open(boxed(&d, &[]), config()).unwrap();
    assert!(store.parity_sets()[0].sync_percent.is_some());
    let p = store.parity_step(30).unwrap().unwrap();
    assert_eq!(p.done, 120, "continues from the checkpoint's mark");
    finish_work(&mut store, 1000);
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 8);
}

#[test]
fn include_exclude_and_stop() {
    let d = devs(5, 90);
    let (mut store, qsys) = store_with(&d, 8);
    let held_before = store.asp().allocator().total_free();
    store.start_parity(Level::Raid5, &[0, 1, 2]).unwrap();
    finish_work(&mut store, 1000);
    assert!(store.asp().allocator().total_free() < held_before);
    // Include unit 3: synchronized again over four.
    store.include_parity(1, 3).unwrap();
    assert_eq!(store.parity_sets()[0].members.len(), 4);
    assert!(store.parity_sets()[0].sync_percent.is_some());
    finish_work(&mut store, 1000);
    assert!(store.parity_verify(1).unwrap().is_empty());
    // Unit 4 cannot be mirrored or removed while in a set.
    store.start_parity(Level::Raid5, &[4]).unwrap_err();
    store.include_parity(1, 4).unwrap();
    assert!(store.remove_unit(4).is_err());
    finish_work(&mut store, 1000);
    // Exclude unit 0.
    store.exclude_parity(0).unwrap();
    finish_work(&mut store, 1000);
    assert_eq!(store.parity_set_of(0), None);
    assert_eq!(store.parity_sets()[0].members.len(), 4);
    assert!(store.parity_verify(1).unwrap().is_empty());
    add_spaces(&mut store, qsys, 8, 11, 12);
    store.close().unwrap();
    let (mut store, _) = Store::open(boxed(&d, &[2]), config()).unwrap();
    assert_spaces(&mut store, qsys, 11);
    drop(store);
    let (mut store, _) = Store::open(boxed(&d, &[]), config()).unwrap();
    finish_work(&mut store, 1000);
    store.stop_parity(1).unwrap();
    assert!(store.parity_sets().is_empty());
    assert_eq!(store.asp().superblock().protection, 0);
    assert!((0..5).all(|u| store.asp().parity_held(u) == 0));
    assert_spaces(&mut store, qsys, 11);
    // Too few units for RAID 6.
    assert!(matches!(
        store.start_parity(Level::Raid6, &[0, 1, 2]),
        Err(StoreError::Refused { reason: store::Refusal::ParityUnits { .. }, .. })
    ));
}

#[test]
fn a_unit_that_dies_while_running_is_failed_and_its_pages_computed() {
    let d = devs(4, 100);
    let (mut store, qsys) = store_with(&d, 10);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_work(&mut store, 1000);
    store.close().unwrap();
    let (mut store, _) = Store::open(boxed(&d, &[]), config()).unwrap();
    d[1].fail_reads(true);
    d[1].fail_after(Some(0));
    assert_spaces(&mut store, qsys, 10);
    add_spaces(&mut store, qsys, 10, 12, 12);
    let events = store.drain_io_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, store::asp::io::IoEvent::UnitFailed { unit: 1, .. })),
        "{events:?}"
    );
    assert_eq!(store.parity_sets()[0].members[1].status, "DPY/Failed");
    store.checkpoint().unwrap();
    let tag = store.asp().superblock().parity.tags[1].clone();
    assert_eq!(tag.state, MemberState::Failed);
    assert_spaces(&mut store, qsys, 12);
}

/// Open with every unit, rebuild what was lost meanwhile, close: a unit
/// left out of an open is stale afterwards.
fn resync(d: &[Arc<MemBlockDev>]) {
    let (mut s, _) = Store::open(boxed(d, &[]), config()).unwrap();
    finish_work(&mut s, 1000);
    assert!(s.parity_verify(1).unwrap().is_empty());
    s.close().unwrap();
}

/// RAID 6: an epoch cut after its data, then two units holding other
/// pages of the stripe lost.
#[test]
fn raid6_epoch_cut_is_repaired_with_two_units_missing() {
    let d = devs(5, 110);
    let mut asp = Asp::format(boxed(&d, &[]), &params(5)).unwrap();
    asp.start_parity(Level::Raid6, &[0, 1, 2, 3, 4]).unwrap();
    asp.checkpoint().unwrap();
    while !asp.parity_step(1000).unwrap().unwrap().complete {}
    asp.checkpoint().unwrap();
    let geo = asp.parity_sets()[0].geometry;
    let stripe = 250 * geo.n as u64 + 3;
    let dm = geo.data_members(stripe);
    assert_eq!(dm.len(), 3);
    let pno = |asp: &Asp, m: usize| {
        let unit = u16::try_from(m).unwrap();
        asp.layout().logical(asp.span_of(unit).unwrap(), geo.row_of(stripe, m))
    };
    let pages: Vec<u64> = dm.iter().map(|&m| pno(&asp, m)).collect();
    let page = |fill: u8| {
        let mut p = Page::zeroed();
        p.body_mut()[..64].fill(fill);
        p
    };
    for (i, &p) in pages.iter().enumerate() {
        let fill = 0x10 + u8::try_from(i).unwrap();
        asp.io()
            .write_page(p, &mut page(fill), PageType::ObjectData, ObjectId::NONE, 1)
            .unwrap();
    }
    asp.io().flush().unwrap();
    asp.parity_fault(1, Some(Fault::StopAfterData));
    asp.io()
        .write_page(pages[0], &mut page(0x99), PageType::ObjectData, ObjectId::NONE, 2)
        .unwrap();
    assert!(asp.io().flush().is_err());
    drop(asp);
    for x in &d {
        x.drop_unflushed();
    }
    let (mut asp, events) = Asp::open(boxed(&d, &[dm[1], dm[2]])).unwrap();
    assert_eq!(events.len(), 2);
    for (i, &p) in pages.iter().enumerate().skip(1) {
        let want = 0x10 + u8::try_from(i).unwrap();
        assert_eq!(&asp.io().read_page(p).unwrap().body()[..64], &[want; 64][..]);
    }
    assert_eq!(&asp.io().read_page(pages[0]).unwrap().body()[..64], &[0x99; 64][..]);
}

#[test]
fn start_is_refused_when_parity_would_take_pages_in_use() {
    let d = devs(3, 120);
    let (mut store, qsys) = store_with(&d, 0);
    let mut i = 0;
    while store.asp().available() > 1500 {
        add_spaces(&mut store, qsys, i, i + 1, 40);
        i += 1;
    }
    match store.start_parity(Level::Raid5, &[0, 1, 2]) {
        Err(StoreError::Refused { reason: store::Refusal::ParityShareInUse(_), .. }) => {}
        other => panic!("started anyway: {:?}", other.map(|_| ())),
    }
    assert!(store.parity_sets().is_empty());
}

#[test]
fn a_smaller_load_source_sets_the_size_and_larger_units_hold_their_rest() {
    // Unit 0 two thirds the size of the others, as a load source that
    // also carries the ESP, the LIC and the IFS partitions.
    let mut d = vec![Arc::new(MemBlockDev::with_seed(PAGES * 2 / 3, 70))];
    d.extend((1..4).map(|i| Arc::new(MemBlockDev::with_seed(PAGES, 70 + i))));
    let (mut store, qsys) = store_with(&d, 6);
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    finish_work(&mut store, 1000);
    let held: Vec<u64> = (0..4).map(|u| store.asp().parity_held(u)).collect();
    assert!(held[1] > held[0] + PAGES / 4, "{held:?}");
    assert_eq!(held[1], held[2]);
    add_spaces(&mut store, qsys, 6, 10, 12);
    assert!(store.parity_verify(1).unwrap().is_empty());
    store.close().unwrap();
    let (mut store, rec) = Store::open(boxed(&d, &[3]), config()).unwrap();
    assert_eq!(rec.events, vec![UnitEvent::ParityExposed { unit: 3 }]);
    assert_spaces(&mut store, qsys, 10);
}
