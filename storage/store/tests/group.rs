// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Group commit: transactions committed in a group are seen at once,
//! become durable together at `end_group` with a few flushes, and a
//! power pull before or during the flush leaves the state before the
//! group or after it, never part of it.

use std::collections::BTreeMap;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::btree::BTree;
use store::obj::ObjectId;
use store::{FormatParams, Store, StoreConfig};

type Model = BTreeMap<u64, u64>;

fn params(wal_pages: u64) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [9; 16],
        unit_uuids: vec![[1; 16]],
        unit_serials: Vec::new(),
        wal_pages,
        now: 1_767_225_600,
    }
}

/// A cache of a few pages, so reads in a group miss it.
fn config() -> StoreConfig {
    StoreConfig {
        cache_pages: 4,
        checkpoint_txns: 1_000_000,
        checkpoint_secs: 30,
    }
}

fn boxed(d: &Arc<MemBlockDev>) -> Vec<Box<dyn BlockDev>> {
    vec![Box::new(d.clone())]
}

fn read_all(store: &mut Store) -> Model {
    let root = store.objdir_root();
    let mut t = store.begin().unwrap();
    let tree: BTree<u64, u64> = BTree::open(root, ObjectId::NONE);
    let out = tree.entries(&mut t).unwrap().map(Result::unwrap).collect();
    t.abort();
    out
}

/// One transaction: insert `n` keys from `from`.
fn insert(store: &mut Store, model: &mut Model, from: u64, n: u64) {
    let mut t = store.begin().unwrap();
    let mut tree: BTree<u64, u64> = BTree::open(t.objdir_root(), ObjectId::NONE);
    for k in from..from + n {
        tree.insert(&mut t, k, &(k * 7)).unwrap();
        model.insert(k, k * 7);
    }
    t.set_root(ObjectId::NONE, tree.root());
    t.commit().unwrap();
}

/// A store with some committed keys.
fn seeded(d: &Arc<MemBlockDev>) -> (Store, Model) {
    let mut store = Store::format(boxed(d), &params(64), config()).unwrap();
    let mut model = Model::new();
    for i in 0..5 {
        insert(&mut store, &mut model, i * 40, 40);
    }
    (store, model)
}

#[test]
fn a_group_is_seen_at_once_and_durable_at_its_end() {
    let d = Arc::new(MemBlockDev::with_seed(4096, 1));
    let (mut store, before) = seeded(&d);
    let mut model = before.clone();
    store.begin_group();
    for i in 0..30 {
        insert(&mut store, &mut model, 1000 + i * 25, 25);
        assert_eq!(read_all(&mut store), model, "in the group, commit {i}");
    }
    // A power pull now: nothing of the group was durable.
    drop(store);
    d.drop_unflushed();
    let (mut store, _) = Store::open(boxed(&d), config()).unwrap();
    assert_eq!(read_all(&mut store), before);
    // Again, ended this time: all of it is durable.
    let mut model = before.clone();
    store.begin_group();
    for i in 0..30 {
        insert(&mut store, &mut model, 1000 + i * 25, 25);
    }
    store.end_group().unwrap();
    drop(store);
    d.drop_unflushed();
    let (mut store, rec) = Store::open(boxed(&d), config()).unwrap();
    assert!(rec.replayed >= 1);
    assert_eq!(read_all(&mut store), model);
}

#[test]
fn a_group_flushes_a_few_times_not_per_commit() {
    let flushes = |grouped: bool| {
        let d = Arc::new(MemBlockDev::with_seed(4096, 2));
        let (mut store, mut model) = seeded(&d);
        let start = d.stats().flushes;
        if grouped {
            store.begin_group();
        }
        for i in 0..20 {
            insert(&mut store, &mut model, 2000 + i * 10, 10);
        }
        if grouped {
            store.end_group().unwrap();
        }
        d.stats().flushes - start
    };
    let alone = flushes(false);
    let grouped = flushes(true);
    assert!(alone >= 40, "{alone} flushes for 20 commits");
    assert!(grouped <= 4, "{grouped} flushes for a group of 20");
}

#[test]
fn a_pull_while_the_group_flushes_keeps_all_or_nothing() {
    for k in 1..40 {
        let d = Arc::new(MemBlockDev::with_seed(4096, 3 + k));
        let (mut store, before) = seeded(&d);
        let mut model = before.clone();
        store.begin_group();
        for i in 0..12 {
            insert(&mut store, &mut model, 5000 + i * 30, 30);
        }
        // The k-th write from here fails: the power goes.
        let done = d.stats().writes;
        d.fail_after(Some(done + k));
        let ended = store.end_group();
        drop(store);
        d.fail_after(None);
        d.drop_unflushed();
        let (mut store, _) = Store::open(boxed(&d), config())
            .unwrap_or_else(|e| panic!("reopen after write {k}: {e}"));
        let got = read_all(&mut store);
        if ended.is_ok() {
            assert_eq!(got, model, "write {k}: the group ended");
        } else {
            assert!(got == before || got == model, "write {k}: part of the group");
        }
    }
}

#[test]
fn a_checkpoint_in_a_group_makes_it_durable_first() {
    let d = Arc::new(MemBlockDev::with_seed(4096, 4));
    let (mut store, mut model) = seeded(&d);
    store.begin_group();
    for i in 0..10 {
        insert(&mut store, &mut model, 7000 + i * 20, 20);
    }
    store.checkpoint().unwrap();
    insert(&mut store, &mut model, 9000, 5);
    store.end_group().unwrap();
    store.close().unwrap();
    let (mut store, rec) = Store::open(boxed(&d), config()).unwrap();
    assert_eq!(rec.replayed, 0);
    assert_eq!(read_all(&mut store), model);
}
