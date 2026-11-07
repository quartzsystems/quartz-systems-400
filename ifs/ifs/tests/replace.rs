// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `client::replace`: a file another profile rewrites keeps its owner
//! and authorities, so its owner can still replace it. The case is the
//! HTTP instance registry, `/www/instances.json`, owned by `QTMHHTTP`
//! (here `BOB`) and rewritten by `ENDTCPSVR` as `QSECOFR`.

mod common;

use common::{sys, user};
use ifs::Errno;
use ifs::auth::data::{DataAuth, RW};
use ifs::client::{Direct, IfsClient, replace};
use ifs::ops::{ChgAut, RenameMode, Role, Who};

const REG: &str = "/www/instances.json";

/// The registry as the owner's rename leaves it.
fn setup() -> common::Sys {
    let s = sys();
    s.ifs.ensure_dir(&s.env, "/www", "BOB", "*RX").unwrap();
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    replace(
        &Direct {
            ifs: &s.ifs,
            cx: &bx,
        },
        REG,
        b"1",
        0o644,
    )
    .unwrap();
    s
}

#[test]
fn a_rename_by_another_profile_takes_the_file() {
    // What the plain temporary file and rename did.
    let s = setup();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    s.put(&cx, "/www/x.tmp", b"2");
    s.ifs
        .rename(&cx, "/www/x.tmp", REG, RenameMode::Unlink)
        .unwrap();
    assert_eq!(s.ifs.dspaut(&cx, REG).unwrap().owner, "QSECOFR");
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    s.put(&bx, "/www/y.tmp", b"3");
    assert_eq!(
        s.ifs.rename(&bx, "/www/y.tmp", REG, RenameMode::Unlink),
        Err(Errno::EACCES)
    );
}

#[test]
fn another_profile_rewrites_in_place() {
    let s = setup();
    let sec = user("QSECOFR");
    let cx = s.cx(&sec, 1);
    replace(
        &Direct {
            ifs: &s.ifs,
            cx: &cx,
        },
        REG,
        b"two",
        0o644,
    )
    .unwrap();
    let a = s.ifs.dspaut(&cx, REG).unwrap();
    assert_eq!(a.owner, "BOB");
    assert!(a.entries.iter().all(|e| e.role != Role::Private), "{a:?}");
    assert_eq!(s.get(&cx, REG), b"two");
    assert!(s.ifs.stat(&cx, "/www/instances.json.tmp").is_err());
    // The owner still replaces it, through a rename.
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    replace(
        &Direct {
            ifs: &s.ifs,
            cx: &bx,
        },
        REG,
        b"three",
        0o644,
    )
    .unwrap();
    assert_eq!(s.get(&bx, REG), b"three");
    assert_eq!(s.ifs.dspaut(&cx, REG).unwrap().owner, "BOB");
}

#[test]
fn private_authorities_survive_the_owner_too() {
    let s = setup();
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    let grant = ChgAut {
        who: vec![Who::User("ZED".into())],
        data: Some(DataAuth::of(RW)),
        ..ChgAut::default()
    };
    s.ifs.chgaut(&bx, REG, &grant).unwrap();
    let d = Direct {
        ifs: &s.ifs,
        cx: &bx,
    };
    replace(&d, REG, b"four", 0o644).unwrap();
    let a = d.dspaut(REG).unwrap();
    assert!(
        a.entries.iter().any(|e| e.who == "ZED" && e.data == "*RW"),
        "{a:?}"
    );
    // ZED, with *RW only, rewrites it in place.
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    s.ifs
        .chgaut(
            &bx,
            "/www",
            &ChgAut {
                who: vec![Who::User("ZED".into())],
                data: Some(DataAuth::of(ifs::auth::data::RWX)),
                ..ChgAut::default()
            },
        )
        .unwrap();
    replace(
        &Direct {
            ifs: &s.ifs,
            cx: &zx,
        },
        REG,
        b"five",
        0o644,
    )
    .unwrap();
    assert_eq!(s.get(&bx, REG), b"five");
    assert_eq!(d.dspaut(REG).unwrap().owner, "BOB");
}

#[test]
fn public_authority_survives_the_owner() {
    // Step 86: CHGAUT gave *PUBLIC *RWX; the owner's rewrite keeps it, so
    // another profile still writes the file.
    let s = setup();
    let bob = user("BOB");
    let bx = s.cx(&bob, 2);
    let public = ChgAut {
        who: vec![Who::Public],
        data: Some(DataAuth::of(ifs::auth::data::RWX)),
        ..ChgAut::default()
    };
    s.ifs.chgaut(&bx, REG, &public).unwrap();
    s.ifs.chgaut(&bx, "/www", &public).unwrap();
    let d = Direct {
        ifs: &s.ifs,
        cx: &bx,
    };
    replace(&d, REG, b"six", 0o644).unwrap();
    let a = d.dspaut(REG).unwrap();
    assert!(
        a.entries
            .iter()
            .any(|e| e.role == Role::Public && e.data == "*RWX"),
        "{a:?}"
    );
    let zed = user("ZED");
    let zx = s.cx(&zed, 3);
    replace(
        &Direct {
            ifs: &s.ifs,
            cx: &zx,
        },
        REG,
        b"seven",
        0o644,
    )
    .unwrap();
    assert_eq!(s.get(&bx, REG), b"seven");
}
