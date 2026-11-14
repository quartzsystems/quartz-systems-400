// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message files in a store: install the system files, create and
//! edit a file, retrieve through the library list, merge.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use msgf::store::{
    Ctx, add_missing_descriptions, addmsgd, chgmsgd, crtmsgf, dspmsgd, install_builtin,
    install_product_files, load, mrgmsgf, rmvmsgd, rtvmsg,
};
use msgf::{MsgDesc, VarType};
use qsname::{LibSpec, Name};
use store::obj::{CreateParams, NameContext, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn open_new() -> (Store, ObjectId, ObjectId) {
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
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
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
    t.commit().unwrap();
    (store, qsys, lib)
}

fn ctx() -> Ctx {
    Ctx {
        now: NOW,
        owner: "QSECOFR".into(),
        system: "QS400".into(),
    }
}

fn desc(id: &str, text: &str) -> MsgDesc {
    MsgDesc {
        id: id.into(),
        severity: 20,
        text: text.into(),
        help: String::new(),
        vars: vec![VarType::Char(10)],
        dft_reply: None,
        valid_replies: Vec::new(),
        dump: Vec::new(),
    }
}

#[test]
fn an_older_files_missing_descriptions_are_added() {
    let (mut store, qsys, _) = open_new();
    let mut t = store.begin().unwrap();
    install_builtin(&mut t, &ctx(), qsys).unwrap();
    assert_eq!(add_missing_descriptions(&mut t, qsys).unwrap(), 0);
    let names = NameContext {
        curlib: None,
        libl: vec![qsys],
    };
    let f = ops::resolve(
        &mut t,
        qsys,
        &names,
        LibSpec::Name(Name::parse("QSYS").unwrap()),
        Name::parse("QCPFMSG").unwrap(),
        types::MSGF,
    )
    .unwrap()
    .id;
    // As a V1R1M0 file: no CPIB68D; one description the customer changed.
    rmvmsgd(&mut t, f, "CPIB68D").unwrap();
    let mut d = load(&mut t, f).unwrap().find("CPF9801").unwrap().clone();
    d.text = "changed here".into();
    chgmsgd(&mut t, f, d).unwrap();
    assert_eq!(add_missing_descriptions(&mut t, qsys).unwrap(), 1);
    let back = load(&mut t, f).unwrap();
    assert!(back.find("CPIB68D").is_some());
    assert_eq!(back.find("CPF9801").unwrap().text, "changed here");
    t.commit().unwrap();
}

#[test]
fn system_files_install_once_and_resolve() {
    let (mut store, qsys, lib) = open_new();
    let mut t = store.begin().unwrap();
    let made = install_builtin(&mut t, &ctx(), qsys).unwrap();
    assert_eq!(made.len(), 10);
    assert!(install_builtin(&mut t, &ctx(), qsys).unwrap().is_empty());
    // A product library's files come with the library, once.
    let made_p = install_product_files(&mut t, &ctx(), qsys, "QHTTPSVR").unwrap();
    assert_eq!(made_p.len(), 2);
    assert!(
        install_product_files(&mut t, &ctx(), qsys, "QHTTPSVR")
            .unwrap()
            .is_empty()
    );
    assert!(
        install_product_files(&mut t, &ctx(), qsys, "QNONE")
            .unwrap()
            .is_empty()
    );
    assert_eq!(load(&mut t, made_p[1]).unwrap().name, "QZHBMSG");
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![qsys, lib],
    };
    let m = rtvmsg(
        &mut t,
        qsys,
        &names,
        "MCH1211",
        name("QCPFMSG"),
        LibSpec::Libl,
    )
    .unwrap()
    .unwrap();
    assert_eq!(m.severity, 40);
    assert!(
        rtvmsg(
            &mut t,
            qsys,
            &names,
            "MCH0000",
            name("QCPFMSG"),
            LibSpec::Libl
        )
        .unwrap()
        .is_none()
    );
    let f = load(&mut t, made[0]).unwrap();
    assert_eq!(f.name, "QCPFMSG");
    assert!(f.msgs.len() > 10);
    t.commit().unwrap();
}

#[test]
fn user_files_are_edited_and_merged() {
    let (mut store, _qsys, lib) = open_new();
    let mut t = store.begin().unwrap();
    let c = ctx();
    let a = crtmsgf(&mut t, &c, lib, name("MYMSGF"), "mine", false).unwrap();
    addmsgd(&mut t, a, desc("USR0001", "Hello &1")).unwrap();
    assert!(
        addmsgd(&mut t, a, desc("USR0001", "again"))
            .unwrap_err()
            .to_string()
            .contains("CPF2412")
    );
    assert!(addmsgd(&mut t, a, desc("BAD", "x")).is_err());
    chgmsgd(&mut t, a, desc("USR0001", "Hi &1")).unwrap();
    assert!(
        chgmsgd(&mut t, a, desc("USR0002", "x"))
            .unwrap_err()
            .to_string()
            .contains("CPF2419")
    );
    let f = load(&mut t, a).unwrap();
    assert_eq!(f.find("USR0001").unwrap().text, "Hi &1");
    let data = ccsid::to_ebcdic("WORLD").unwrap();
    assert_eq!(f.find("USR0001").unwrap().format(&data), "Hi WORLD");
    let text = dspmsgd("MYMSGF", f.find("USR0001").unwrap());
    assert!(
        text.contains("USR0001") && text.contains("*CHAR 10"),
        "{text}"
    );

    let b = crtmsgf(&mut t, &c, lib, name("OTHER"), "", false).unwrap();
    addmsgd(&mut t, b, desc("USR0001", "theirs")).unwrap();
    addmsgd(&mut t, b, desc("USR0009", "nine")).unwrap();
    assert_eq!(mrgmsgf(&mut t, b, a, false).unwrap(), 1);
    assert_eq!(
        load(&mut t, a).unwrap().find("USR0001").unwrap().text,
        "Hi &1"
    );
    assert_eq!(mrgmsgf(&mut t, b, a, true).unwrap(), 2);
    assert_eq!(
        load(&mut t, a).unwrap().find("USR0001").unwrap().text,
        "theirs"
    );
    let gone = rmvmsgd(&mut t, a, "USR0009").unwrap();
    assert_eq!(gone.text, "nine");
    assert_eq!(load(&mut t, a).unwrap().msgs.len(), 1);
    t.commit().unwrap();
}

#[test]
fn a_secondary_language_library_comes_first_on_the_list() {
    let (mut store, qsys, lib) = open_new();
    let mut t = store.begin().unwrap();
    install_builtin(&mut t, &ctx(), qsys).unwrap();
    let es = msgf::nls::language(2931).unwrap();
    let (l2931, made) = msgf::store::install_language(&mut t, &ctx(), qsys, &es, false).unwrap();
    assert_eq!(made.len(), 3);
    assert!(
        msgf::store::install_language(&mut t, &ctx(), qsys, &es, false)
            .unwrap()
            .1
            .is_empty()
    );
    let look = |t: &mut store::Txn<'_>, libl: Vec<ObjectId>| {
        let names = NameContext {
            curlib: Some(lib),
            libl,
        };
        rtvmsg(t, qsys, &names, "CPF9810", name("QCPFMSG"), LibSpec::Libl)
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        look(&mut t, vec![l2931, qsys, lib]).text,
        "No se ha encontrado la biblioteca &1."
    );
    assert_eq!(
        look(&mut t, vec![qsys, lib]).text,
        "Library &1 was not found."
    );
    let f = load(&mut t, made[1]).unwrap();
    assert_eq!(f.ccsid, 284);
    t.commit().unwrap();
}
