// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL information on an object and a library's SQL name index.

mod common;

use common::{name, open_new, reopen};
use db::sqlinf::names::{self, Space, Target};
use db::sqlinf::{self, Kind, SqlInfo, tag};
use store::obj::{CreateParams, JobHandle, ops, types};

#[test]
fn info_survives_reopen_and_goes_with_its_owner() {
    let (d, mut store, lib) = open_new();
    let mut t = store.begin().unwrap();
    let area = ops::create(
        &mut t,
        &CreateParams {
            library: lib,
            name: name("ORDER_SEQ"),
            type_code: types::DTAARA,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: 1,
            by_system: false,
        },
    )
    .unwrap();
    assert!(sqlinf::load(&mut t, area).unwrap().is_none());
    let mut info = SqlInfo::new(Kind::Sequence);
    info.set(tag::NAME, b"ORDER_NUMBER_SEQUENCE".to_vec());
    info.set(tag::TEXT, vec![b'x'; 9000]);
    sqlinf::save(&mut t, area, &info, 2).unwrap();
    let to = Target { id: area, system: name("ORDER_SEQ") };
    names::put(&mut t, lib, Space::Sequence, "ORDER_NUMBER_SEQUENCE", to).unwrap();
    t.commit().unwrap();
    drop(store);

    let mut store = reopen(&d);
    let mut t = store.begin().unwrap();
    let got = sqlinf::load(&mut t, area).unwrap().unwrap();
    assert_eq!(got, info);
    assert_eq!(got.name().as_deref(), Some("ORDER_NUMBER_SEQUENCE"));
    assert_eq!(names::find(&mut t, lib, Space::Sequence, "ORDER_NUMBER_SEQUENCE").unwrap(), Some(to));
    assert_eq!(names::find(&mut t, lib, Space::Table, "ORDER_NUMBER_SEQUENCE").unwrap(), None);
    assert_eq!(names::list(&mut t, lib, Space::Sequence).unwrap(), vec![("ORDER_NUMBER_SEQUENCE".into(), to)]);

    let mut shorter = got.clone();
    shorter.set(tag::TEXT, b"short".to_vec());
    sqlinf::save(&mut t, area, &shorter, 3).unwrap();
    assert_eq!(sqlinf::load(&mut t, area).unwrap().unwrap(), shorter);

    let sql = ops::read_header(&mut t, area).unwrap().1.sql;
    assert!(!sql.is_none());
    ops::delete(&mut t, JobHandle(1), area).unwrap();
    assert!(ops::read_header(&mut t, sql).is_err(), "the *SQLINF went with its owner");
    names::remove(&mut t, lib, Space::Sequence, "ORDER_NUMBER_SEQUENCE").unwrap();
    assert!(names::list(&mut t, lib, Space::Sequence).unwrap().is_empty());
    t.commit().unwrap();
}
