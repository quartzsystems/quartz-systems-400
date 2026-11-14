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
#[allow(clippy::too_many_lines)]
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
    // IBM's 93 time zone descriptions, in WRKTIMZON order.
    let zones = osobjects::timzon::load_all(&mut t, qsys).unwrap();
    assert_eq!(zones.len(), 93);
    assert_eq!(zones.as_slice(), sysval::tz::shipped());
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
        assert_eq!(
            h.public_authority == store::obj::Authority::EXCLUDE,
            excluded,
            "{name}"
        );
    }
    // The primary language library, and Spanish installed beside it.
    assert!(
        l.lookup(&mut t, types::LIB, Name::parse("QSYS2924").unwrap())
            .unwrap()
            .is_some()
    );
    // The LDAP client's include members.
    let inc = l
        .lookup(&mut t, types::LIB, Name::parse("QSYSINC").unwrap())
        .unwrap()
        .unwrap();
    for (file, member, srctype, _, text) in osobjects::qsysinc::MEMBERS {
        let f = Library::load(&mut t, inc)
            .unwrap()
            .lookup(&mut t, types::FILE, Name::parse(file).unwrap())
            .unwrap()
            .unwrap();
        let m = db::source::read_member(
            &mut t,
            f,
            qsname::MemberSpec::Name(Name::parse(member).unwrap()),
        )
        .unwrap();
        assert_eq!(m.srctype, *srctype);
        assert_eq!(m.records.len(), text.lines().count(), "{file}({member})");
        assert_eq!(m.records[0].2, text.lines().next().unwrap().trim_end());
    }
    let (lib, n) = osobjects::nls::install_language(&mut t, qsys, &stamp, 2931, false).unwrap();
    assert_eq!((lib.as_str(), n), ("QSYS2931", 5));
    assert_eq!(
        osobjects::nls::install_language(&mut t, qsys, &stamp, 2931, false)
            .unwrap()
            .1,
        0
    );
    assert!(
        osobjects::nls::install_language(&mut t, qsys, &stamp, 2999, false)
            .unwrap_err()
            .starts_with("TXT0203")
    );
    let l = Library::load(&mut t, qsys).unwrap();
    let es = l
        .lookup(&mut t, types::LIB, Name::parse("QSYS2931").unwrap())
        .unwrap()
        .unwrap();
    let esl = Library::load(&mut t, es).unwrap();
    let f = esl
        .lookup(&mut t, types::FILE, Name::parse("QDSIGNON").unwrap())
        .unwrap()
        .unwrap();
    let df = dspf::decode(&wm::obj::read(&mut t, f).unwrap()).unwrap();
    assert_eq!(df.file.ccsid.unwrap_or(37), 37);
    let shown = format!("{df:?}");
    assert!(shown.contains("Inicio de sesi"), "the Spanish constants");
    let again = osobjects::create(&mut t, qsys, &stamp).unwrap();
    t.commit().unwrap();
    assert_eq!((again.made, again.commands_replaced), (0, 0));
}
