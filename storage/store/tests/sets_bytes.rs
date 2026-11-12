// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bytes mirrored and parity protection put on the units. Each
//! scenario runs a fixed list of operations and takes the digest of
//! every unit's image at its stops; `tests/golden/sets.txt` holds the
//! digests the code wrote before the protection engine moved to the
//! `protect` crate (handoff 14), so the same digests after it are the
//! same bytes: a set written before opens after, and the reverse.
//!
//! Object headers and the superblock record the release, so the digests
//! are compared only by a build of the release the file names (the
//! first line); another release still runs every scenario and reads
//! everything back. `QS400_UPDATE_GOLDEN=1` rewrites the file.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use crc::{CRC_32_ISCSI, CRC_64_XZ, Crc};
use store::asp::parity::Level;
use store::asp::unit::UnitState;
use store::obj::{CreateParams, ObjectId, ops, types, usrspc};
use store::{FormatParams, MemJournal, Store, StoreConfig, UnitEvent};

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

/// Checkpoints by count only: the clock has no say in the bytes.
fn config() -> StoreConfig {
    StoreConfig {
        cache_pages: 16,
        checkpoint_txns: 8,
        checkpoint_secs: u64::MAX,
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

fn store_with(d: &[&Arc<MemBlockDev>], count: usize, pages: u64) -> (Store, ObjectId) {
    let mut store = Store::format(boxed(d), &params(d.len()), config()).unwrap();
    let qsys = {
        let mut txn = store.begin().unwrap();
        let q = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        txn.commit().unwrap();
        q
    };
    add_spaces(&mut store, qsys, 0, count, pages);
    (store, qsys)
}

fn assert_spaces(store: &mut Store, qsys: ObjectId, count: usize, pages: u64) {
    let mut txn = store.begin().unwrap();
    let list = ops::list(&mut txn, qsys, Some(types::USRSPC)).unwrap();
    for i in 0..count {
        let name = format!("S{i}");
        let id = list
            .iter()
            .find(|(k, _)| k.name.to_string() == name)
            .map_or_else(|| panic!("{name} not found"), |(_, id)| *id);
        let head = usrspc::read(&mut txn, id, 10, 8).unwrap();
        let tail = usrspc::read(&mut txn, id, pages * 4000, 8).unwrap();
        let text = |b: Vec<u8>| {
            String::from_utf8_lossy(&b)
                .trim_end_matches('\0')
                .trim()
                .to_string()
        };
        assert_eq!(text(head), format!("space {i}"));
        assert_eq!(text(tail), format!("tail {i}"));
    }
    txn.abort();
    let check = store.check_pages().unwrap();
    assert!(check.bad_pages.is_empty(), "{check}");
}

const CRC64: Crc<u64> = Crc::<u64>::new(&CRC_64_XZ);
const CRC32C: Crc<u32> = Crc::<u32>::new(&CRC_32_ISCSI);

/// One stop of a scenario: every unit's flushed image, digested.
fn stop(out: &mut String, name: &str, devs: &[(&str, &Arc<MemBlockDev>)]) {
    writeln!(out, "[{name}]").unwrap();
    for (label, d) in devs {
        let image = d.flushed_image();
        let used = image
            .chunks(4096)
            .filter(|p| p.iter().any(|&b| b != 0))
            .count();
        writeln!(
            out,
            "{label}: {} pages, {used} written, crc64 {:016x}, crc32c {:08x}",
            image.len() / 4096,
            CRC64.checksum(&image),
            CRC32C.checksum(&image)
        )
        .unwrap();
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        write!(s, "{b:02x}").unwrap();
        s
    })
}

fn finish_parity(store: &mut Store) {
    let mut steps = 0;
    while store.parity_step(64).unwrap().is_some() {
        steps += 1;
        assert!(steps < 10_000, "work does not end");
    }
}

fn mirrored(out: &mut String) {
    let (a, b, c, spare) = (dev(1024, 1), dev(1024, 2), dev(1100, 3), dev(1024, 4));
    let (mut store, qsys) = store_with(&[&a], 6, 20);
    let mut journal = MemJournal::default();
    store
        .start_mirror(0, Box::new(b.clone()), [2; 16], "UNIT1")
        .unwrap();
    add_spaces(&mut store, qsys, 6, 9, 20);
    let mut res = store.resume_unit(1, &mut journal).unwrap();
    store.resume_step(&mut res, &mut journal, 2).unwrap();
    store.checkpoint().unwrap();
    stop(
        out,
        "mirror, half resumed",
        &[("unit 0", &a), ("unit 1", &b)],
    );
    writeln!(
        out,
        "resume map: {}",
        hex(&journal.saved.as_ref().expect("saved").encode())
    )
    .unwrap();
    while !store
        .resume_step(&mut res, &mut journal, 8)
        .unwrap()
        .complete
    {}
    assert!(journal.saved.is_none());
    add_spaces(&mut store, qsys, 9, 11, 20);
    store.suspend_unit(1).unwrap();
    add_spaces(&mut store, qsys, 11, 13, 20);
    store.close().unwrap();
    stop(
        out,
        "mirror, one half suspended",
        &[("unit 0", &a), ("unit 1", &b)],
    );

    // Opened again as written: the suspended half replaced, a hot spare.
    let (mut store, rec) = Store::open(boxed(&[&a, &b]), config()).unwrap();
    assert!(rec.events.is_empty(), "{:?}", rec.events);
    assert_eq!(store.unit_state(1), UnitState::Suspended);
    assert_spaces(&mut store, qsys, 13, 20);
    store
        .replace_unit(1, Box::new(c.clone()), [3; 16], "UNIT2")
        .unwrap();
    let mut res = store.resume_unit(1, &mut journal).unwrap();
    while !store
        .resume_step(&mut res, &mut journal, 8)
        .unwrap()
        .complete
    {}
    store
        .start_hot_spare(Box::new(spare.clone()), [4; 16])
        .unwrap();
    add_spaces(&mut store, qsys, 13, 14, 20);
    store.close().unwrap();
    stop(
        out,
        "mirror, half replaced, a hot spare",
        &[("unit 0", &a), ("unit 1", &c), ("spare", &spare)],
    );
    for only in [&a, &c] {
        let (mut s, rec) = Store::open(boxed(&[only, &spare]), config()).unwrap();
        assert!(matches!(rec.events[..], [UnitEvent::Missing { .. }]));
        assert_spaces(&mut s, qsys, 14, 20);
    }
}

fn parity(out: &mut String, level: Level, n: usize, lose: &[usize]) {
    let name = level.to_string();
    let devs: Vec<Arc<MemBlockDev>> = (0..n).map(|i| dev(1200, 10 + i as u64)).collect();
    let all: Vec<&Arc<MemBlockDev>> = devs.iter().collect();
    let labels: Vec<String> = (0..n).map(|i| format!("unit {i}")).collect();
    let labelled = |keep: &dyn Fn(usize) -> bool| -> Vec<(&str, &Arc<MemBlockDev>)> {
        (0..n)
            .filter(|i| keep(*i))
            .map(|i| (labels[i].as_str(), &devs[i]))
            .collect()
    };
    let (mut store, qsys) = store_with(&all, 4, 12);
    let units: Vec<u16> = (0..n).map(|i| u16::try_from(i).unwrap()).collect();
    store.start_parity(level, &units).unwrap();
    add_spaces(&mut store, qsys, 4, 6, 12);
    store.parity_step(64).unwrap();
    store.checkpoint().unwrap();
    stop(out, &format!("{name}, synchronizing"), &labelled(&|_| true));
    finish_parity(&mut store);
    add_spaces(&mut store, qsys, 6, 10, 12);
    assert!(store.parity_verify(1).unwrap().is_empty());
    store.close().unwrap();
    stop(out, &format!("{name}, active"), &labelled(&|_| true));

    // Units missing: exposed, and written to.
    let here: Vec<&Arc<MemBlockDev>> = (0..n)
        .filter(|i| !lose.contains(i))
        .map(|i| &devs[i])
        .collect();
    let (mut store, rec) = Store::open(boxed(&here), config()).unwrap();
    assert_eq!(rec.events.len(), lose.len(), "{:?}", rec.events);
    assert_spaces(&mut store, qsys, 10, 12);
    add_spaces(&mut store, qsys, 10, 13, 12);
    store.close().unwrap();
    stop(
        out,
        &format!("{name}, exposed"),
        &labelled(&|i| !lose.contains(&i)),
    );

    // Back: stale, rebuilt.
    let (mut store, rec) = Store::open(boxed(&all), config()).unwrap();
    assert!(
        rec.events
            .iter()
            .all(|e| matches!(e, UnitEvent::ParityStale { .. })),
        "{:?}",
        rec.events
    );
    store.parity_step(64).unwrap();
    store.checkpoint().unwrap();
    stop(out, &format!("{name}, rebuilding"), &labelled(&|_| true));
    finish_parity(&mut store);
    add_spaces(&mut store, qsys, 13, 14, 12);
    assert!(store.parity_verify(1).unwrap().is_empty());
    assert_spaces(&mut store, qsys, 14, 12);
    store.close().unwrap();
    stop(out, &format!("{name}, rebuilt"), &labelled(&|_| true));
}

#[test]
fn the_units_hold_the_bytes_they_held_before() {
    let release = store_release();
    let mut got = format!("release {release}\n");
    mirrored(&mut got);
    parity(&mut got, Level::Raid5, 3, &[1]);
    parity(&mut got, Level::Raid6, 4, &[0, 3]);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/sets.txt");
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &got).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    if want.lines().next() != got.lines().next() {
        // Another release's headers: other bytes, and nothing to compare.
        eprintln!(
            "sets.txt is of {:?}; this build is {release}: digests not compared",
            want.lines().next()
        );
        return;
    }
    assert_eq!(
        got, want,
        "the units' bytes changed; QS400_UPDATE_GOLDEN=1 rewrites the file after a deliberate format change"
    );
}

/// The release the superblock of a new store records.
fn store_release() -> String {
    let d = dev(256, 99);
    let store = Store::format(boxed(&[&d]), &params(1), config()).unwrap();
    store.asp_release().to_string()
}
