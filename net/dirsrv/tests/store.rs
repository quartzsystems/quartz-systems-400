// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The entry store over a store in memory: names, entries, indexes, the
//! change log, transactions, RCLSTG, delete and listings.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::needless_range_loop
)]

mod common;

use std::collections::BTreeSet;

use common::{NORM, Sys, dn, name, ou, person, rdn, root, system};
use dirsrv::store::{
    Attr, ChangeKind, ChangeRec, DirDb, DirError, Entry, IndexKind, Normalizer, Resolved,
};
use store::obj::library::Library;
use store::obj::{JobHandle, ops, types};
use store::{ReclaimMode, Store, StoreConfig};

/// `dc=example,dc=com` with `ou=people` under it; their IDs.
fn base(sys: &mut Sys) -> (u64, u64) {
    let mut t = sys.store.begin().unwrap();
    let s = sys
        .db
        .add_suffix(
            &mut t,
            &NORM,
            &dn("dc=example,dc=com"),
            &root("dc=example,dc=com"),
        )
        .unwrap();
    let p = sys
        .db
        .add(&mut t, &NORM, s, &rdn("ou=people"), &ou("people"))
        .unwrap();
    t.commit().unwrap();
    (s, p)
}

fn found(r: Resolved) -> u64 {
    r.entry.expect("resolved")
}

