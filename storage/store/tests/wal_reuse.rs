// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A store formatted over units an older store used: the older store's
//! WAL pages carry sequence numbers the new one reaches, and must never
//! be replayed into it.

use std::collections::BTreeMap;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::btree::BTree;
use store::obj::ObjectId;
use store::{FormatParams, Store, StoreConfig};

type Model = BTreeMap<u64, u64>;

fn params(asp_uuid: u8) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [asp_uuid; 16],
        unit_uuids: vec![[asp_uuid; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: 1_767_225_600,
    }
}

fn config() -> StoreConfig {
    StoreConfig {
        cache_pages: 64,
        checkpoint_txns: 1_000_000,
        checkpoint_secs: 1_000_000,
    }
}

fn boxed(d: &Arc<MemBlockDev>) -> Vec<Box<dyn BlockDev>> {
    vec![Box::new(d.clone())]
}

fn insert(store: &mut Store, model: &mut Model, from: u64, n: u64) {
    let mut t = store.begin().unwrap();
    let mut tree: BTree<u64, u64> = BTree::open(t.objdir_root(), ObjectId::NONE);
    for k in from..from + n {
        tree.insert(&mut t, k, &(k * 3)).unwrap();
        model.insert(k, k * 3);
    }
    t.set_root(ObjectId::NONE, tree.root());
    t.commit().unwrap();
}

fn read_all(store: &mut Store) -> Model {
    let root = store.objdir_root();
    let mut t = store.begin().unwrap();
    let tree: BTree<u64, u64> = BTree::open(root, ObjectId::NONE);
    let out = tree.entries(&mut t).unwrap().map(Result::unwrap).collect();
    t.abort();
    out
}

#[test]
fn an_older_stores_wal_is_not_replayed() {
    let d = Arc::new(MemBlockDev::with_seed(8192, 3));
    let mut old = Store::format(boxed(&d), &params(1), config()).unwrap();
    let mut ignored = Model::new();
    for i in 0..40 {
        insert(&mut old, &mut ignored, 10_000 + i * 50, 50);
    }
    old.close().unwrap();
    // A new store on the same unit, with fewer transactions.
    let mut new = Store::format(boxed(&d), &params(2), config()).unwrap();
    let mut model = Model::new();
    for i in 0..3 {
        insert(&mut new, &mut model, i * 10, 10);
    }
    new.close().unwrap();
    let (mut s, rec) = Store::open(boxed(&d), config()).unwrap();
    assert_eq!(rec.replayed, 0);
    assert_eq!(read_all(&mut s), model);
}
