// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS volume in the ASP: created, held across opens and RCLSTG,
//! grown within the threshold, read and written raw on unprotected and
//! parity ASPs, and the unit changes it refuses.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::asp::parity::Level;
use store::obj::ops;
use store::{FormatParams, Refusal, Store, StoreConfig, StoreError};

const WAL: u64 = 32;
const NOW: u64 = 1_767_225_600;
const PAGES: u64 = 4096;

fn params(n: usize) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [0xB7; 16],
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

fn devs(n: usize) -> Vec<Arc<MemBlockDev>> {
    (0..n).map(|_| Arc::new(MemBlockDev::new(PAGES))).collect()
}

fn boxed(devs: &[Arc<MemBlockDev>], skip: &[usize]) -> Vec<Box<dyn BlockDev>> {
    devs.iter()
        .enumerate()
        .filter(|(i, _)| !skip.contains(i))
        .map(|(_, d)| Box::new(d.clone()) as Box<dyn BlockDev>)
        .collect()
}

fn small(store: &mut Store) {
    store.set_ifs_limits(64, 256);
}

fn new_store(d: &[Arc<MemBlockDev>]) -> Store {
    let mut store = Store::format(boxed(d, &[]), &params(d.len()), config()).unwrap();
    let mut txn = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
    txn.commit().unwrap();
    store.asp_superblock_set_qsys(qsys);
    small(&mut store);
    store
}

fn open(d: &[Arc<MemBlockDev>], skip: &[usize]) -> Store {
    let (mut store, _) = Store::open(boxed(d, skip), config()).unwrap();
    small(&mut store);
    store
}

/// Volume pages with no trailer: every byte a pattern of the page.
fn pattern(v: u64, n: u64, salt: u8) -> Vec<u8> {
    (0..n * 4096)
        .map(|i| u8::try_from((v * 4096 + i) % 251).unwrap() ^ salt)
        .collect()
}

fn write_all(store: &mut Store, salt: u8) {
    let pages = store.ifs_volume().pages();
    let mut v = 0;
    while v < pages {
        let n = (pages - v).min(64);
        store.ifs_write(v, &pattern(v, n, salt)).unwrap();
        v += n;
    }
    store.ifs_flush().unwrap();
}

fn check_all(store: &mut Store, salt: u8) {
    let pages = store.ifs_volume().pages();
    let mut v = 0;
    while v < pages {
        let n = (pages - v).min(64);
        let mut buf = vec![0u8; usize::try_from(n).unwrap() * 4096];
        store.ifs_read(v, &mut buf).unwrap();
        assert!(buf == pattern(v, n, salt), "volume page {v} differs");
        v += n;
    }
}

#[test]
fn created_held_written_and_kept_across_opens_and_rclstg() {
    let d = devs(2);
    let mut store = new_store(&d);
    assert!(store.ifs_volume().is_empty());
    let usable = store.asp().usable_pages();
    let pages = store.ifs_create(90).unwrap();
    assert!(pages >= usable / 10 && pages >= 256, "{pages} of {usable}");
    for e in store.ifs_volume().extents() {
        for p in e.start..e.end() {
            assert!(store.asp().allocator().is_held(p));
        }
    }
    assert!(!store.ifs_formatted());
    write_all(&mut store, 0x5A);
    store.ifs_set_formatted().unwrap();
    // Objects keep being made around it.
    let used_before = store.asp().used_pages();
    store.close().unwrap();

    let mut store = open(&d, &[]);
    assert!(store.ifs_formatted());
    assert_eq!(store.ifs_volume().pages(), pages);
    assert_eq!(store.asp().used_pages(), used_before);
    check_all(&mut store, 0x5A);
    let report = store.reclaim(store::ReclaimMode::All, NOW).unwrap();
    assert!(report.bad_pages.is_empty(), "{report}");
    check_all(&mut store, 0x5A);
    store.close().unwrap();
    let mut store = open(&d, &[]);
    assert_eq!(store.ifs_volume().pages(), pages);
    check_all(&mut store, 0x5A);
}

#[test]
fn grows_within_the_threshold_and_never_past_it() {
    let d = devs(2);
    let mut store = new_store(&d);
    let first = store.ifs_create(90).unwrap();
    write_all(&mut store, 1);
    let added = store.ifs_grow(500, 90).unwrap();
    assert!(added >= 500);
    assert_eq!(store.ifs_volume().pages(), first + added);
    // The old pages keep their data; the new ones take writes.
    let mut buf = vec![0u8; 4096];
    store.ifs_read(0, &mut buf).unwrap();
    assert_eq!(buf, pattern(0, 1, 1));
    write_all(&mut store, 2);
    check_all(&mut store, 2);
    // Up to the threshold, then refused.
    let room = store.ifs_room(90);
    let used = store.asp().used_pages();
    let usable = store.asp().usable_pages();
    assert!(used + room <= usable * 90 / 100);
    store.ifs_grow(room, 90).unwrap();
    write_all(&mut store, 3);
    match store.ifs_grow(500, 90) {
        Err(StoreError::Refused {
            reason: Refusal::IfsNoRoom(_),
            ..
        }) => {}
        other => panic!("grew past the threshold: {other:?}"),
    }
    assert!(store.asp().used_pages() <= usable * 90 / 100 + 1);
    let pages = store.ifs_volume().pages();
    store.close().unwrap();
    let mut store = open(&d, &[]);
    assert_eq!(store.ifs_volume().pages(), pages);
    check_all(&mut store, 3);
}

