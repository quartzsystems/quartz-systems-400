// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPG commands' parameters as compile options, the SQL scan, and
//! a compiled member made into a module and a program in a store.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use cmd::analyze::{Env, MemSource, analyze};
use cmd::block::Parms;
use ile::CreateData;
use ile::store::Ctx;
use qsname::Name;
use rpgc::Command;
use store::obj::library::Library;
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

use super::object::{Made, Shape, make};
use super::opts::request;
use super::source::{Source, has_sql, member_of};

const NOW: u64 = 1_792_713_600;

/// The parameters of a command string, defaults filled, as the
/// processing program reads them.
fn parms(text: &str) -> Parms {
    let mut src = MemSource::default();
    for name in ["CRTBNDRPG", "CRTRPGMOD", "CRTSQLRPGI"] {
        src.put(sys_cpps::install::object(name, 0).expect("definition").def);
    }
    let mut env = Env::new(&mut src);
    let r = analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"));
    Parms::from_bound(&r.def, r.parms.clone())
}

fn source(lines: &[&str]) -> Source {
    let recs: Vec<(u32, u32, String)> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            (
                u32::try_from(i + 1).unwrap() * 100,
                261_023,
                (*l).to_string(),
            )
        })
        .collect();
    Source {
        member: member_of("SRC", "QRPGLESRC", "HELLO", &recs),
        text: "Says hello".into(),
    }
}

#[test]
fn crtbndrpg_defaults_and_choices() {
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO)")).unwrap();
    assert_eq!(r.command, Command::CrtBndRpg);
    assert_eq!(r.shape, Shape::Program);
    assert_eq!((r.name.as_str(), r.lib.as_str()), ("HELLO", "QGPL"));
    assert_eq!(
        (r.srcfile.as_str(), r.mbr.as_deref()),
        ("QRPGLESRC", Some("HELLO"))
    );
    assert!(r.dftactgrp && r.print && r.replace && r.truncnbr);
    assert_eq!(
        (r.genlvl, r.dbgview.as_str(), r.actgrp.as_str()),
        (10, "*STMT", "QILE")
    );
    let o = r.options("HELLO", "QGPL", &source(&["**FREE"]));
    assert_eq!(o.text, "Says hello");
    assert_eq!(o.src, ("SRC".into(), "QRPGLESRC".into(), "HELLO".into()));

    let r = request(
        "CRTBNDRPG",
        &parms(
            "CRTBNDRPG PGM(QGPL/HELLO) SRCFILE(DEV/QSRC) SRCMBR(HI) GENLVL(20) TEXT('Hi') \
             DFTACTGRP(*NO) ACTGRP(*NEW) OPTION(*NOGEN *SRCSTMT) DBGVIEW(*SOURCE) \
             OUTPUT(*NONE) REPLACE(*NO) DEFINE(TESTING) BNDDIR(TOOLS/UTIL QC2LE) USRPRF(*OWNER)",
        ),
    )
    .unwrap();
    assert_eq!(
        (r.srcfile.as_str(), r.srclib.as_str(), r.mbr.as_deref()),
        ("QSRC", "DEV", Some("HI"))
    );
    assert!(!r.dftactgrp && !r.print && !r.replace && r.usrprf_owner);
    assert_eq!(
        (r.genlvl, r.actgrp.as_str(), r.dbgview.as_str()),
        (20, "*NEW", "*SOURCE")
    );
    assert!(r.option.contains(&"*NOGEN".to_string()));
    assert_eq!(r.define, ["TESTING"]);
    assert_eq!(r.bnddir, ["TOOLS/UTIL", "QC2LE"]);
    assert_eq!(r.options("HELLO", "QGPL", &source(&[])).text, "Hi");
}

#[test]
fn crtrpgmod_and_crtsqlrpgi_shapes() {
    let r = request("CRTRPGMOD", &parms("CRTRPGMOD MODULE(QGPL/M1)")).unwrap();
    assert_eq!(
        (r.shape, r.command, r.dftactgrp),
        (Shape::Module, Command::CrtRpgMod, false)
    );
    let r = request("CRTSQLRPGI", &parms("CRTSQLRPGI OBJ(QGPL/S1)")).unwrap();
    assert_eq!(
        (r.shape, r.command, r.dftactgrp, r.sql),
        (Shape::Program, Command::CrtBndRpg, false, true)
    );
    assert_eq!((r.dbgview.as_str(), r.print), ("*NONE", false));
    let r = request(
        "CRTSQLRPGI",
        &parms("CRTSQLRPGI OBJ(QGPL/S1) OBJTYPE(*MODULE)"),
    )
    .unwrap();
    assert_eq!(r.shape, Shape::Module);
    let r = request(
        "CRTSQLRPGI",
        &parms("CRTSQLRPGI OBJ(QGPL/S1) OBJTYPE(*SRVPGM)"),
    )
    .unwrap();
    assert_eq!(r.shape, Shape::SrvPgm);
}

