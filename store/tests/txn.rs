// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Transactions, the WAL and recovery: committed work survives a power
//! pull at any point, uncommitted work never appears, and a torn WAL
//! write is a clean rollback.

use std::collections::BTreeMap;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use store::btree::BTree;
use store::obj::ObjectId;
use store::{FormatParams, Pager, Store, StoreConfig, StoreError};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn params(wal_pages: u64) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: [9; 16],
        unit_uuids: vec![[1; 16]],
        wal_pages,
        now: 1_767_225_600,
    }
}

fn config(checkpoint_txns: u64) -> StoreConfig {
    StoreConfig {
        cache_pages: 64,
        checkpoint_txns,
        checkpoint_secs: 30,
    }
}

fn dev(pages: u64, seed: u64) -> Arc<MemBlockDev> {
    Arc::new(MemBlockDev::with_seed(pages, seed))
}

fn boxed(d: &Arc<MemBlockDev>) -> Vec<Box<dyn BlockDev>> {
    vec![Box::new(d.clone())]
}

type Model = BTreeMap<u64, u64>;

/// The object directory tree as the store sees it now.
fn read_all(store: &mut Store) -> Model {
    let root = store.objdir_root();
    let mut t = store.begin().unwrap();
    let tree: BTree<u64, u64> = BTree::open(root, ObjectId::NONE);
    let out = tree.entries(&mut t).unwrap().map(Result::unwrap).collect();
    t.abort();
    out
}

/// One transaction: a few tree changes and, for `data_page`, a patch.
fn run_txn(
    store: &mut Store,
    rng: &mut Rng,
    model: &mut Model,
    data_page: u64,
    data: &mut [u8; 64],
) -> Result<(), StoreError> {
    let mut t = store.begin()?;
    let mut tree: BTree<u64, u64> = BTree::open(t.objdir_root(), ObjectId::NONE);
    let mut local = model.clone();
    for _ in 0..=rng.below(4) {
        let k = rng.below(500);
        if rng.below(3) == 0 {
            tree.delete(&mut t, &k)?;
            local.remove(&k);
        } else {
            let v = rng.next();
            tree.insert(&mut t, k, &v)?;
            local.insert(k, v);
        }
    }
    t.set_root(ObjectId::NONE, tree.root());
    let off = usize::try_from(rng.below(60)).unwrap();
    let byte = u8::try_from(rng.below(256)).unwrap();
    t.patch(data_page, off, &[byte])?;
    let mut local_data = *data;
    local_data[off] = byte;
    t.commit()?;
    *model = local;
    *data = local_data;
    Ok(())
}

fn data_page_bytes(store: &mut Store, page: u64) -> [u8; 64] {
    store.read_page(page).unwrap().body()[..64]
        .try_into()
        .unwrap()
}

#[test]
fn committed_work_survives_power_pulls_between_commits() {
    let d = dev(4096, 1);
    let mut rng = Rng(3);
    let mut store = Store::format(boxed(&d), &params(64), config(16)).unwrap();
    // One data page for in-place changes, allocated and zeroed in its
    // own transaction.
    let data_page = {
        let mut t = store.begin().unwrap();
        let p = t.alloc().unwrap();
        let mut page = store::Page::zeroed();
        t.write(p, &mut page, store::PageType::ObjectData, ObjectId::NONE)
            .unwrap();
        t.commit().unwrap();
        p
    };
    let mut model = Model::new();
    let mut data = [0u8; 64];
    let mut total_replayed = 0;
    for round in 0..40 {
        for _ in 0..=rng.below(20) {
            run_txn(&mut store, &mut rng, &mut model, data_page, &mut data).unwrap();
        }
        // Start a transaction and leave it uncommitted: it must vanish.
        {
            let mut t = store.begin().unwrap();
            let mut tree: BTree<u64, u64> = BTree::open(t.objdir_root(), ObjectId::NONE);
            tree.insert(&mut t, 9_999_999, &1).unwrap();
            t.set_root(ObjectId::NONE, tree.root());
            t.patch(data_page, 63, &[0xEE]).unwrap();
            if round % 2 == 0 {
                drop(t);
            } else {
                t.abort();
            }
        }
        drop(store);
        d.drop_unflushed();
        let (s, rec) = Store::open(boxed(&d), config(16)).unwrap();
        store = s;
        total_replayed += rec.replayed;
        assert_eq!(read_all(&mut store), model, "round {round}");
        assert_eq!(
            data_page_bytes(&mut store, data_page),
            data,
            "round {round}"
        );
        assert!(!model.contains_key(&9_999_999));
    }
    assert!(total_replayed > 0, "some rounds replayed something");
    // Clean close: nothing to replay.
    store.close().unwrap();
    let (mut s, rec) = Store::open(boxed(&d), config(16)).unwrap();
    assert_eq!(rec.replayed, 0);
    assert_eq!(read_all(&mut s), model);
}

