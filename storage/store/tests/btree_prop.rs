// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The B+tree against `BTreeMap` under random operations, with crashes
//! between operations and torn or corrupted pages.

use std::collections::BTreeMap;

use blockdev::BlockDev;
use store::btree::BTree;
use store::obj::ObjectId;
use store::{MemPager, StoreError};

/// Small deterministic generator for the operation sequence.
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

fn value(rng: &mut Rng) -> Vec<u8> {
    // Mostly small values, sometimes near the limit, so leaves hold
    // anywhere from a few entries to a few hundred.
    let len = match rng.below(10) {
        0 => rng.below(1000) + 1,
        1 => 0,
        _ => rng.below(24) + 1,
    };
    let fill = u8::try_from(rng.below(256)).unwrap();
    vec![fill; usize::try_from(len).unwrap()]
}

/// Check every entry the tree holds against the model.
fn assert_same(tree: &BTree<u64, Vec<u8>>, p: &mut MemPager, model: &BTreeMap<u64, Vec<u8>>) {
    let got: Vec<(u64, Vec<u8>)> = tree.entries(p).unwrap().map(Result::unwrap).collect();
    let want: Vec<(u64, Vec<u8>)> = model.iter().map(|(k, v)| (*k, v.clone())).collect();
    assert_eq!(got.len(), want.len(), "entry count");
    assert_eq!(got, want, "entries in order");
    assert_eq!(tree.len(p).unwrap(), want.len());
}

fn run(seed: u64, ops: usize, key_space: u64) {
    let mut rng = Rng(seed);
    let mut p = MemPager::new(16 * 1024);
    let mut tree: BTree<u64, Vec<u8>> = BTree::new(ObjectId::NONE);
    let mut model: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    let mut committed_root = tree.root();
    let mut committed_model = model.clone();
    for step in 0..ops {
        let k = rng.below(key_space);
        match rng.below(100) {
            0..=54 => {
                let v = value(&mut rng);
                let old = tree.insert(&mut p, k, &v).unwrap();
                assert_eq!(old, model.insert(k, v), "insert {k} at step {step}");
            }
            55..=89 => {
                let old = tree.delete(&mut p, &k).unwrap();
                assert_eq!(old, model.remove(&k), "delete {k} at step {step}");
            }
            90..=94 => {
                let hi = k + rng.below(64);
                let got: Vec<_> = tree
                    .range(&mut p, k..hi)
                    .unwrap()
                    .map(Result::unwrap)
                    .collect();
                let want: Vec<_> = model.range(k..hi).map(|(a, b)| (*a, b.clone())).collect();
                assert_eq!(got, want, "range {k}..{hi} at step {step}");
            }
            95..=97 => {
                p.commit().unwrap();
                committed_root = tree.root();
                committed_model = model.clone();
            }
            _ => {
                // Power pull: everything since the last commit is gone,
                // the committed tree must be intact.
                p.crash();
                tree = BTree::open(committed_root, ObjectId::NONE);
                model = committed_model.clone();
                assert_same(&tree, &mut p, &model);
            }
        }
        if step % 997 == 0 {
            assert_eq!(tree.get(&mut p, &k).unwrap(), model.get(&k).cloned());
        }
    }
    assert_same(&tree, &mut p, &model);
    // Drain and check every page comes back.
    let keys: Vec<u64> = model.keys().copied().collect();
    for k in keys {
        assert!(tree.delete(&mut p, &k).unwrap().is_some());
    }
    assert!(tree.is_empty());
    p.commit().unwrap();
    assert_eq!(p.in_use(), 0, "seed {seed}: pages leaked");
}

#[test]
fn random_ops_match_btreemap() {
    for seed in [1, 2, 3] {
        run(seed, 100_000, 5_000);
    }
}

#[test]
fn random_ops_dense_small_keyspace() {
    run(77, 100_000, 300);
}

#[test]
fn torn_page_write_before_commit_is_harmless() {
    let mut p = MemPager::new(8192);
    let mut tree: BTree<u64, u64> = BTree::new(ObjectId::NONE);
    for i in 0..500 {
        tree.insert(&mut p, i, &i).unwrap();
    }
    p.commit().unwrap();
    let root = tree.root();
    let writes = p.device().stats().writes;
    // The next write is torn, then the power goes.
    p.device().tear_at(Some(writes + 1));
    tree.insert(&mut p, 1000, &1).unwrap();
    p.crash();
    let tree: BTree<u64, u64> = BTree::open(root, ObjectId::NONE);
    let n = tree.entries(&mut p).unwrap().map(Result::unwrap).count();
    assert_eq!(n, 500);
}

#[test]
fn corrupted_committed_page_names_page_and_owner() {
    let owner = ObjectId {
        asp: 1,
        type_code: 0x0001,
        serial: 42,
        generation: 0,
    };
    let mut p = MemPager::new(8192);
    let mut tree: BTree<u64, u64> = BTree::new(owner);
    for i in 0..2000 {
        tree.insert(&mut p, i, &i).unwrap();
    }
    p.commit().unwrap();
    let root = tree.root().unwrap();
    // Flip a byte in the root page behind the pager's back.
    let mut buf = vec![0u8; 4096];
    p.device().read(root, &mut buf).unwrap();
    buf[100] ^= 0xFF;
    p.device().write(root, &buf).unwrap();
    p.device().flush().unwrap();
    let e = tree.get(&mut p, &5).unwrap_err();
    match e {
        StoreError::Checksum { page, owner: o, .. } => {
            assert_eq!(page, root);
            assert_eq!(o, owner);
        }
        other => panic!("expected a checksum error, got {other}"),
    }
    assert!(e.to_string().contains(&format!("page {root}")));
}

#[test]
fn pager_errors_surface_from_the_tree() {
    let mut p = MemPager::new(8);
    let mut tree: BTree<u64, Vec<u8>> = BTree::new(ObjectId::NONE);
    let mut hit_no_space = false;
    for i in 0..100 {
        match tree.insert(&mut p, i, &vec![0; 900]) {
            Ok(_) => {}
            Err(StoreError::NoSpace { .. }) => {
                hit_no_space = true;
                break;
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(hit_no_space);
}
