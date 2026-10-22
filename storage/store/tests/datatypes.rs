// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Payload semantics of the simple types, each checked across a power
//! pull (unflushed in-place writes replayed from the WAL).

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue, Decimal};
use store::obj::dtaq::{self, KeyOp, Order, Select};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, sysval, types, usridx, usrspc};
use store::{FormatParams, Store, StoreConfig, StoreError};

fn open_new(pages: u64) -> (Arc<MemBlockDev>, Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(pages, 8));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [6; 16],
        unit_uuids: vec![[7; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let (qsys, lib) = {
        let mut t = store.begin().unwrap();
        let q = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        let l = ops::create(&mut t, &params_for(q, "TESTLIB", types::LIB)).unwrap();
        t.commit().unwrap();
        (q, l)
    };
    (d, store, qsys, lib)
}

fn params_for(library: ObjectId, n: &str, type_code: u16) -> CreateParams {
    CreateParams {
        library,
        name: Name::parse(n).unwrap(),
        type_code,
        attribute: String::new(),
        text: String::new(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: 1_767_225_700,
        by_system: false,
    }
}

/// Power pull and reopen: in-place work since the last flush must come
/// back from the WAL.
fn pull(d: &Arc<MemBlockDev>, store: Store) -> Store {
    drop(store);
    d.drop_unflushed();
    Store::open(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0
}

#[test]
fn data_areas() {
    let (d, mut store, _qsys, lib) = open_new(4096);
    let (c, n, l) = {
        let mut t = store.begin().unwrap();
        let c = dtaara::create(
            &mut t,
            &params_for(lib, "CHR", 0),
            DataAreaType::Char { len: 2000 },
            &DataAreaValue::Char("hello".into()),
        )
        .unwrap();
        let n = dtaara::create(
            &mut t,
            &params_for(lib, "NUM", 0),
            DataAreaType::Dec {
                digits: 15,
                decimals: 5,
            },
            &DataAreaValue::Dec(Decimal::parse("-123.45", 15, 5).unwrap().scaled),
        )
        .unwrap();
        let l = dtaara::create(
            &mut t,
            &params_for(lib, "LGL", 0),
            DataAreaType::Lgl,
            &DataAreaValue::Lgl(true),
        )
        .unwrap();
        assert!(matches!(
            dtaara::create(
                &mut t,
                &params_for(lib, "BAD", 0),
                DataAreaType::Char { len: 0 },
                &DataAreaValue::Char(String::new())
            ),
            Err(StoreError::BadValue { .. })
        ));
        t.commit().unwrap();
        (c, n, l)
    };
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(
        dtaara::read(&mut t, c).unwrap(),
        DataAreaValue::Char(format!("hello{}", " ".repeat(1995)))
    );
    assert_eq!(
        dtaara::read(&mut t, n).unwrap(),
        DataAreaValue::Dec(-12_345_000)
    );
    assert_eq!(dtaara::read(&mut t, l).unwrap(), DataAreaValue::Lgl(true));
    assert_eq!(
        dtaara::describe(&mut t, c).unwrap(),
        (DataAreaType::Char { len: 2000 }, false)
    );
    // Small values live in the header; a 2000-byte one needs a page.
    assert_eq!(ops::describe(&mut t, n).unwrap().size_pages, 0);
    assert_eq!(ops::describe(&mut t, l).unwrap().size_pages, 0);
    assert_eq!(ops::describe(&mut t, c).unwrap().size_pages, 1);
    let small = dtaara::create(
        &mut t,
        &params_for(lib, "SMALL", 0),
        DataAreaType::Char { len: 1008 },
        &DataAreaValue::Char("edge".into()),
    )
    .unwrap();
    assert_eq!(ops::describe(&mut t, small).unwrap().size_pages, 0);
    dtaara::write_substring(&mut t, small, 1005, 4, "last").unwrap();
    let v = dtaara::read(&mut t, small).unwrap();
    assert!(
        matches!(&v, DataAreaValue::Char(s) if s.starts_with("edge") && s.ends_with("last") && s.len() == 1008)
    );
    // Substring, whole write, type mismatch, overflow.
    dtaara::write_substring(&mut t, c, 7, 5, "world").unwrap();
    assert_eq!(
        dtaara::read(&mut t, c).unwrap(),
        DataAreaValue::Char(format!("hello world{}", " ".repeat(1989)))
    );
    assert!(matches!(
        dtaara::write_substring(&mut t, c, 2000, 2, "xx"),
        Err(StoreError::BadValue { .. })
    ));
    assert!(matches!(
        dtaara::write(&mut t, c, &DataAreaValue::Lgl(false)),
        Err(StoreError::BadValue { .. })
    ));
    assert!(matches!(
        dtaara::write(&mut t, n, &DataAreaValue::Dec(10i128.pow(15))),
        Err(StoreError::BadValue { .. })
    ));
    dtaara::write(&mut t, n, &DataAreaValue::Dec(7)).unwrap();
    dtaara::write(&mut t, l, &DataAreaValue::Lgl(false)).unwrap();
    dtaara::set_locked(&mut t, c, true).unwrap();
    t.commit().unwrap();
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(dtaara::read(&mut t, n).unwrap(), DataAreaValue::Dec(7));
    assert_eq!(dtaara::read(&mut t, l).unwrap(), DataAreaValue::Lgl(false));
    assert!(dtaara::describe(&mut t, c).unwrap().1);
    assert_eq!(
        dtaara::read(&mut t, c).unwrap(),
        DataAreaValue::Char(format!("hello world{}", " ".repeat(1989)))
    );
    ops::delete(&mut t, JobHandle(1), c).unwrap();
    assert!(matches!(
        dtaara::read(&mut t, c),
        Err(StoreError::StaleReference { .. })
    ));
    t.commit().unwrap();
}

#[test]
fn data_queues_fifo_lifo_keyed() {
    let (d, mut store, _qsys, lib) = open_new(8192);
    let (fifo, lifo, keyed, usrq) = {
        let mut t = store.begin().unwrap();
        let fifo = dtaq::create(
            &mut t,
            &params_for(lib, "FIFO", 0),
            types::DTAQ,
            Order::Fifo,
            100,
            true,
        )
        .unwrap();
        let lifo = dtaq::create(
            &mut t,
            &params_for(lib, "LIFO", 0),
            types::DTAQ,
            Order::Lifo,
            64512,
            false,
        )
        .unwrap();
        let keyed = dtaq::create(
            &mut t,
            &params_for(lib, "KEYED", 0),
            types::DTAQ,
            Order::Keyed { key_len: 3 },
            50,
            false,
        )
        .unwrap();
        let usrq = dtaq::create(
            &mut t,
            &params_for(lib, "UQ", 0),
            types::USRQ,
            Order::Fifo,
            10,
            true,
        )
        .unwrap();
        assert!(
            dtaq::create(
                &mut t,
                &params_for(lib, "BAD", 0),
                types::DTAQ,
                Order::Keyed { key_len: 0 },
                10,
                false
            )
            .is_err()
        );
        assert!(
            dtaq::create(
                &mut t,
                &params_for(lib, "BAD", 0),
                types::DTAQ,
                Order::Fifo,
                0,
                false
            )
            .is_err()
        );
        t.commit().unwrap();
        (fifo, lifo, keyed, usrq)
    };
    let sender = [0xC1u8; 26];
    {
        let mut t = store.begin().unwrap();
        for i in 0..5u8 {
            dtaq::send(&mut t, fifo, &[], &[i; 10], Some(sender)).unwrap();
        }
        assert!(matches!(
            dtaq::send(&mut t, fifo, &[], &[0; 101], None),
            Err(StoreError::BadValue { .. })
        ));
        assert!(matches!(
            dtaq::send(&mut t, fifo, b"k", &[0; 1], None),
            Err(StoreError::BadValue { .. })
        ));
        // A big LIFO entry spans pages.
        let big = vec![0x5A; 64512];
        dtaq::send(&mut t, lifo, &[], &big, None).unwrap();
        dtaq::send(&mut t, lifo, &[], b"small", None).unwrap();
        let keyed_sends: [(&[u8], &[u8]); 4] = [
            (b"BBB", b"two"),
            (b"AAA", b"one"),
            (b"CCC", b"three"),
            (b"BBB", b"two again"),
        ];
        for (k, v) in keyed_sends {
            dtaq::send(&mut t, keyed, k, v, None).unwrap();
        }
        dtaq::send(&mut t, usrq, &[], b"u", Some(sender)).unwrap();
        t.commit().unwrap();
    }
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(dtaq::entry_count(&mut t, fifo).unwrap(), 5);
    let e = dtaq::receive(&mut t, fifo, &Select::Next).unwrap().unwrap();
    assert_eq!((e.seq, e.data, e.sender), (1, vec![0; 10], sender));
    let e = dtaq::receive(&mut t, fifo, &Select::Next).unwrap().unwrap();
    assert_eq!(e.data, vec![1; 10]);
    assert_eq!(
        dtaq::receive(&mut t, lifo, &Select::Next)
            .unwrap()
            .unwrap()
            .data,
        b"small"
    );
    let big = dtaq::receive(&mut t, lifo, &Select::Next).unwrap().unwrap();
    assert_eq!(big.data.len(), 64512);
    assert!(big.data.iter().all(|&b| b == 0x5A));
    assert!(
        dtaq::receive(&mut t, lifo, &Select::Next)
            .unwrap()
            .is_none()
    );
    let a = dtaq::describe(&mut t, lifo).unwrap();
    assert_eq!(
        (a.head, a.tail, a.entry_count),
        (0, 0, 0),
        "emptied queue resets"
    );
    // Keyed: EQ takes the oldest BBB; GT takes CCC; LT takes AAA; NE
    // with no other key matches nothing; EQ again takes the second BBB.
    assert_eq!(
        dtaq::receive(&mut t, keyed, &Select::Key(KeyOp::Eq, b"BBB"))
            .unwrap()
            .unwrap()
            .data,
        b"two"
    );
    assert_eq!(
        dtaq::receive(&mut t, keyed, &Select::Key(KeyOp::Gt, b"BBB"))
            .unwrap()
            .unwrap()
            .data,
        b"three"
    );
    assert_eq!(
        dtaq::receive(&mut t, keyed, &Select::Key(KeyOp::Lt, b"BBB"))
            .unwrap()
            .unwrap()
            .data,
        b"one"
    );
    assert!(
        dtaq::receive(&mut t, keyed, &Select::Key(KeyOp::Ne, b"BBB"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        dtaq::receive(&mut t, keyed, &Select::Key(KeyOp::Eq, b"BBB"))
            .unwrap()
            .unwrap()
            .data,
        b"two again"
    );
    assert!(matches!(
        dtaq::receive(&mut t, fifo, &Select::Key(KeyOp::Eq, b"x")),
        Err(StoreError::BadValue { .. })
    ));
    let u = dtaq::receive(&mut t, usrq, &Select::Next).unwrap().unwrap();
    assert_eq!(u.sender, [0x40; 26], "user queues carry no sender");
    t.commit().unwrap();
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(dtaq::entry_count(&mut t, fifo).unwrap(), 3);
    assert_eq!(
        dtaq::receive(&mut t, fifo, &Select::Next)
            .unwrap()
            .unwrap()
            .data,
        vec![2; 10]
    );
    dtaq::clear(&mut t, fifo).unwrap();
    assert!(
        dtaq::receive(&mut t, fifo, &Select::Next)
            .unwrap()
            .is_none()
    );
    // Many sends grow the space; draining resets it.
    for i in 0..500u32 {
        dtaq::send(&mut t, fifo, &[], &i.to_le_bytes(), None).unwrap();
    }
    let a = dtaq::describe(&mut t, fifo).unwrap();
    assert!(a.space_bytes >= 500 * 56);
    for i in 0..500u32 {
        assert_eq!(
            dtaq::receive(&mut t, fifo, &Select::Next)
                .unwrap()
                .unwrap()
                .data,
            i.to_le_bytes()
        );
    }
    assert_eq!(dtaq::describe(&mut t, fifo).unwrap().tail, 0);
    t.commit().unwrap();
}

#[test]
fn user_spaces() {
    let (d, mut store, _qsys, lib) = open_new(8192);
    let (fixed, growing) = {
        let mut t = store.begin().unwrap();
        let fixed =
            usrspc::create(&mut t, &params_for(lib, "FIXED", 0), 10_000, false, 0x00).unwrap();
        let growing = usrspc::create(&mut t, &params_for(lib, "GROW", 0), 100, true, 0x40).unwrap();
        t.commit().unwrap();
        (fixed, growing)
    };
    {
        let mut t = store.begin().unwrap();
        assert_eq!(
            usrspc::read(&mut t, fixed, 0, 10_000).unwrap(),
            vec![0; 10_000]
        );
        usrspc::write(&mut t, fixed, 4060, b"across a page boundary").unwrap();
        assert!(matches!(
            usrspc::write(&mut t, fixed, 9_990, &[1; 20]),
            Err(StoreError::BadValue { .. })
        ));
        assert!(matches!(
            usrspc::read(&mut t, fixed, 10_000, 1),
            Err(StoreError::BadValue { .. })
        ));
        usrspc::write(&mut t, growing, 95, &[7; 10]).unwrap();
        assert_eq!(usrspc::describe(&mut t, growing).unwrap().size, 105);
        usrspc::write(&mut t, growing, 20_000, b"far").unwrap();
        assert_eq!(
            usrspc::read(&mut t, growing, 105, 5).unwrap(),
            vec![0x40; 5],
            "initial value fills growth"
        );
        t.commit().unwrap();
    }
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(
        usrspc::read(&mut t, fixed, 4060, 22).unwrap(),
        b"across a page boundary"
    );
    assert_eq!(usrspc::read(&mut t, growing, 20_000, 3).unwrap(), b"far");
    let before = ops::describe(&mut t, growing).unwrap().size_pages;
    usrspc::change_size(&mut t, growing, 50).unwrap();
    assert_eq!(usrspc::describe(&mut t, growing).unwrap().size, 50);
    assert!(ops::describe(&mut t, growing).unwrap().size_pages < before);
    assert_eq!(
        usrspc::read(&mut t, growing, 0, 50).unwrap()[..30],
        [0x40; 30][..]
    );
    assert!(matches!(
        usrspc::read(&mut t, growing, 50, 1),
        Err(StoreError::BadValue { .. })
    ));
    assert!(matches!(
        usrspc::change_size(&mut t, growing, usrspc::MAX_SIZE + 1),
        Err(StoreError::BadValue { .. })
    ));
    t.commit().unwrap();
}

/// A user space as the test expects it, byte for byte.
fn expect_space(t: &mut store::Txn<'_>, id: ObjectId, model: &[u8]) {
    let got = usrspc::read(t, id, 0, model.len()).unwrap();
    assert!(got == model, "user space differs from the model");
}

#[test]
fn user_space_whole_pages_relocate() {
    use store::obj::space::{MAX_EXTENTS, PAGE_BYTES, RELOCATE_MIN_PAGES};
    let (d, mut store, _qsys, lib) = open_new(32768);
    let size = 1 << 20;
    let mut model = vec![0u8; size];
    let id = {
        let mut t = store.begin().unwrap();
        let id =
            usrspc::create(&mut t, &params_for(lib, "BIG", 0), size as u64, false, 0).unwrap();
        t.commit().unwrap();
        id
    };
    let extents_of = |t: &mut store::Txn<'_>| {
        let h = ops::describe(t, id).unwrap();
        (h.size_pages, ops::all_extents(t, &h).unwrap())
    };
    // An unaligned write over 600 KiB: a patched head and tail around
    // whole pages that move.
    {
        let mut t = store.begin().unwrap();
        let (pages, before) = extents_of(&mut t);
        let data: Vec<u8> = (0..600 * 1024u32)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        usrspc::write(&mut t, id, 100, &data).unwrap();
        model[100..100 + data.len()].copy_from_slice(&data);
        let (pages_after, after) = extents_of(&mut t);
        assert_eq!(pages, pages_after, "the space keeps its size");
        assert_ne!(before, after, "the whole pages moved");
        assert!(after.len() >= 3);
        expect_space(&mut t, id, &model);
        t.commit().unwrap();
    }
    let mut store = pull(&d, store);
    {
        let mut t = store.begin().unwrap();
        expect_space(&mut t, id, &model);
        // An aborted relocation leaves nothing behind.
        usrspc::write(&mut t, id, 0, &vec![0xEE; 100 * 1024]).unwrap();
        t.abort();
    }
    let mut store = pull(&d, store);
    {
        let mut t = store.begin().unwrap();
        expect_space(&mut t, id, &model);
        t.commit().unwrap();
    }
    let report = store.reclaim(store::ReclaimMode::Check, 1_767_225_800).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
    // Short whole-page spans are patched, not moved.
    {
        let mut t = store.begin().unwrap();
        let (_, before) = extents_of(&mut t);
        let span = usize::try_from((RELOCATE_MIN_PAGES - 1) * PAGE_BYTES).unwrap();
        let at = usize::try_from(10 * PAGE_BYTES).unwrap();
        usrspc::write(&mut t, id, at as u64, &vec![0x11; span]).unwrap();
        model[at..at + span].fill(0x11);
        assert_eq!(before, extents_of(&mut t).1);
        expect_space(&mut t, id, &model);
        t.commit().unwrap();
    }
    // Scattered writes fragment the list until it is full; then the
    // writes are patched and everything still reads back.
    let big = 12 << 20;
    let mut big_model = vec![0u8; big];
    let big_id = {
        let mut t = store.begin().unwrap();
        let id = usrspc::create(&mut t, &params_for(lib, "BIGGER", 0), big as u64, false, 0)
            .unwrap();
        t.commit().unwrap();
        id
    };
    let span = usize::try_from(RELOCATE_MIN_PAGES * PAGE_BYTES).unwrap();
    let stride = usize::try_from(8 * PAGE_BYTES).unwrap();
    let mut most = 0;
    for (n, at) in (0..big - span).step_by(stride).enumerate() {
        let mut t = store.begin().unwrap();
        let fill = u8::try_from(n % 200).unwrap() + 1;
        usrspc::write(&mut t, big_id, at as u64, &vec![fill; span]).unwrap();
        big_model[at..at + span].fill(fill);
        let h = ops::describe(&mut t, big_id).unwrap();
        let count = ops::all_extents(&mut t, &h).unwrap().len();
        assert!(count <= MAX_EXTENTS);
        most = most.max(count);
        t.commit().unwrap();
    }
    assert!(most + 2 > MAX_EXTENTS, "the list filled up: {most}");
    let mut store = pull(&d, store);
    {
        let mut t = store.begin().unwrap();
        expect_space(&mut t, big_id, &big_model);
        expect_space(&mut t, id, &model);
        t.commit().unwrap();
    }
    let report = store.reclaim(store::ReclaimMode::Check, 1_767_225_900).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
}

#[test]
fn user_indexes() {
    let (d, mut store, _qsys, lib) = open_new(8192);
    let idx = {
        let mut t = store.begin().unwrap();
        let idx = usridx::create(&mut t, &params_for(lib, "IDX", 0), false, 4, 40).unwrap();
        assert!(usridx::create(&mut t, &params_for(lib, "BAD", 0), true, 0, 10).is_err());
        assert!(usridx::create(&mut t, &params_for(lib, "BAD", 0), true, 10, 4).is_err());
        for k in ["K003", "K001", "K005", "K002", "K004"] {
            usridx::insert(&mut t, idx, format!("{k}data").as_bytes()).unwrap();
        }
        usridx::insert(&mut t, idx, b"K003more").unwrap();
        assert!(matches!(
            usridx::insert(&mut t, idx, b"K003more"),
            Err(StoreError::AlreadyExists { .. })
        ));
        assert!(matches!(
            usridx::insert(&mut t, idx, b"K0"),
            Err(StoreError::BadValue { .. })
        ));
        t.commit().unwrap();
        idx
    };
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    let s = |v: Vec<Vec<u8>>| -> Vec<String> {
        v.into_iter()
            .map(|e| String::from_utf8(e).unwrap())
            .collect()
    };
    assert_eq!(usridx::describe(&mut t, idx).unwrap().entry_count, 6);
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Eq, b"K003", 10).unwrap()),
        vec!["K003data", "K003more"]
    );
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Ge, b"K004", 10).unwrap()),
        vec!["K004data", "K005data"]
    );
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Gt, b"K003", 1).unwrap()),
        vec!["K004data"]
    );
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Le, b"K002", 10).unwrap()),
        vec!["K002data", "K001data"]
    );
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Lt, b"K003", 10).unwrap()),
        vec!["K002data", "K001data"]
    );
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Ne, b"K003", 2).unwrap()),
        vec!["K001data", "K002data"]
    );
    assert_eq!(
        s(usridx::first(&mut t, idx, 2).unwrap()),
        vec!["K001data", "K002data"]
    );
    assert_eq!(
        s(usridx::last(&mut t, idx, 2).unwrap()),
        vec!["K005data", "K004data"]
    );
    assert!(matches!(
        usridx::find(&mut t, idx, KeyOp::Eq, b"K", 1),
        Err(StoreError::BadValue { .. })
    ));
    assert_eq!(
        s(usridx::remove(&mut t, idx, KeyOp::Eq, b"K003", 1).unwrap()),
        vec!["K003data"]
    );
    assert_eq!(usridx::describe(&mut t, idx).unwrap().entry_count, 5);
    t.commit().unwrap();
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(
        s(usridx::find(&mut t, idx, KeyOp::Eq, b"K003", 10).unwrap()),
        vec!["K003more"]
    );
    t.commit().unwrap();
    // Pages freed by copy-on-write come back only at a checkpoint, so
    // bulk inserts go in batches.
    for batch in 0..6u32 {
        let mut t = store.begin().unwrap();
        for i in batch * 500..(batch + 1) * 500 {
            usridx::insert(&mut t, idx, format!("{i:04}{}", "x".repeat(20)).as_bytes()).unwrap();
        }
        t.commit().unwrap();
        store.checkpoint().unwrap();
    }
    let mut t = store.begin().unwrap();
    assert_eq!(usridx::first(&mut t, idx, 1).unwrap()[0][..4], b"0000"[..]);
    assert_eq!(usridx::last(&mut t, idx, 1).unwrap()[0][..4], b"K005"[..]);
    assert_eq!(usridx::describe(&mut t, idx).unwrap().entry_count, 3005);
    usridx::drop_entries(&mut t, idx).unwrap();
    ops::delete(&mut t, JobHandle(1), idx).unwrap();
    t.commit().unwrap();
}