#[test]
fn torn_commit_writes_roll_back_or_complete() {
    let d = dev(2048, 7);
    let mut rng = Rng(11);
    let mut store = Store::format(boxed(&d), &params(64), config(1000)).unwrap();
    let data_page = {
        let mut t = store.begin().unwrap();
        let p = t.alloc().unwrap();
        let mut page = store::Page::zeroed();
        t.write(p, &mut page, store::PageType::ObjectData, ObjectId::NONE)
            .unwrap();
        t.commit().unwrap();
        p
    };
    let mut model = Model::new();
    let mut data = [0u8; 64];
    for _ in 0..10 {
        run_txn(&mut store, &mut rng, &mut model, data_page, &mut data).unwrap();
    }
    store.checkpoint().unwrap();
    // Count the writes one more transaction makes.
    let w0 = d.stats().writes;
    let mut probe_model = model.clone();
    let mut probe_data = data;
    let mut probe_rng = Rng(99);
    run_txn(
        &mut store,
        &mut probe_rng,
        &mut probe_model,
        data_page,
        &mut probe_data,
    )
    .unwrap();
    let n = d.stats().writes - w0;
    assert!(n >= 2, "CoW pages and a WAL page at least");
    drop(store);
    // Replay that transaction from the same state with write k torn and
    // the power pulled right after.
    for k in 1..=n {
        let (mut store, _) = Store::open(boxed(&d), config(1000)).unwrap();
        // Undo the probe by reopening from the checkpoint state: the
        // store checkpoints at open, so rebuild the checkpoint state
        // instead by restoring the model and the page through a txn.
        {
            let mut t = store.begin().unwrap();
            let mut tree: BTree<u64, u64> = BTree::open(t.objdir_root(), ObjectId::NONE);
            for (key, v) in &model {
                tree.insert(&mut t, *key, v).unwrap();
            }
            for key in probe_model.keys() {
                if !model.contains_key(key) {
                    tree.delete(&mut t, key).unwrap();
                }
            }
            t.set_root(ObjectId::NONE, tree.root());
            t.patch(data_page, 0, &data).unwrap();
            t.commit().unwrap();
        }
        store.checkpoint().unwrap();
        assert_eq!(read_all(&mut store), model);
        let before_model = model.clone();
        let before_data = data;
        let mut after_model = model.clone();
        let mut after_data = data;
        let mut r = Rng(99);
        let w = d.stats().writes;
        d.tear_at(Some(w + k));
        d.fail_after(Some(w + k));
        let result = run_txn(
            &mut store,
            &mut r,
            &mut after_model,
            data_page,
            &mut after_data,
        );
        drop(store);
        d.drop_unflushed();
        d.tear_at(None);
        d.fail_after(None);
        let (mut store, _) = Store::open(boxed(&d), config(1000))
            .unwrap_or_else(|e| panic!("reopen after torn write {k}: {e}"));
        let got = read_all(&mut store);
        let got_data = data_page_bytes(&mut store, data_page);
        if got == before_model {
            assert_eq!(got_data, before_data, "torn write {k}: rolled back");
        } else {
            assert_eq!(got, after_model, "torn write {k}: neither state");
            assert_eq!(got_data, after_data, "torn write {k}: completed");
            assert!(
                result.is_ok() || k == n,
                "a completed commit reported success unless its last write tore"
            );
        }
        store.close().unwrap();
    }
}

