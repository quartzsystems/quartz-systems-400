// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*CMD` objects in a store: create, change defaults, proxy, find
//! through the library list, analyze against them.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use cmd::analyze::{Env, analyze};
use cmd::def::Qual;
use cmd::payload::CmdObject;
use cmd::store::{
    Changes, Ctx, StoreSource, chgcmd, chgcmddft, crtcmd, crtprxcmd, dltcmd, dspcmd, find, load,
};
use qsname::Name;
use store::obj::{CreateParams, JobHandle, NameContext, ObjectId, ops, types};
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

const CRTLIB: &str = "\
 CMD PROMPT('Create Library') MAXPOS(2)
 PARM KWD(LIB) TYPE(*SNAME) LEN(10) MIN(1) PROMPT('Library')
 PARM KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*PROD) VALUES(*PROD *TEST) PROMPT('Library type')
 PARM KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) SPCVAL((*BLANK ' ')) PROMPT('Text')
";

#[test]
#[allow(clippy::too_many_lines)]
fn objects() {
    let (mut store, qsys, qgpl) = open_new();
    let names = NameContext {
        curlib: Some(qgpl),
        libl: vec![qsys, qgpl],
    };
    let mut t = store.begin().unwrap();
    let mut def = cmd::source::compile("CRTLIB", CRTLIB).unwrap();
    def.cpp = Qual::new("QLICRLIB", "QSYS");
    def.text = "Create a library".into();
    let object = CmdObject::new(def, CRTLIB.into(), NOW);
    let id = crtcmd(&mut t, &ctx(), qsys, name("CRTLIB"), &object, false, true).unwrap();
    assert_eq!(
        crtcmd(&mut t, &ctx(), qsys, name("CRTLIB"), &object, false, true)
            .unwrap_err()
            .msgid(),
        "CPF0201"
    );
    let again = crtcmd(&mut t, &ctx(), qsys, name("CRTLIB"), &object, true, true).unwrap();
    assert_ne!(id, again);
    let found = find(&mut t, qsys, &names, "CRTLIB", "*LIBL").unwrap();
    assert_eq!(found.id, again);
    let o = load(&mut t, found.id).unwrap();
    assert_eq!(o.def.cpp.name, "QLICRLIB");
    assert_eq!(o.source, CRTLIB);

    // CHGCMDDFT changes a default; DSPCMD shows it; the analyzer takes it.
    let ov = chgcmddft(&mut t, found.id, "TYPE(*TEST) TEXT('new')").unwrap();
    assert_eq!(ov.len(), 2);
    assert_eq!(
        chgcmddft(&mut t, found.id, "NOPE(1)").unwrap_err().msgid(),
        "CPD0202"
    );
    assert_eq!(
        chgcmddft(&mut t, found.id, "LIB(X)").unwrap_err().msgid(),
        "CPD0203"
    );
    let o = load(&mut t, found.id).unwrap();
    let shown = dspcmd(&o, "QSYS");
    assert!(shown.contains("TYPE       *TEST"), "{shown}");
    assert!(shown.contains("QLICRLIB"));
    {
        let mut src = StoreSource {
            txn: &mut t,
            qsys,
            names: names.clone(),
        };
        let mut env = Env::new(&mut src);
        let r = analyze(&mut env, "CRTLIB X").unwrap();
        assert_eq!(
            r.get("TYPE").unwrap().value.as_ref().unwrap().source(),
            "*TEST"
        );
        assert_eq!(
            r.get("TEXT").unwrap().value.as_ref().unwrap().source(),
            "'new'"
        );
        assert_eq!(analyze(&mut env, "NOPE X").unwrap_err().msgid(), "CPD0030");
    }

    // CHGCMD.
    chgcmd(
        &mut t,
        found.id,
        &Changes {
            allow: Some(cmd::def::Allow(cmd::def::Allow::BATCH)),
            text: Some("changed".into()),
            ..Changes::default()
        },
        NOW,
    )
    .unwrap();
    let o = load(&mut t, found.id).unwrap();
    assert_eq!(o.def.allow.names(), vec!["*BATCH"]);
    assert_eq!(o.def.text, "changed");
    assert_eq!(
        ops::describe(&mut t, found.id).unwrap().text_of(),
        "changed"
    );

    // A proxy in QGPL resolves to it.
    let p = crtprxcmd(
        &mut t,
        &ctx(),
        qgpl,
        name("MKLIB"),
        Qual::new("CRTLIB", "QSYS"),
        "proxy",
        false,
    )
    .unwrap();
    {
        let mut src = StoreSource {
            txn: &mut t,
            qsys,
            names: names.clone(),
        };
        let mut env = Env::new(&mut src);
        env.setting.interactive = false;
        let r = analyze(&mut env, "MKLIB X").unwrap();
        assert_eq!(r.name, "CRTLIB");
        assert_eq!(r.lib, "QSYS");
        let pr = load(src.txn, p).unwrap();
        assert!(dspcmd(&pr, "QGPL").contains("Proxy target"));
    }
    dltcmd(&mut t, JobHandle(0), p).unwrap();
    assert!(find(&mut t, qsys, &names, "MKLIB", "QGPL").is_err());
    t.commit().unwrap();
}

trait TextOf {
    fn text_of(&self) -> String;
}

impl TextOf for store::obj::ObjectHeader {
    fn text_of(&self) -> String {
        store::obj::header::text_of(&self.text)
    }
}
