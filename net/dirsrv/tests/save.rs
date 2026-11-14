// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVLIB QUSRDIRDB` and `RSTLIB` carry a `*DIRDB` whole through the
//! save engine, with no code of its own there.

#![allow(clippy::many_single_char_names, clippy::too_many_lines)]

mod common;

use std::io::Cursor;

use common::{NORM, NOW, dn, name, ou, person, rdn, root, system};
use dirsrv::store::{Attr, ChangeKind, ChangeRec, DirDb};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use store::obj::{JobHandle, ops};

fn header() -> MediaHeader {
    MediaHeader {
        release: (1, 2, 0),
        target: (1, 2, 0),
        compression: Compression::Low,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [2; 16],
        system: "QS400".into(),
        serial: "1000001".into(),
        command: "SAVLIB".into(),
        user: "QSECOFR".into(),
    }
}

#[test]
fn savlib_and_rstlib_carry_the_store() {
    let mut sys = system(32768);
    let db = sys.db;
    let mut t = sys.store.begin().unwrap();
    let s = db
        .add_suffix(&mut t, &NORM, &dn("o=acme"), &root("o=acme"))
        .unwrap();
    let p = db
        .add(&mut t, &NORM, s, &rdn("ou=people"), &ou("people"))
        .unwrap();
    let mut ids = Vec::new();
    for i in 0..300 {
        let mut e = person(&format!("p{i}"));
        e.attrs
            .push(Attr::text("mail", &[&format!("p{i}@acme.example")]));
        if i == 7 {
            e.attrs.push(Attr {
                name: "jpegPhoto".into(),
                values: vec![vec![0xA5; 20_000]],
            });
        }
        ids.push(
            db.add(&mut t, &NORM, p, &rdn(&format!("cn=p{i}")), &e)
                .unwrap(),
        );
    }
    for i in 0..5 {
        db.log_append(
            &mut t,
            &ChangeRec {
                kind: ChangeKind::Add,
                time: i,
                entry: ids[usize::try_from(i).unwrap()],
                target_dn: format!("cn=p{i},ou=people,o=acme"),
                new_rdn: None,
                delete_old_rdn: false,
                new_superior: None,
                modifier: "cn=Administrator".into(),
                changes: vec![b'l'; 3000],
            },
        )
        .unwrap();
    }
    let before = db.attr(&mut t).unwrap();
    let photo = db.get(&mut t, ids[7]).unwrap().unwrap();
    t.commit().unwrap();

    let mut w = Writer::new(Vec::new(), &header()).unwrap();
    w.parameters("SAVLIB LIB(QUSRDIRDB)").unwrap();
    let rep = save_library(
        &mut sys.store,
        JobHandle(1),
        &AllowAll,
        sys.qsys,
        "QUSRDIRDB",
        &SaveOptions {
            now: NOW + 10,
            ..SaveOptions::default()
        },
        &mut w,
    )
    .unwrap();
    assert_eq!(rep.not_saved(), 0, "{:?}", rep.objects);
    assert_eq!(rep.saved(), 1);
    let (_, bytes) = w.finish(0).unwrap();

    let mut r = Reader::new(Cursor::new(&bytes)).unwrap();
    let rr = restore(
        &mut sys.store,
        JobHandle(1),
        &AllowAll,
        sys.qsys,
        &mut r,
        &RestoreOptions {
            target_library: Some("QUSRDIRDB2".into()),
            now: NOW + 20,
            system: "QS400".into(),
            ..RestoreOptions::default()
        },
    )
    .unwrap();
    assert_eq!(rr.not_restored(), 0, "{:?}", rr.objects);

    let mut t = sys.store.begin().unwrap();
    let lib2 = ops::find_library(&mut t, sys.qsys, name("QUSRDIRDB2"))
        .unwrap()
        .id;
    let db2 = DirDb::open(&mut t, lib2, name("QUSRDIR")).unwrap().unwrap();
    assert_ne!(db2.id(), db.id());
    assert_eq!(db2.attr(&mut t).unwrap(), before);
    let got = db2
        .resolve(&mut t, &NORM, &dn("cn=P150,ou=people,o=ACME"))
        .unwrap();
    assert_eq!(got.entry, Some(ids[150]));
    assert_eq!(db2.get(&mut t, ids[7]).unwrap().unwrap(), photo);
    let eq = db2
        .equal(&mut t, &NORM, "mail", b"P42@acme.example", None, 9)
        .unwrap()
        .unwrap();
    assert_eq!(eq, [ids[42]]);
    assert_eq!(db2.log_read(&mut t, 0, 10).unwrap().len(), 5);
    // The copy is a store of its own: a change to it leaves the first.
    db2.delete(&mut t, &NORM, ids[150]).unwrap();
    let first = db
        .resolve(&mut t, &NORM, &dn("cn=p150,ou=people,o=acme"))
        .unwrap();
    assert_eq!(first.entry, Some(ids[150]));
    t.commit().unwrap();
    sys.store.checkpoint().unwrap();
    let c = sys.store.check_pages().unwrap();
    assert_eq!(c.problems(), 0, "{c}");
}
