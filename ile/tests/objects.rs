// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects in a store: create modules, bind programs and service
//! programs through binding directories, update a program, and read
//! everything back after a reopen.

#![allow(clippy::too_many_lines, clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use ile::store::{
    Ctx, ObjSpec, addbnddire, crtbnddir, crtmod, crtpgm, load_bnddir, load_program, rmvbnddire,
    updpgm,
};
use ile::{
    BindOptions, BndDirEntry, BndEntryKind, CreateData, ExportSpec, ImportTarget, ModuleObject,
    PgmKind, parse_binder_source,
};
use qsname::{LibSpec, Name};
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn open_new() -> (Arc<MemBlockDev>, Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let (qsys, lib) = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
        let lib = ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: name("QGPL"),
                type_code: types::LIB,
                attribute: String::new(),
                text: "General purpose".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        (qsys, lib)
    };
    (d, store, qsys, lib)
}

fn ctx(qsys: ObjectId, lib: ObjectId) -> Ctx {
    Ctx {
        qsys,
        names: NameContext {
            curlib: Some(lib),
            libl: vec![qsys, lib],
        },
        now: NOW,
        owner: "QSECOFR".into(),
        system: "QS400".into(),
    }
}

fn module(text: &str) -> ModuleObject {
    ModuleObject::compile(qir::parse(text).unwrap(), CreateData::default(), NOW).unwrap()
}

fn spec(lib: &str, n: &str) -> ObjSpec {
    ObjSpec {
        lib: if lib == "*LIBL" {
            LibSpec::Libl
        } else {
            LibSpec::Name(name(lib))
        },
        name: name(n),
    }
}

const MAIN: &str = "module MAIN\nextern proc UTIL(byval $n: i32) -> i32\nproc MAIN() entry {\nL0:\n  %a = callb UTIL(1)\n  ret\n}\n";
const UTIL1: &str = "module UTIL\nproc UTIL(byval $n: i32) -> i32 export {\nL0:\n  ret 1\n}\n";
const UTIL2: &str = "module UTIL\nproc UTIL(byval $n: i32) -> i32 export {\nL0:\n  ret 2\n}\nproc EXTRA() export {\nL0:\n  ret\n}\n";