#[test]
fn ctlspec_and_bad_values() {
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG SRCMBR(HELLO)")).unwrap();
    assert_eq!(
        (r.name.as_str(), r.mbr.as_deref()),
        ("*CTLSPEC", Some("HELLO"))
    );
    assert_eq!(r.object_name(&source(&[])), "HELLO");
    let r = request("CRTBNDRPG", &parms("CRTBNDRPG")).unwrap();
    assert_eq!(r.mbr, None);
    let err = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(A) TGTRLS(V3R1M0)")).unwrap_err();
    assert!(err.starts_with("CPF0006"), "{err}");
    assert!(request("CRTBNDRPG", &parms("CRTBNDRPG PGM(A) TGTRLS(V7R5M0)")).is_ok());
}

#[test]
fn embedded_sql_is_seen_outside_comments() {
    assert!(has_sql(
        &source(&["**FREE", "exec sql select 1 into :x from sysibm.sysdummy1;"]).member
    ));
    assert!(has_sql(
        &source(&["     C/EXEC SQL", "     C+ SET :X = 1", "     C/END-EXEC"]).member
    ));
    assert!(!has_sql(
        &source(&["**FREE", "// exec sql is not here", "dsply 'exec';"]).member
    ));
    assert!(!has_sql(&source(&["      * EXEC SQL in a comment"]).member));
}

fn open_new() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
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
                name: Name::parse("QGPL").unwrap(),
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
    (store, qsys, lib)
}

#[test]
fn a_compiled_member_becomes_a_module_and_a_program() {
    let src = source(&[
        "**FREE",
        "dcl-s n int(10) inz(1);",
        "n += 1;",
        "*inlr = *on;",
    ]);
    let req = request("CRTBNDRPG", &parms("CRTBNDRPG PGM(QGPL/HELLO)")).unwrap();
    let opts = req.options("HELLO", "QGPL", &src);
    let compiled = rpgc::compile(
        src.member.clone(),
        &mut rpgfront::NoIncludes,
        &mut rpgc::NoExtFiles,
        &opts,
    );
    assert!(compiled.created, "{:?}", compiled.diags);
    assert!(
        compiled.listing.iter().any(|l| l.starts_with('1')),
        "the listing starts pages with FCFC 1"
    );
    let (mut store, qsys, lib) = open_new();
    let ctx = Ctx {
        qsys,
        names: NameContext {
            curlib: Some(lib),
            libl: vec![qsys, lib],
        },
        now: NOW,
        owner: "QSECOFR".into(),
        system: "QS400".into(),
    };
    let mut made = Made {
        shape: Shape::Program,
        name: "HELLO".into(),
        text: "Says hello".into(),
        replace: true,
        usrprf_owner: false,
        bnddir: Vec::new(),
        source: Some(vec!["**FREE".into()]),
    };
    let mut txn = store.begin().unwrap();
    make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap();
    let qgpl = Library::load(&mut txn, lib).unwrap();
    let hello = Name::parse("HELLO").unwrap();
    let pgm = qgpl
        .lookup(&mut txn, types::PGM, hello)
        .unwrap()
        .expect("the program");
    let program = ile::store::load_program(&mut txn, pgm).unwrap();
    assert_eq!(program.modules.len(), 1);
    assert!(
        qgpl.lookup(&mut txn, types::MODULE, hello)
            .unwrap()
            .is_none(),
        "the bind's module is gone"
    );
    // REPLACE(*NO) keeps what is there.
    made.replace = false;
    let err = make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap_err();
    assert!(err.starts_with("CPF5D0D"), "{err}");
    made.shape = Shape::Module;
    make(
        &mut txn,
        &ctx,
        lib,
        &made,
        &compiled.qir,
        CreateData::default(),
        NOW,
    )
    .unwrap();
    assert!(
        qgpl.lookup(&mut txn, types::MODULE, hello)
            .unwrap()
            .is_some()
    );
}