#[test]
fn names_resolve_and_errors() {
    let mut sys = system(16384);
    let (s, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let bob = db
        .add(&mut t, &NORM, p, &rdn("cn=Bob"), &person("Bob"))
        .unwrap();
    assert!(bob > p && p > s);
    // Case and spacing as the normalizer folds them.
    let r = db
        .resolve(&mut t, &NORM, &dn("CN=bob, OU=People, DC=Example, dc=COM"))
        .unwrap();
    assert_eq!(found(r), bob);
    assert_eq!(
        db.dn_of(&mut t, bob).unwrap(),
        "cn=Bob,ou=people,dc=example,dc=com"
    );
    // A missing entry gives the deepest match.
    let r = db
        .resolve(
            &mut t,
            &NORM,
            &dn("cn=x,cn=bob,ou=people,dc=example,dc=com"),
        )
        .unwrap();
    assert_eq!(r.entry, None);
    assert_eq!(r.matched, Some((bob, 4)));
    let r = db.resolve(&mut t, &NORM, &dn("cn=bob,o=else")).unwrap();
    assert_eq!(r.matched, None);
    // Duplicates, non-leaves, moves under oneself.
    assert!(matches!(
        db.add(&mut t, &NORM, p, &rdn("cn=BOB"), &person("BOB")),
        Err(DirError::AlreadyExists)
    ));
    assert!(matches!(
        db.add_suffix(&mut t, &NORM, &dn("DC=example,DC=com"), &root("x")),
        Err(DirError::AlreadyExists)
    ));
    assert!(matches!(
        db.delete(&mut t, &NORM, p),
        Err(DirError::NotLeaf(_))
    ));
    assert!(matches!(
        db.rename(&mut t, &NORM, p, &rdn("ou=people"), "ou=people", Some(bob)),
        Err(DirError::Unwilling(_))
    ));
    assert!(matches!(
        db.rename(&mut t, &NORM, s, &rdn("dc=x"), "dc=x", None),
        Err(DirError::Unwilling(_))
    ));
    assert!(matches!(
        db.add(&mut t, &NORM, 9999, &rdn("cn=a"), &person("a")),
        Err(DirError::NoSuchEntry(9999))
    ));
    assert_eq!(
        DirError::NotLeaf(1).code(),
        ldap::ResultCode::NotAllowedOnNonLeaf
    );
    // Sixteen suffixes, not seventeen.
    for i in 1..16 {
        let d = format!("o=s{i}");
        db.add_suffix(&mut t, &NORM, &dn(&d), &root(&d)).unwrap();
    }
    assert!(matches!(
        db.add_suffix(&mut t, &NORM, &dn("o=s16"), &root("o=s16")),
        Err(DirError::Limit(_))
    ));
    assert_eq!(db.suffixes(&mut t).unwrap().len(), 16);
    // Nested suffixes: the longer one wins.
    let a = db.attr(&mut t).unwrap();
    assert_eq!(a.suffixes[0], s);
    t.commit().unwrap();
}

#[test]
fn multi_valued_and_long_rdns() {
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let m = db
        .add(&mut t, &NORM, p, &rdn("cn=A+uid=a1"), &person("A"))
        .unwrap();
    let r = db
        .resolve(
            &mut t,
            &NORM,
            &dn("uid=A1+cn=a,ou=people,dc=example,dc=com"),
        )
        .unwrap();
    assert_eq!(found(r), m);
    // Two RDNs past the key's room that differ only at the end.
    let long1 = format!("cn={}1", "x".repeat(300));
    let long2 = format!("cn={}2", "x".repeat(300));
    let a = db
        .add(&mut t, &NORM, p, &rdn(&long1), &person("l1"))
        .unwrap();
    let b = db
        .add(&mut t, &NORM, p, &rdn(&long2), &person("l2"))
        .unwrap();
    let d1 = format!("{long1},ou=people,dc=example,dc=com");
    let d2 = format!("{long2},ou=people,dc=example,dc=com");
    assert_eq!(found(db.resolve(&mut t, &NORM, &dn(&d1)).unwrap()), a);
    assert_eq!(found(db.resolve(&mut t, &NORM, &dn(&d2)).unwrap()), b);
    t.commit().unwrap();
}

#[test]
fn large_entries_span_chunks() {
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let mut e = person("Pic");
    e.attrs.push(Attr {
        name: "jpegPhoto".into(),
        values: vec![(0..100_000u32).map(|i| (i % 251) as u8).collect()],
    });
    let id = db.add(&mut t, &NORM, p, &rdn("cn=Pic"), &e).unwrap();
    let got = db.get(&mut t, id).unwrap().unwrap();
    assert_eq!(got.attrs, e.attrs);
    assert_eq!(got.parent, p);
    // Rewritten smaller: the extra chunks go.
    db.put(&mut t, &NORM, id, &person("Pic")).unwrap();
    assert_eq!(
        db.get(&mut t, id).unwrap().unwrap().attrs,
        person("Pic").attrs
    );
    t.commit().unwrap();
    let c = sys.store.check_pages().unwrap();
    assert_eq!(c.problems(), 0, "{c}");
}

#[test]
fn ten_thousand_entries() {
    let mut sys = system(65536);
    let (s, p) = base(&mut sys);
    let db = sys.db;
    let groups = 10;
    let per = 1000;
    let mut ous = Vec::new();
    let mut t = sys.store.begin().unwrap();
    for g in 0..groups {
        let n = format!("g{g}");
        ous.push(
            db.add(&mut t, &NORM, p, &rdn(&format!("ou={n}")), &ou(&n))
                .unwrap(),
        );
    }
    t.commit().unwrap();
    let mut ids = Vec::new();
    for g in 0..groups {
        let mut t = sys.store.begin().unwrap();
        for i in 0..per {
            let cn = format!("u{g}x{i}");
            ids.push(
                db.add(
                    &mut t,
                    &NORM,
                    ous[g],
                    &rdn(&format!("cn={cn}")),
                    &person(&cn),
                )
                .unwrap(),
            );
        }
        t.commit().unwrap();
    }
    let mut t = sys.store.begin().unwrap();
    let a = db.attr(&mut t).unwrap();
    assert_eq!(a.entry_count, 2 + 10 + 10_000);
    // Every entry resolves.
    for g in 0..groups {
        for i in (0..per).step_by(7) {
            let d = format!("cn=u{g}x{i},ou=g{g},ou=people,dc=example,dc=com");
            assert_eq!(
                found(db.resolve(&mut t, &NORM, &dn(&d)).unwrap()),
                ids[g * per + i]
            );
        }
    }
    // Children in pages, resumable.
    let mut seen = Vec::new();
    let mut from = None;
    loop {
        let page = db.children(&mut t, ous[3], from.as_ref(), 128).unwrap();
        seen.extend(page.ids);
        match page.next {
            Some(c) => from = Some(c),
            None => break,
        }
    }
    let want: BTreeSet<u64> = ids[3 * per..4 * per].iter().copied().collect();
    assert_eq!(seen.len(), per);
    assert_eq!(seen.iter().copied().collect::<BTreeSet<_>>(), want);
    // A subtree walk in pages: every entry once, parents first.
    let mut walked = Vec::new();
    let mut from = None;
    loop {
        let page = db.subtree(&mut t, s, from.as_ref(), 333).unwrap();
        assert!(page.ids.len() <= 333);
        walked.extend(page.ids);
        match page.next {
            Some(c) => from = Some(c),
            None => break,
        }
    }
    assert_eq!(walked.len(), 2 + 10 + 10_000);
    assert_eq!(walked[0], s);
    assert_eq!(walked[1], p);
    assert_eq!(
        walked.iter().copied().collect::<BTreeSet<_>>().len(),
        walked.len()
    );
    let pos = |id: u64| walked.iter().position(|x| *x == id).unwrap();
    assert!(pos(ous[5]) < pos(ids[5 * per + 999]));
    t.commit().unwrap();

    // Move a thousand-entry subtree with one re-key, and rename it.
    let mut t = sys.store.begin().unwrap();
    db.rename(
        &mut t,
        &NORM,
        ous[0],
        &rdn("ou=moved"),
        "ou=moved",
        Some(ous[1]),
    )
    .unwrap();
    let d = "cn=u0x500,ou=moved,ou=g1,ou=people,dc=example,dc=com";
    assert_eq!(found(db.resolve(&mut t, &NORM, &dn(d)).unwrap()), ids[500]);
    let old = "cn=u0x500,ou=g0,ou=people,dc=example,dc=com";
    assert_eq!(db.resolve(&mut t, &NORM, &dn(old)).unwrap().entry, None);
    assert_eq!(db.dn_of(&mut t, ids[500]).unwrap(), d);
    t.commit().unwrap();

    // Delete every entry, leaves first.
    for chunk in ids.chunks(1000) {
        let mut t = sys.store.begin().unwrap();
        for id in chunk {
            db.delete(&mut t, &NORM, *id).unwrap();
        }
        t.commit().unwrap();
    }
    let mut t = sys.store.begin().unwrap();
    for o in &ous {
        db.delete(&mut t, &NORM, *o).unwrap();
    }
    db.delete(&mut t, &NORM, p).unwrap();
    db.delete(&mut t, &NORM, s).unwrap();
    let a = db.attr(&mut t).unwrap();
    assert_eq!(a.entry_count, 0);
    assert_eq!(a.index_keys, 0);
    assert!(a.suffixes.is_empty());
    assert!(db.ids(&mut t, 1, 10).unwrap().is_empty());
    // IDs are never reused.
    let again = db
        .add_suffix(&mut t, &NORM, &dn("o=x"), &root("o=x"))
        .unwrap();
    assert!(again > *ids.last().unwrap());
    t.commit().unwrap();
    let c = sys.store.check_pages().unwrap();
    assert_eq!(c.problems(), 0, "{c}");
}

#[test]
fn index_lookups() {
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let mut ids = Vec::new();
    for (cn, mail) in [
        ("John Smith", "jsmith@example.com"),
        ("Jane Smith", "jane@example.com"),
        ("Bob Jones", "bob@other.org"),
    ] {
        let mut e = person(cn);
        e.attrs.push(Attr::text("mail", &[mail]));
        ids.push(
            db.add(&mut t, &NORM, p, &rdn(&format!("cn={cn}")), &e)
                .unwrap(),
        );
    }
    let defs = db.indexes(&mut t).unwrap();
    assert_eq!(defs.len(), 6);
    assert!(defs.iter().all(|d| d.always));
    // Equality, normalized.
    let eq = db
        .equal(&mut t, &NORM, "cn", b"JOHN  smith", None, 100)
        .unwrap()
        .unwrap();
    assert_eq!(eq, [ids[0]]);
    // Presence: every entry with mail.
    let pr = db.present(&mut t, "mail", None, 100).unwrap().unwrap();
    assert_eq!(pr, ids);
    let pr = db
        .present(&mut t, "mail", Some(ids[0]), 1)
        .unwrap()
        .unwrap();
    assert_eq!(pr, [ids[1]]);
    // Not indexed.
    assert!(
        db.present(&mut t, "telephonenumber", None, 9)
            .unwrap()
            .is_none()
    );
    // Substrings: *smith, ja*, bo*jon*, *example*; pieces too short.
    let c = |t: &mut store::Txn<'_>, i: Option<&[u8]>, a: &[&[u8]], f: Option<&[u8]>| {
        db.substring_candidates(t, &NORM, "cn", i, a, f).unwrap()
    };
    assert_eq!(
        c(&mut t, None, &[], Some(b"SMITH")).unwrap(),
        [ids[0], ids[1]]
    );
    assert!(c(&mut t, Some(b"j"), &[], None).is_none());
    assert_eq!(c(&mut t, Some(b"JA"), &[], None).unwrap(), [ids[1]]);
    assert_eq!(c(&mut t, Some(b"bo"), &[b"jon"], None).unwrap(), [ids[2]]);
    assert!(c(&mut t, None, &[b"o"], None).is_none());
    let m = db
        .substring_candidates(&mut t, &NORM, "mail", None, &[b"example"], None)
        .unwrap()
        .unwrap();
    assert_eq!(m, [ids[0], ids[1]]);
    // A modify moves the keys.
    let mut e = db.get(&mut t, ids[2]).unwrap().unwrap();
    e.attrs.retain(|a| a.name != "mail");
    e.attrs.push(Attr::text("cn", &["Robert Jones"]));
    db.put(&mut t, &NORM, ids[2], &e).unwrap();
    let pr = db.present(&mut t, "mail", None, 100).unwrap().unwrap();
    assert_eq!(pr, [ids[0], ids[1]]);
    let eq = db
        .equal(&mut t, &NORM, "cn", b"robert jones", None, 9)
        .unwrap()
        .unwrap();
    assert_eq!(eq, [ids[2]]);
    // A configured index over existing entries, then removed.
    let n = db
        .add_index(&mut t, &NORM, "sn", IndexKind::Equality.bit())
        .map(|_| ())
        .unwrap_err();
    assert!(matches!(n, DirError::AlreadyExists));
    let before = db.attr(&mut t).unwrap().index_keys;
    let num = db
        .add_index(&mut t, &NORM, "objectclass2", IndexKind::Presence.bit())
        .unwrap();
    assert_eq!(num, 7);
    assert_eq!(db.attr(&mut t).unwrap().index_keys, before);
    let tel = db
        .add_index(
            &mut t,
            &NORM,
            "telephonenumber",
            IndexKind::Equality.bit() | IndexKind::Presence.bit(),
        )
        .unwrap();
    let mut e = db.get(&mut t, ids[1]).unwrap().unwrap();
    e.attrs.push(Attr::text("telephoneNumber", &["555 1234"]));
    db.put(&mut t, &NORM, ids[1], &e).unwrap();
    assert_eq!(
        db.equal(&mut t, &NORM, "telephonenumber", b"555  1234", None, 9)
            .unwrap()
            .unwrap(),
        [ids[1]]
    );
    db.remove_index(&mut t, tel).unwrap();
    assert!(
        db.present(&mut t, "telephonenumber", None, 9)
            .unwrap()
            .is_none()
    );
    assert_eq!(db.attr(&mut t).unwrap().index_keys, before);
    assert!(matches!(
        db.remove_index(&mut t, 1),
        Err(DirError::Unwilling(_))
    ));
    t.commit().unwrap();
    // An index added later covers entries added before it.
    let mut t = sys.store.begin().unwrap();
    let late = db
        .add_index(&mut t, &NORM, "ou", IndexKind::Equality.bit())
        .unwrap();
    assert!(late > tel);
    let o = db
        .equal(&mut t, &NORM, "ou", b"PEOPLE", None, 9)
        .unwrap()
        .unwrap();
    assert_eq!(o, [p]);
    t.commit().unwrap();
}

