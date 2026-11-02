// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A query reads a table only with the authority a read open needs:
//! a profile excluded from a table gets SQL0551, one with `*USE` reads it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::{LibSpec, Name};
use sec::auth::bits::Auth;
use sec::auth::cache::Session as AuthSession;
use sec::auth::grant;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::guard::{SecGuard, as_job};
use sec::profile::shipped;
use sql::embed::{Desc, Embedded, Op, Request, Session};
use sql::{Naming, RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn sysval(n: &str) -> String {
    if n == "QSECURITY" { "40".into() } else { String::new() }
}

fn query(s: &mut Session) -> i32 {
    s.run(&Request {
        scope: "1:A".into(),
        desc: Desc {
            op: Op::Open,
            name: "C1".into(),
            text: "SELECT ID FROM QSTEST.SECRET".into(),
            sql_naming: true,
            ..Desc::default()
        },
        inputs: Vec::new(),
    })
    .sqlca
    .sqlcode
}

#[test]
fn a_query_needs_read_authority() {
    let d = Arc::new(MemBlockDev::with_seed(32768, 41));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QSTEST").unwrap(),
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
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![lib],
    };

    // The table, as the system.
    let mut ctx = SqlContext::new(qsys, JobHandle(2), names.clone());
    ctx.now = Some(NOW);
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = runsqlstm(
        &mut store,
        &mut ctx,
        "CREATE TABLE QSTEST.SECRET (ID INT NOT NULL); INSERT INTO QSTEST.SECRET VALUES (1);",
        &opts,
    )
    .unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);

    // BOB, and the guard; the library is *USE, the table *EXCLUDE.
    let g = SecGuard::new(Some(qsys));
    g.set_sysval("QSECURITY", "40");
    store.set_guard(Some(g.clone()));
    let mut t = store.begin().unwrap();
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "BOB")])).unwrap();
    let table = ops::resolve(&mut t, qsys, &names, LibSpec::Name(Name::parse("QSTEST").unwrap()), Name::parse("SECRET").unwrap(), types::FILE)
        .unwrap()
        .id;
    grant::set_public(&mut t, lib, Some(Auth::USE)).unwrap();
    grant::set_public(&mut t, table, Some(Auth::EXCLUDE)).unwrap();
    let bob = Rc::new(RefCell::new(AuthSession::new(Principal::load(&mut t, qsys, "BOB", 40).unwrap())));
    t.commit().unwrap();

    let store = Rc::new(RefCell::new(store));
    let mut s = Session::new(Box::new(Rc::clone(&store)), qsys, JobHandle(1), names.clone());
    s.ctx.now = Some(NOW);
    assert_eq!(as_job(&bob, || query(&mut s)), -551);

    // With *USE it reads.
    {
        let mut st = store.borrow_mut();
        let mut t = st.begin().unwrap();
        grant::set_public(&mut t, table, Some(Auth::USE)).unwrap();
        t.commit().unwrap();
    }
    let mut s = Session::new(Box::new(Rc::clone(&store)), qsys, JobHandle(3), names);
    s.ctx.now = Some(NOW);
    assert_eq!(as_job(&bob, || query(&mut s)), 0);
}