#[test]
fn wal_fill_triggers_checkpoints_and_oversize_is_refused() {
    let d = dev(4096, 2);
    let mut store = Store::format(boxed(&d), &params(8), config(1_000_000)).unwrap();
    let mut rng = Rng(5);
    let data_page = {
        let mut t = store.begin().unwrap();
        let p = t.alloc().unwrap();
        let mut page = store::Page::zeroed();
        t.write(p, &mut page, store::PageType::ObjectData, ObjectId::NONE)
            .unwrap();
        t.commit().unwrap();
        p
    };
    let mut model = Model::new();
    let mut data = [0u8; 64];
    // Each transaction images the data page once per checkpoint window
    // (4 KiB), so an 8-page ring forces checkpoints every few commits.
    for _ in 0..50 {
        run_txn(&mut store, &mut rng, &mut model, data_page, &mut data).unwrap();
    }
    assert!(store.stats().checkpoints >= 5, "{:?}", store.stats());
    assert_eq!(read_all(&mut store), model);
    // A transaction imaging more pages than the ring holds is refused
    // and leaves the store usable.
    let pages: Vec<u64> = {
        let mut t = store.begin().unwrap();
        let run = t.alloc_run(12).unwrap();
        assert_eq!(run.len, 12);
        for p in run.start..run.start + 12 {
            let mut page = store::Page::zeroed();
            t.write(p, &mut page, store::PageType::ObjectData, ObjectId::NONE)
                .unwrap();
        }
        t.commit().unwrap();
        (run.start..run.start + 12).collect()
    };
    let mut t = store.begin().unwrap();
    for p in &pages {
        t.patch(*p, 0, &[1]).unwrap();
    }
    assert!(matches!(t.commit(), Err(StoreError::TxnTooLarge { .. })));
    run_txn(&mut store, &mut rng, &mut model, data_page, &mut data).unwrap();
    assert_eq!(read_all(&mut store), model);
}

#[test]
fn abort_returns_pages_and_cancels_frees() {
    let d = dev(1024, 4);
    let mut store = Store::format(boxed(&d), &params(16), config(100)).unwrap();
    let free0 = store.asp().total_free();
    let run = {
        let mut t = store.begin().unwrap();
        let r = t.alloc_run(4).unwrap();
        t.commit().unwrap();
        r
    };
    assert_eq!(store.asp().total_free(), free0 - 4);
    {
        let mut t = store.begin().unwrap();
        t.alloc_run(8).unwrap();
        t.free_run(run.start, run.len).unwrap();
        assert_eq!(t.store().asp().total_free(), free0 - 12);
        t.abort();
    }
    assert_eq!(store.asp().total_free(), free0 - 4, "allocation undone");
    assert_eq!(store.stats().aborts, 1);
    // The cancelled free can be done again.
    let mut t = store.begin().unwrap();
    t.free_run(run.start, run.len).unwrap();
    t.commit().unwrap();
    store.checkpoint().unwrap();
    assert!(
        store.asp().total_free() >= free0 - 2,
        "freed after the checkpoint"
    );
}

#[test]
fn time_based_checkpoint() {
    let d = dev(1024, 6);
    let mut store = Store::format(boxed(&d), &params(16), config(100)).unwrap();
    assert!(
        !store.tick(1_767_225_600 + 100).unwrap(),
        "nothing to checkpoint"
    );
    let mut t = store.begin().unwrap();
    t.alloc_run(1).unwrap();
    t.commit().unwrap();
    assert!(!store.tick(1_767_225_600 + 110).unwrap());
    assert!(store.tick(1_767_225_600 + 140).unwrap());
    assert_eq!(store.txns_since_checkpoint(), 0);
}
