// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message files in a store: install the system files, create and
//! edit a file, retrieve through the library list, merge.

#![allow(clippy::many_single_char_names)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use msgf::store::{
    Ctx, addmsgd, chgmsgd, crtmsgf, dspmsgd, install_builtin, load, mrgmsgf, rmvmsgd, rtvmsg,
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
fn system_files_install_once_and_resolve() {
    let (mut store, qsys, lib) = open_new();
    let mut t = store.begin().unwrap();
    let made = install_builtin(&mut t, &ctx(), qsys).unwrap();
    assert_eq!(made.len(), 7);
    assert!(install_builtin(&mut t, &ctx(), qsys).unwrap().is_empty());
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
            .contains("CPF2499")
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
