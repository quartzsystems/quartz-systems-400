// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Multi-unit ASPs: the unit table, adding and removing units, every
//! unit carrying the same table, and a missing unprotected unit.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::asp::superblock::{NO_PARTNER, Superblock};
use store::asp::unit::UnitState;
use store::obj::lock::JobHandle;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{FormatParams, PageType, Store, StoreConfig, StoreError};

const WAL: u64 = 32;

fn params(units: usize) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xA5; 16],
        unit_uuids: (0..units)
            .map(|i| [u8::try_from(i + 1).unwrap(); 16])
            .collect(),
        unit_serials: (0..units).map(|i| format!("SER{i}")).collect(),
        wal_pages: WAL,
        now: 1_767_225_600,
    }
}

fn config() -> StoreConfig {
    StoreConfig {
        cache_pages: 64,
        checkpoint_txns: 16,
        checkpoint_secs: 30,
    }
}

fn devices(sizes: &[u64], seed: u64) -> Vec<Arc<MemBlockDev>> {
    sizes
        .iter()
        .enumerate()
        .map(|(i, &s)| Arc::new(MemBlockDev::with_seed(s, seed + i as u64)))
        .collect()
}

fn boxed(devs: &[Arc<MemBlockDev>]) -> Vec<Box<dyn BlockDev>> {
    devs.iter()
        .map(|d| Box::new(d.clone()) as Box<dyn BlockDev>)
        .collect()
}

/// The newest superblock a device carries, from its own two slots.
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

/// Every attached device must carry the same table at the same generation.
fn assert_tables_agree(devs: &[Arc<MemBlockDev>]) -> Superblock {
    let first = superblock_on(&devs[0]);
    for d in &devs[1..] {
        let sb = superblock_on(d);
        assert_eq!(sb.generation, first.generation);
        assert_eq!(sb.units, first.units);
    }
    first
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
        now: 1_767_225_600,
        by_system: false,
    }
}

#[test]
fn three_units_make_one_page_space() {
    let devs = devices(&[1024, 512, 256], 1);
    let store = Store::format(boxed(&devs), &params(3), config()).unwrap();
    let units = store.units();
    assert_eq!(units.len(), 3);
    assert_eq!(units[1].entry.start, 1024);
    assert_eq!(units[2].entry.start, 1536);
    assert_eq!(units[2].entry.serial_text(), "SER2");
    assert!(units.iter().all(|u| u.entry.mirror_partner == NO_PARTNER));
    assert!(units.iter().all(|u| u.entry.state == UnitState::Unprotected));
    assert_eq!(store.asp().layout().total_pages(), 1792);
    store.close().unwrap();
    let sb = assert_tables_agree(&devs);
    assert_eq!(sb.units.len(), 3);
    assert_eq!(sb.protection, 0);
    // Reopen with the units in another order.
    let mut rev = devs.clone();
    rev.reverse();
    let (store, recovery) = Store::open(boxed(&rev), config()).unwrap();
    assert!(recovery.events.is_empty());
    assert_eq!(store.units()[0].entry.serial_text(), "SER0");
    assert_eq!(store.asp().layout().span(2).primary, 2);
}

