// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QSYS.LIB` changed outside the IFS (`ifs::lease::qsys`): a member's
//! key is the one the IFS gives it, and each change breaks and is heard
//! as the IFS operation with the same effect.

mod common;

use std::time::Duration;

use common::{sys, user};
use ifs::Errno;
use ifs::lease::defer::deferring;
use ifs::lease::notify::{ChangeKind, DirEvent};
use ifs::lease::qsys::{QsysChange, changed};
use ifs::lease::state::{H, R, RWH};
use ifs::ops::{Attr, AttrId, Owner};
use ifs::resolve::qsys_key;
use ifs::vfs::qsys::QsysNode;

const HELLO: &str = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";
const DB: Owner = Owner(2);

fn hello() -> QsysNode {
    QsysNode::Mbr("QGPL".into(), "QCLSRC".into(), "HELLO".into())
}

fn ev(name: &str, kind: ChangeKind) -> DirEvent {
    DirEvent {
        name: name.into(),
        kind,
        dir: false,
    }
}

#[test]
fn a_member_changed_outside_the_ifs_breaks_its_lease() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    let id = s
        .ifs
        .get_attrs(&a, HELLO, &[AttrId::FileId], true)
        .unwrap()
        .into_iter()
        .find_map(|x| match x {
            Attr::FileId(id) => Some(id),
            _ => None,
        })
        .unwrap();
    // One identity: the IFS's file number is the key's.
    assert_eq!(id.ino, qsys_key(&hello()).ino);
    assert_eq!(id.fs, qsys_key(&hello()).fs);
    assert_eq!(
        s.ifs.lease_grant(&a, HELLO, id, 5, [1; 32], RWH).unwrap(),
        RWH
    );
    let l = s.ifs.leases();
    // An open for change waits for write caching to be given back.
    let (r, d) = deferring(true, || changed(l, DB, &hello(), &QsysChange::Opening));
    assert_eq!(r, Err(Errno::EAGAIN));
    let d = d.unwrap();
    l.report(Owner(1), 5, &[([1; 32], R | H)]);
    assert!(l.ready(&d));
    // Records never wait, even where a deferral is allowed.
    let (r, d) = deferring(true, || changed(l, DB, &hello(), &QsysChange::Data));
    assert_eq!((r, d), (Ok(()), None));
    let news = s.ifs.lease_orders(&a, Duration::ZERO).unwrap();
    // The order queued first is lowered, not repeated.
    assert_eq!(news.breaks, vec![(5, 0)]);
}

#[test]
fn a_watch_hears_one_modified_for_many_records() {
    let s = sys();
    let p = user("QSECOFR");
    let a = s.cx(&p, 1);
    s.ifs
        .watch_add(&a, "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE", false)
        .unwrap();
    let l = s.ifs.leases();
    for _ in 0..1000 {
        changed(l, DB, &hello(), &QsysChange::Data).unwrap();
    }
    let gone = QsysNode::Mbr("QGPL".into(), "QCLSRC".into(), "OLD".into());
    changed(l, DB, &hello(), &QsysChange::Renamed { from: gone }).unwrap();
    changed(
        l,
        DB,
        &hello(),
        &QsysChange::Removing {
            members: Vec::new(),
        },
    )
    .unwrap();
    let news = s.ifs.lease_orders(&a, Duration::ZERO).unwrap();
    let heard: Vec<DirEvent> = news.changes.into_iter().flat_map(|(_, v)| v).collect();
    assert_eq!(
        heard,
        vec![
            ev("HELLO.MBR", ChangeKind::Modified),
            ev(
                "HELLO.MBR",
                ChangeKind::Renamed {
                    from: "OLD.MBR".into()
                }
            ),
            ev("HELLO.MBR", ChangeKind::Removed),
        ]
    );
}
