// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Trees whose key length is chosen at run time, up to the limit, as
//! file access paths use them; and the type-erased page walk.

use std::collections::BTreeMap;

use store::MemPager;
use store::btree::{self, BTree, DynKey};
use store::obj::ObjectId;

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
}

fn key(rng: &mut Rng, len: usize) -> DynKey {
    let mut k = vec![0u8; len];
    // A few leading bytes vary; the rest is filler, so keys are long but
    // ordering is decided early.
    for b in k.iter_mut().take(6) {
        *b = u8::try_from(rng.next() % 256).unwrap();
    }
    k[len - 1] = u8::try_from(rng.next() % 256).unwrap();
    DynKey(k)
}

fn exercise(key_len: usize, count: usize, value_len: usize) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ key_len as u64);
    let mut p = MemPager::new(64 * 1024);
    let mut tree: BTree<DynKey, Vec<u8>> = BTree::with_key_len(None, ObjectId::NONE, key_len);
    assert_eq!(tree.key_len(), key_len);
    let mut model: BTreeMap<DynKey, Vec<u8>> = BTreeMap::new();
    for i in 0..count {
        let k = key(&mut rng, key_len);
        let v = vec![u8::try_from(i % 251).unwrap(); value_len];
        tree.insert(&mut p, k.clone(), &v).unwrap();
        model.insert(k, v);
    }
    let got: Vec<(DynKey, Vec<u8>)> = tree.entries(&mut p).unwrap().map(Result::unwrap).collect();
    let want: Vec<(DynKey, Vec<u8>)> = model.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert_eq!(got, want);
    for (k, v) in model.iter().take(50) {
        assert_eq!(tree.get(&mut p, k).unwrap().as_ref(), Some(v));
        assert_eq!(tree.floor(&mut p, k).unwrap().unwrap().0, *k);
    }
    // Reopen by root and key length, as a file does.
    let reopened: BTree<DynKey, Vec<u8>> =
        BTree::with_key_len(tree.root(), ObjectId::NONE, key_len);
    assert_eq!(reopened.len(&mut p).unwrap(), model.len());
    // The type-erased walk sees every page the typed walk sees.
    let typed = tree.pages(&mut p).unwrap();
    let mut erased = btree::walk_pages(&mut p, tree.root().unwrap()).unwrap();
    let mut typed_sorted = typed.clone();
    typed_sorted.sort_unstable();
    erased.sort_unstable();
    assert_eq!(erased, typed_sorted);
    // Delete half, in model order.
    let keys: Vec<DynKey> = model.keys().cloned().collect();
    for k in keys.iter().step_by(2) {
        assert!(tree.delete(&mut p, k).unwrap().is_some());
        model.remove(k);
    }
    let got: Vec<DynKey> = tree
        .entries(&mut p)
        .unwrap()
        .map(|r| r.unwrap().0)
        .collect();
    let want: Vec<DynKey> = model.keys().cloned().collect();
    assert_eq!(got, want);
    // Bulk load the rest into a fresh tree and compare.
    let loaded = BTree::<DynKey, Vec<u8>>::bulk_load_with_key_len(
        &mut p,
        ObjectId::NONE,
        key_len,
        model.iter().map(|(k, v)| (k.clone(), v.clone())),
    )
    .unwrap();
    assert_eq!(loaded.len(&mut p).unwrap(), model.len());
    assert_eq!(
        loaded.pages(&mut p).unwrap().len() as u64,
        BTree::<DynKey, Vec<u8>>::bulk_load_pages_with(key_len, model.len(), value_len)
    );
    tree.free_all(&mut p).unwrap();
}

#[test]
fn short_keys() {
    exercise(12, 3000, 8);
}

#[test]
fn medium_keys_with_rrn_tail() {
    exercise(248, 1500, 0);
}

#[test]
fn keys_at_the_limit() {
    exercise(1024, 400, 0);
    exercise(1000, 300, 1024);
}

#[test]
fn wrong_key_length_is_refused() {
    let mut p = MemPager::new(1024);
    let mut tree: BTree<DynKey, Vec<u8>> = BTree::with_key_len(None, ObjectId::NONE, 4);
    tree.insert(&mut p, DynKey(vec![1, 2, 3, 4]), &vec![])
        .unwrap();
    let other: BTree<DynKey, Vec<u8>> = BTree::with_key_len(tree.root(), ObjectId::NONE, 5);
    assert!(other.get(&mut p, &DynKey(vec![1, 2, 3, 4, 5])).is_err());
}