#[test]
fn user_index_long_entries() {
    let (d, mut store, _qsys, lib) = open_new(8192);
    let idx = {
        let mut t = store.begin().unwrap();
        assert!(matches!(
            usridx::create(&mut t, &params_for(lib, "TOOLONG", 0), false, 10, 2001),
            Err(StoreError::BadValue { .. })
        ));
        let idx = usridx::create(&mut t, &params_for(lib, "LONG", 0), false, 10, 2000).unwrap();
        t.commit().unwrap();
        idx
    };
    let entry = |key: &str, len: usize, fill: u8| -> Vec<u8> {
        let mut e = format!("{key:<10}").into_bytes();
        e.resize(len, fill);
        e
    };
    let (a, b, c) = (
        entry("ALPHA", 2000, b'a'),
        entry("BETA", 1500, b'b'),
        entry("GAMMA", 40, b'c'),
    );
    {
        let mut t = store.begin().unwrap();
        usridx::insert(&mut t, idx, &a).unwrap();
        usridx::insert(&mut t, idx, &b).unwrap();
        usridx::insert(&mut t, idx, &c).unwrap();
        assert!(matches!(
            usridx::insert(&mut t, idx, &a),
            Err(StoreError::AlreadyExists { .. })
        ));
        assert_eq!(
            usridx::find(&mut t, idx, KeyOp::Eq, b"ALPHA     ", 1).unwrap(),
            vec![a.clone()]
        );
        t.commit().unwrap();
    }
    let mut store = pull(&d, store);
    {
        let mut t = store.begin().unwrap();
        assert_eq!(
            usridx::first(&mut t, idx, 3).unwrap(),
            vec![a.clone(), b.clone(), c.clone()]
        );
        assert_eq!(usridx::last(&mut t, idx, 1).unwrap(), vec![c.clone()]);
        assert_eq!(
            usridx::find(&mut t, idx, KeyOp::Ge, b"BETA      ", 10).unwrap(),
            vec![b.clone(), c.clone()]
        );
        t.commit().unwrap();
    }
    let report = store.reclaim(store::ReclaimMode::Check, 1_767_225_800).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
    {
        let mut t = store.begin().unwrap();
        let free = t.store().asp().total_free();
        assert_eq!(
            usridx::remove(&mut t, idx, KeyOp::Eq, b"BETA      ", 1).unwrap(),
            vec![b.clone()]
        );
        t.commit().unwrap();
        store.checkpoint().unwrap();
        assert!(store.asp().total_free() > free, "the overflow page came back");
    }
    let report = store.reclaim(store::ReclaimMode::Check, 1_767_225_900).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
    let mut t = store.begin().unwrap();
    assert_eq!(usridx::describe(&mut t, idx).unwrap().entry_count, 2);
    usridx::drop_entries(&mut t, idx).unwrap();
    ops::delete(&mut t, JobHandle(1), idx).unwrap();
    t.commit().unwrap();
    store.checkpoint().unwrap();
    let report = store.reclaim(store::ReclaimMode::Check, 1_767_226_000).unwrap();
    assert_eq!(report.problems(), 0, "{report}");
}

#[test]
fn sysval_object() {
    let (d, mut store, qsys, _lib) = open_new(4096);
    let id = {
        let mut t = store.begin().unwrap();
        let id = sysval::create(&mut t, qsys, 1_767_225_600, "QS400", b"tlv-blob-v1").unwrap();
        assert!(matches!(
            sysval::create(&mut t, qsys, 1, "QS400", b""),
            Err(StoreError::AlreadyExists { .. })
        ));
        t.commit().unwrap();
        id
    };
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(sysval::read(&mut t, id).unwrap(), b"tlv-blob-v1");
    let big = vec![9u8; 20_000];
    sysval::write(&mut t, id, &big).unwrap();
    t.commit().unwrap();
    let mut store = pull(&d, store);
    let mut t = store.begin().unwrap();
    assert_eq!(sysval::read(&mut t, id).unwrap(), big);
    sysval::write(&mut t, id, b"").unwrap();
    assert!(sysval::read(&mut t, id).unwrap().is_empty());
    assert_eq!(
        ops::describe(&mut t, id).unwrap().name,
        Name::parse("QSYSVAL").unwrap()
    );
    t.abort();
}