#[test]
fn change_log_appends_reads_trims() {
    let mut sys = system(16384);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let rec = |i: u64| ChangeRec {
        kind: ChangeKind::Add,
        time: 1_000 * i,
        entry: i,
        target_dn: format!("cn=e{i},o=x"),
        new_rdn: None,
        delete_old_rdn: false,
        new_superior: None,
        modifier: "cn=Administrator".into(),
        changes: vec![b'x'; usize::try_from(i * 300).unwrap()],
    };
    for i in 1..=20 {
        assert_eq!(db.log_append(&mut t, &rec(i)).unwrap(), i);
    }
    let got = db.log_read(&mut t, 5, 3).unwrap();
    assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), [5, 6, 7]);
    assert_eq!(got[2].1, rec(7));
    db.log_trim(&mut t, 8).unwrap();
    let a = db.attr(&mut t).unwrap();
    assert_eq!((a.log_first, a.log_count), (13, 8));
    assert_eq!(db.log_read(&mut t, 1, 100).unwrap()[0].0, 13);
    db.set_log_max(&mut t, 5).unwrap();
    db.log_append(&mut t, &rec(21)).unwrap();
    let a = db.attr(&mut t).unwrap();
    assert_eq!((a.log_first, a.log_count, a.next_change), (17, 5, 22));
    db.log_trim(&mut t, 0).unwrap();
    let a = db.attr(&mut t).unwrap();
    assert_eq!((a.log_first, a.log_count), (0, 0));
    assert!(db.log_read(&mut t, 0, 10).unwrap().is_empty());
    t.commit().unwrap();
}