#[test]
fn objects_round_trip_through_the_store() {
    let (d, mut store, qsys, lib) = open_new();
    let c = ctx(qsys, lib);
    // Modules, a binding directory and a service program.
    let (srv_id, bnddir_id) = {
        let mut t = store.begin().unwrap();
        crtmod(&mut t, &c, lib, name("UTIL"), &module(UTIL1), "utilities").unwrap();
        crtmod(&mut t, &c, lib, name("MAIN"), &module(MAIN), "main").unwrap();
        let src = "STRPGMEXP PGMLVL(*CURRENT)\n EXPORT SYMBOL('UTIL')\nENDPGMEXP\n";
        let mut opts = BindOptions::new("UTILS", PgmKind::SrvPgm);
        opts.export = ExportSpec::Source(parse_binder_source(src).unwrap(), src.into());
        let (srv, diags) =
            crtpgm(&mut t, &c, lib, &[spec("QGPL", "UTIL")], &[], opts, "utils").unwrap();
        assert!(diags.is_empty(), "{diags:?}");
        let bd = crtbnddir(&mut t, &c, lib, name("MYBND"), "").unwrap();
        addbnddire(
            &mut t,
            bd,
            vec![BndDirEntry {
                name: "UTILS".into(),
                lib: "*LIBL".into(),
                kind: BndEntryKind::SrvPgm,
                defer: false,
            }],
            None,
        )
        .unwrap();
        t.commit().unwrap();
        (srv, bd)
    };
    assert_eq!(srv_id.type_code, types::SRVPGM);
    // A program bound to the service program through the directory.
    let pgm_id = {
        let mut t = store.begin().unwrap();
        let (id, diags) = crtpgm(
            &mut t,
            &c,
            lib,
            &[spec("*LIBL", "MAIN")],
            &[spec("QGPL", "MYBND")],
            BindOptions::new("MAIN", PgmKind::Pgm),
            "main program",
        )
        .unwrap();
        assert!(diags.is_empty(), "{diags:?}");
        t.commit().unwrap();
        id
    };
    // Reopen and read everything back.
    drop(store);
    let mut store = Store::open(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0;
    {
        let mut t = store.begin().unwrap();
        let p = load_program(&mut t, pgm_id).unwrap();
        assert_eq!(p.name, "MAIN");
        assert_eq!(p.attr.kind, PgmKind::Pgm);
        assert_eq!(p.modules.len(), 1);
        assert_eq!(p.srvpgms.len(), 1);
        assert_eq!(p.srvpgms[0].name, "UTILS");
        assert_eq!(p.srvpgms[0].lib, "*LIBL");
        let util = p.imports.iter().find(|i| i.name == "UTIL").unwrap();
        assert_eq!(
            util.target,
            ImportTarget::SrvPgm {
                srvpgm: 0,
                export: "UTIL".into()
            }
        );
        let s = load_program(&mut t, srv_id).unwrap();
        assert_eq!(s.attr.kind, PgmKind::SrvPgm);
        assert_eq!(s.exports.len(), 1);
        assert_eq!(p.srvpgms[0].signature, s.signature().unwrap());
        let bd = load_bnddir(&mut t, bnddir_id).unwrap();
        assert_eq!(bd.entries.len(), 1);
        assert!(ile::dsp::dspbnddir("MYBND", &bd).contains("UTILS"));
        assert!(ile::dsp::dsppgmref(&p).contains("No objects referenced"));
        t.abort();
    }
    // UPDSRVPGM with a new UTIL module changes the exports under EXPORT(*SRCFILE)
    // only as the source says: UTIL stays the one export.
    {
        let mut t = store.begin().unwrap();
        let util = c
            .resolve(
                &mut t,
                LibSpec::Name(name("QGPL")),
                name("UTIL"),
                types::MODULE,
            )
            .unwrap();
        // Replace the module object: delete and create again.
        ops::delete(&mut t, store::obj::JobHandle(1), util).unwrap();
        crtmod(
            &mut t,
            &c,
            lib,
            name("UTIL"),
            &module(UTIL2),
            "utilities v2",
        )
        .unwrap();
        let diags = updpgm(&mut t, &c, srv_id, &[spec("QGPL", "UTIL")]).unwrap();
        assert!(diags.is_empty(), "{diags:?}");
        let s = load_program(&mut t, srv_id).unwrap();
        assert_eq!(s.exports.len(), 1);
        assert_eq!(
            s.modules[0].module.ir.procs.len(),
            2,
            "the new module is bound"
        );
        let n = rmvbnddire(&mut t, bnddir_id, "UTILS", None, BndEntryKind::SrvPgm).unwrap();
        assert_eq!(n, 1);
        assert!(load_bnddir(&mut t, bnddir_id).unwrap().entries.is_empty());
        t.commit().unwrap();
    }
}

#[test]
fn wrong_types_are_refused() {
    let (_d, mut store, qsys, lib) = open_new();
    let c = ctx(qsys, lib);
    let mut t = store.begin().unwrap();
    let bd = crtbnddir(&mut t, &c, lib, name("X"), "").unwrap();
    let err = load_program(&mut t, bd).unwrap_err().to_string();
    assert!(err.contains("*BNDDIR") && err.contains("*PGM"), "{err}");
    let err = crtpgm(
        &mut t,
        &c,
        lib,
        &[spec("QGPL", "NOPE")],
        &[],
        BindOptions::new("P", PgmKind::Pgm),
        "",
    )
    .unwrap_err();
    assert!(err.to_string().contains("NOPE"), "{err}");
    t.abort();
}
