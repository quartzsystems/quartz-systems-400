// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An SQL package saved with its library and restored: its attributes
//! and statements come back as they were.

use std::io::Cursor;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::sqlpkg::{self, Package, PkgAttr, PkgKind, PkgStmt};
use qsname::Name;
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use store::obj::{CreateParams, JobHandle, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

#[test]
#[allow(clippy::many_single_char_names)] // the store's parts
fn a_package_round_trips() {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("PKGLIB").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    let name = Name::parse("QSPKG9899").unwrap();
    let attr = PkgAttr { kind: PkgKind::Extended, sql_naming: true, token: 42, default_collection: "PKGLIB".into(), ..PkgAttr::default() };
    let id = sqlpkg::create(&mut t, lib, name, &attr, "QSECOFR", "", NOW).unwrap();
    let pkg = Package {
        attr,
        stmts: vec![PkgStmt {
            name: "QZ000000000002A001".into(),
            stmt_type: 2,
            text: "SELECT CAST(? AS INTEGER) + 1 AS N FROM SYSIBM.SYSDUMMY1".into(),
            cols: vec![7; 96],
            markers: vec![8; 96],
            ..PkgStmt::default()
        }],
    };
    sqlpkg::save(&mut t, id, &pkg).unwrap();
    t.commit().unwrap();

    let h = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [7; 16],
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVLIB".into(),
        user: "QSYS".into(),
    };
    let mut w = Writer::new(Vec::new(), &h).unwrap();
    let opts = SaveOptions { update_history: false, ..SaveOptions::default() };
    save_library(&mut s, JobHandle(1), &AllowAll, qsys, "PKGLIB", &opts, &mut w).unwrap();
    let bytes = w.finish(0).unwrap().1;

    let mut t = s.begin().unwrap();
    sqlpkg::delete(&mut t, JobHandle(1), id).unwrap();
    assert!(sqlpkg::find(&mut t, lib, name).unwrap().is_none());
    t.commit().unwrap();
    let mut r = Reader::new(Cursor::new(&bytes[..])).unwrap();
    let rep = restore(&mut s, JobHandle(1), &AllowAll, qsys, &mut r, &RestoreOptions::default()).unwrap();
    assert!(rep.restored() >= 1, "{rep:?}");
    let mut t = s.begin().unwrap();
    let back = sqlpkg::find(&mut t, lib, name).unwrap().expect("restored");
    assert_eq!(sqlpkg::load(&mut t, back).unwrap(), pkg);
}
