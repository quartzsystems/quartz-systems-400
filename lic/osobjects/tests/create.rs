// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operating system's objects in an empty store, twice.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::library::Library;
use store::obj::{CreateParams, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

#[test]
fn every_piece_is_made_once() {
    let d = Arc::new(MemBlockDev::with_seed(131_072, 3));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 2048,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    for lib in ["QGPL", "QUSRSYS", "QSYS2", "QRECOVERY", "QSPL"] {
        ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse(lib).unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: String::new(),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
        )
        .unwrap();
    }
    t.commit().unwrap();
    let stamp = wm::obj::Stamp {
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
    };
    let mut t = store.begin().unwrap();
    let r = osobjects::create(&mut t, qsys, &stamp).unwrap();
    t.commit().unwrap();
    assert!(r.problems.is_empty(), "{:?}", r.problems);
    assert!(r.failed_commands.is_empty(), "{:?}", r.failed_commands);
    assert!(r.made > 300, "{} objects", r.made);
    let mut t = store.begin().unwrap();
    let l = Library::load(&mut t, qsys).unwrap();
    for (name, ty) in [
        ("QSTRUP", types::PGM),
        ("CRTLIB", types::CMD),
        ("QCPFMSG", types::MSGF),
    ] {
        assert!(
            l.lookup(&mut t, ty, Name::parse(name).unwrap())
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
    // Restores and PTFs are shipped *PUBLIC *EXCLUDE; ordinary commands are not.
    for (name, excluded) in [("RSTLIB", true), ("LODPTF", true), ("CRTLIB", false)] {
        let id = l
            .lookup(&mut t, types::CMD, Name::parse(name).unwrap())
            .unwrap()
            .unwrap();
        let (_, h) = ops::read_header(&mut t, id).unwrap();
        assert_eq!(h.public_authority == store::obj::Authority::EXCLUDE, excluded, "{name}");
    }
    let again = osobjects::create(&mut t, qsys, &stamp).unwrap();
    t.commit().unwrap();
    assert_eq!((again.made, again.commands_replaced), (0, 0));
}