#[test]
fn a_growth_without_its_checkpoint_is_lost() {
    let d = devs(1);
    let mut store = new_store(&d);
    let first = store.ifs_create(90).unwrap();
    // The map is written, then the checkpoint's first write fails.
    d[0].fail_after(Some(1));
    assert!(store.ifs_grow(300, 90).is_err());
    d[0].fail_after(None);
    d[0].drop_unflushed();
    drop(store);
    let store = open(&d, &[]);
    assert_eq!(store.ifs_volume().pages(), first);
}

#[test]
fn units_holding_the_ifs_stay() {
    let d = devs(2);
    let mut store = new_store(&d);
    // Fill unit 0 first so the volume lands on unit 1 too.
    store.ifs_create(90).unwrap();
    store.ifs_grow(store.ifs_room(90), 90).unwrap();
    let on_last = store
        .ifs_volume()
        .extents()
        .iter()
        .any(|e| e.start >= store.asp().layout().span(1).start);
    assert!(on_last, "{:?}", store.ifs_volume().extents());
    match store.remove_unit(1).map(|_| ()) {
        Err(StoreError::Refused {
            reason: Refusal::HoldsIfs,
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn parity_growth_takes_whole_blocks_and_survives_a_lost_unit() {
    let d = devs(4);
    let mut store = new_store(&d);
    // A volume from before the set: it stays where it is.
    store.ifs_create(90).unwrap();
    let before = store.ifs_volume().extents().to_vec();
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    while let Some(p) = store.parity_step(1000).unwrap() {
        if p.complete {
            break;
        }
    }
    assert_eq!(store.ifs_volume().extents(), &before[..]);
    write_all(&mut store, 7);
    let added = store.ifs_grow(600, 90).unwrap();
    // The same rows on every member, whole blocks (w = 3): each new run
    // on members 1 to 3 is matched on member 0.
    let lay = store.asp().layout().clone();
    let runs: Vec<(u16, u64, u64)> = store
        .ifs_volume()
        .extents()
        .iter()
        .filter(|e| !before.contains(e))
        .map(|e| {
            let (span, lba) = lay.locate(e.start).unwrap();
            (span, lba, e.pages)
        })
        .collect();
    let others: Vec<_> = runs.iter().filter(|r| r.0 > 0).collect();
    assert_eq!(others.len() % 3, 0, "{runs:?}");
    for &&(_, lba, len) in &others {
        assert_eq!((lba - 3) % 3, 0);
        assert_eq!(len % 3, 0);
        for m in 0..4u16 {
            let start = lay.logical(m, lba);
            let covered = store
                .ifs_volume()
                .extents()
                .iter()
                .any(|e| e.start <= start && start + len <= e.end());
            assert!(covered, "member {m} lacks rows {lba}+{len}: {runs:?}");
        }
    }
    assert_eq!(added % 12, 0);
    write_all(&mut store, 8);
    check_all(&mut store, 8);
    assert!(store.parity_verify(1).unwrap().is_empty());
    store.close().unwrap();
    // A unit missing: volume pages come from the stripes.
    let mut store = open(&d, &[2]);
    check_all(&mut store, 8);
    write_all(&mut store, 9);
    check_all(&mut store, 9);
}

#[test]
fn parity_is_refused_over_a_volume_in_its_share() {
    let d = devs(3);
    let mut store = new_store(&d);
    store.ifs_create(90).unwrap();
    store.ifs_grow(store.ifs_room(90), 90).unwrap();
    match store.start_parity(Level::Raid5, &[0, 1, 2]) {
        Err(StoreError::Refused {
            reason: Refusal::HoldsIfs | Refusal::ParityShareInUse(_),
            ..
        }) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn another_thread_serves_the_volume_while_the_store_works() {
    let d = devs(4);
    let mut store = new_store(&d);
    store.ifs_create(90).unwrap();
    store.start_parity(Level::Raid5, &[0, 1, 2, 3]).unwrap();
    let io = store.ifs_io();
    let pages = io.pages();
    let server = std::thread::spawn(move || {
        for round in 0..4u8 {
            let mut v = 0;
            while v < pages {
                let n = (pages - v).min(32);
                io.write(v, &pattern(v, n, round)).unwrap();
                v += n;
            }
            io.flush().unwrap();
            let mut v = 0;
            while v < pages {
                let n = (pages - v).min(32);
                let mut buf = vec![0u8; usize::try_from(n).unwrap() * 4096];
                io.read(v, &mut buf).unwrap();
                assert!(buf == pattern(v, n, round), "round {round} page {v}");
                v += n;
            }
        }
    });
    // Meanwhile the store synchronizes the set and makes objects.
    let qsys = store.asp().superblock().qsys;
    let mut i = 0;
    while !server.is_finished() {
        let _ = store.parity_step(16).unwrap();
        let mut txn = store.begin().unwrap();
        let p = store::obj::CreateParams {
            library: qsys,
            name: qsname::Name::parse(&format!("T{i}")).unwrap(),
            type_code: store::obj::types::USRSPC,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        };
        store::obj::usrspc::create(&mut txn, &p, 4064, false, 0).unwrap();
        txn.commit().unwrap();
        i += 1;
    }
    server.join().unwrap();
    while let Some(p) = store.parity_step(1000).unwrap() {
        if p.complete {
            break;
        }
    }
    check_all(&mut store, 3);
    assert!(store.parity_verify(1).unwrap().is_empty());
    let check = store.check_pages().unwrap();
    assert!(check.bad_pages.is_empty(), "{check}");
}