#[test]
fn an_aborted_transaction_leaves_nothing() {
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let before = {
        let mut t = sys.store.begin().unwrap();
        db.attr(&mut t).unwrap()
    };
    {
        let mut t = sys.store.begin().unwrap();
        for i in 0..200 {
            db.add(
                &mut t,
                &NORM,
                p,
                &rdn(&format!("cn=t{i}")),
                &person(&format!("t{i}")),
            )
            .unwrap();
        }
        db.log_append(
            &mut t,
            &ChangeRec {
                kind: ChangeKind::Delete,
                time: 1,
                entry: 1,
                target_dn: "o=x".into(),
                new_rdn: None,
                delete_old_rdn: false,
                new_superior: None,
                modifier: String::new(),
                changes: Vec::new(),
            },
        )
        .unwrap();
        t.abort();
    }
    let mut t = sys.store.begin().unwrap();
    assert_eq!(db.attr(&mut t).unwrap(), before);
    let r = db
        .resolve(&mut t, &NORM, &dn("cn=t5,ou=people,dc=example,dc=com"))
        .unwrap();
    assert_eq!(r.entry, None);
    assert!(db.children(&mut t, p, None, 10).unwrap().ids.is_empty());
    assert!(db.log_read(&mut t, 0, 10).unwrap().is_empty());
    t.commit().unwrap();
    // And a committed one survives a reopen of the device.
    let mut t = sys.store.begin().unwrap();
    let id = db
        .add(&mut t, &NORM, p, &rdn("cn=kept"), &person("kept"))
        .unwrap();
    t.commit().unwrap();
    sys.store.checkpoint().unwrap();
    drop(sys.store);
    let (mut store, _) = Store::open(
        vec![Box::new(sys.dev.clone()) as Box<dyn blockdev::BlockDev>],
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let db = DirDb::open(&mut t, sys.lib, name("QUSRDIR"))
        .unwrap()
        .unwrap();
    let r = db
        .resolve(&mut t, &NORM, &dn("cn=KEPT,ou=people,dc=example,dc=com"))
        .unwrap();
    assert_eq!(r.entry, Some(id));
}

#[test]
fn reclaim_walks_delete_frees_listings_hide() {
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    for i in 0..500 {
        let mut e = person(&format!("r{i}"));
        e.attrs.push(Attr {
            name: "description".into(),
            values: vec![vec![b'd'; 1500]],
        });
        db.add(&mut t, &NORM, p, &rdn(&format!("cn=r{i}")), &e)
            .unwrap();
    }
    for i in 0..30 {
        db.log_append(
            &mut t,
            &ChangeRec {
                kind: ChangeKind::Modify,
                time: i,
                entry: p,
                target_dn: "ou=people,dc=example,dc=com".into(),
                new_rdn: None,
                delete_old_rdn: false,
                new_superior: None,
                modifier: String::new(),
                changes: vec![b'c'; 2500],
            },
        )
        .unwrap();
    }
    // Listings leave it out unless asked for by type; the library's
    // own directory (save, RCLSTG) has it.
    assert!(
        ops::list(&mut t, sys.lib, None)
            .unwrap()
            .iter()
            .all(|(k, _)| k.type_code != types::DIRDB)
    );
    assert_eq!(
        ops::list(&mut t, sys.lib, Some(types::DIRDB))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        Library::load(&mut t, sys.lib)
            .unwrap()
            .list(&mut t, None)
            .unwrap()
            .len(),
        1
    );
    t.commit().unwrap();
    sys.store.checkpoint().unwrap();
    // RCLSTG finds every page of the four trees.
    let r = sys.store.reclaim(ReclaimMode::Check, NOW_RCL).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
    let r = sys.store.reclaim(ReclaimMode::All, NOW_RCL + 1).unwrap();
    assert_eq!(r.problems(), 0, "{r}");
    let mut t = sys.store.begin().unwrap();
    let db = DirDb::open(&mut t, sys.lib, name("QUSRDIR"))
        .unwrap()
        .unwrap();
    let r = db
        .resolve(&mut t, &NORM, &dn("cn=r250,ou=people,dc=example,dc=com"))
        .unwrap();
    assert!(r.entry.is_some());
    assert_eq!(db.log_read(&mut t, 0, 100).unwrap().len(), 30);
    // Delete frees the trees.
    ops::delete(&mut t, JobHandle(1), db.id()).unwrap();
    assert!(
        DirDb::open(&mut t, sys.lib, name("QUSRDIR"))
            .unwrap()
            .is_none()
    );
    t.commit().unwrap();
    sys.store.checkpoint().unwrap();
    let c = sys.store.check_pages().unwrap();
    assert_eq!(c.problems(), 0, "{c}");
}

const NOW_RCL: u64 = common::NOW + 1000;

#[test]
fn normalizer_errors_refuse_cleanly() {
    struct Picky;
    impl Normalizer for Picky {
        fn attr_id(&self, d: &str) -> Option<String> {
            NORM.attr_id(d)
        }
        fn rdn_key(&self, r: &ldap::Rdn) -> Result<Vec<u8>, dirsrv::store::NormError> {
            NORM.rdn_key(r)
        }
        fn value_key(
            &self,
            a: &str,
            v: &[u8],
            rule: dirsrv::store::Rule,
        ) -> Result<Option<Vec<u8>>, dirsrv::store::NormError> {
            if v == b"bad" {
                return Err(dirsrv::store::NormError::new(
                    ldap::ResultCode::InvalidAttributeSyntax,
                    "bad value",
                ));
            }
            NORM.value_key(a, v, rule)
        }
    }
    let mut sys = system(16384);
    let (_, p) = base(&mut sys);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let e = Entry::new("cn=bad", vec![Attr::text("cn", &["bad"])]);
    let err = db.add(&mut t, &Picky, p, &rdn("cn=x"), &e).unwrap_err();
    assert_eq!(err.code(), ldap::ResultCode::InvalidAttributeSyntax);
    t.abort();
}