#[test]
fn units_are_added_and_removed_through_the_table() {
    let devs = devices(&[1024], 2);
    let mut store = Store::format(boxed(&devs), &params(1), config()).unwrap();
    let qsys = {
        let mut t = store.begin().unwrap();
        let q = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        t.commit().unwrap();
        q
    };
    // Larger than unit 0's free space, so new allocations land on it.
    let extra = Arc::new(MemBlockDev::with_seed(1536, 9));
    let unit = store
        .add_unit(Box::new(extra.clone()), [7; 16], "EXTRA1")
        .unwrap();
    assert_eq!(unit, 1);
    assert_eq!(store.asp().layout().total_pages(), 2560);
    let mut all = devs.clone();
    all.push(extra.clone());
    let sb = assert_tables_agree(&all);
    assert_eq!(sb.units[1].serial_text(), "EXTRA1");
    assert_eq!(sb.units[1].start, 1024);

    // The allocator takes from the span with the most free pages: the
    // new unit.
    for i in 0..12 {
        let mut t = store.begin().unwrap();
        usrspc::create(&mut t, &create_params(qsys, &format!("S{i}")), 40 * 4064, false, 0)
            .unwrap();
        t.commit().unwrap();
    }
    assert!(store.units()[1].used_pages > 0);
    // Write something recognizable into the last object, then remove
    // the unit: its data moves to unit 0 and stays readable.
    let last = {
        let mut t = store.begin().unwrap();
        let id = ops::list(&mut t, qsys, Some(types::USRSPC))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == "S11")
            .map(|(_, id)| id)
            .unwrap();
        let (_, h) = ops::read_header(&mut t, id).unwrap();
        let first = ops::all_extents(&mut t, &h).unwrap()[0].start;
        assert_eq!(t.layout().locate(first).unwrap().0, 1, "S11 lives on unit 1");
        let mut page = store::Page::zeroed();
        page.body_mut()[100..119].copy_from_slice(b"moved with the unit");
        store::Pager::write(&mut t, first, &mut page, store::PageType::ObjectData, id).unwrap();
        t.commit().unwrap();
        id
    };
    let dev = store.remove_unit(1).unwrap();
    assert_eq!(dev.blocks(), 1536);
    assert_eq!(store.units().len(), 1);
    assert_eq!(store.asp().layout().total_pages(), 1024);
    {
        let mut t = store.begin().unwrap();
        let (_, h) = ops::read_header(&mut t, last).unwrap();
        let first = ops::all_extents(&mut t, &h).unwrap()[0].start;
        assert_eq!(t.layout().locate(first).unwrap().0, 0, "S11 moved to unit 0");
        let page = store::Pager::read(&mut t, first).unwrap();
        assert_eq!(&page.body()[100..119], b"moved with the unit");
        let ids = ops::list(&mut t, qsys, Some(types::USRSPC)).unwrap();
        assert_eq!(ids.len(), 12);
        t.abort();
    }
    let check = store.reclaim(store::ReclaimMode::Check, 1).unwrap();
    assert_eq!(check.problems(), 0, "{check}");
    {
        let mut t = store.begin().unwrap();
        let ids: Vec<ObjectId> = ops::list(&mut t, qsys, Some(types::USRSPC))
            .unwrap()
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        for id in ids {
            ops::delete(&mut t, JobHandle(1), id).unwrap();
        }
        t.commit().unwrap();
    }
    assert!(matches!(
        store.remove_unit(0),
        Err(StoreError::UnitInUse { unit: 0, .. })
    ));
    store.close().unwrap();
    // The removed device is no longer a unit: opening it alone fails.
    assert!(Store::open(vec![Box::new(extra.clone())], config()).is_err());
    // And the ASP opens without it.
    let (store, _) = Store::open(boxed(&devs), config()).unwrap();
    assert_eq!(store.units().len(), 1);
    assert_eq!(superblock_on(&devs[0]).units.len(), 1);
}

#[test]
fn a_missing_unprotected_unit_stops_the_open() {
    let devs = devices(&[1024, 512], 3);
    let store = Store::format(boxed(&devs), &params(2), config()).unwrap();
    store.close().unwrap();
    let Err(err) = Store::open(boxed(&devs[..1]), config()) else {
        panic!("opened without unit 1");
    };
    match err {
        StoreError::UnitMissing { unit: 1, what } => assert!(what.contains("SER1"), "{what}"),
        other => panic!("{other}"),
    }
    let Err(err) = Store::open(boxed(&devs[1..]), config()) else {
        panic!("opened without unit 0");
    };
    assert!(matches!(err, StoreError::UnitMissing { unit: 0, .. }));
}

#[test]
fn foreign_units_are_ignored() {
    let devs = devices(&[1024], 4);
    Store::format(boxed(&devs), &params(1), config())
        .unwrap()
        .close()
        .unwrap();
    let other = devices(&[1024], 5);
    let mut p = params(1);
    p.asp_uuid = [0x5A; 16];
    p.unit_uuids = vec![[0x77; 16]];
    Store::format(boxed(&other), &p, config())
        .unwrap()
        .close()
        .unwrap();
    // Two ASPs' units in one set: the one with more units wins, ties go
    // to the first found; either way the open is of one ASP only.
    let mut all = devs.clone();
    all.push(other[0].clone());
    let (store, _) = Store::open(boxed(&all), config()).unwrap();
    assert_eq!(store.units().len(), 1);
}
