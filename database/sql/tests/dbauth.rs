// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database entry points the authority audit found unchecked: each
//! refuses a profile without the authority IBM requires (`CPF2189`,
//! SQL's `SQL0551`) and lets it through once it has it.

#![allow(clippy::many_single_char_names)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::DbError;
use db::cst::Kind;
use db::cst::ops::NewCst;
use db::trg::ops::{authorize_add, authorize_change};
use db::util::{CpyfOpts, MbrOpt, Target, cpyf};
use qsname::{LibSpec, Name};
use sec::auth::bits::Auth;
use sec::auth::cache::Session as AuthSession;
use sec::auth::grant;
use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords, usrprf};
use sec::guard::{SecGuard, as_job};
use sec::profile::shipped;
use sql::{Naming, RunOpts, SqlContext, runsqlstm};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig, StoreError};

const NOW: u64 = 1_767_225_600;

fn sysval(n: &str) -> String {
    if n == "QSECURITY" { "40".into() } else { String::new() }
}

/// A store with `QSTEST` and `QSJRN` (a schema, so journaled), the
/// objects `setup` made as the system, and `BOB` with no authority of
/// his own: `*PUBLIC` decides.
struct Fx {
    store: Store,
    qsys: ObjectId,
    names: NameContext,
    bob: Rc<RefCell<AuthSession>>,
}

impl Fx {
    fn new(setup: &str) -> Self {
        let d = Arc::new(MemBlockDev::with_seed(32768, 43));
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
        let lib = create(&mut t, qsys, "QSTEST", types::LIB);
        create(&mut t, lib, "TRGPGM", types::PGM);
        t.commit().unwrap();
        let names = NameContext { curlib: Some(lib), libl: vec![lib] };
        let mut fx = Self { store, qsys, names, bob: Rc::new(RefCell::new(AuthSession::new(Principal::default()))) };
        let r = fx.run(setup);
        assert_eq!(r.errors, 0, "{}", r.listing);

        let g = SecGuard::new(Some(qsys));
        g.set_sysval("QSECURITY", "40");
        fx.store.set_guard(Some(g));
        let mut t = fx.store.begin().unwrap();
        shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
        let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
        let env = Env { qsys, actor: &secofr, now: NOW, sysval: &sysval };
        usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", "BOB")])).unwrap();
        fx.bob = Rc::new(RefCell::new(AuthSession::new(Principal::load(&mut t, qsys, "BOB", 40).unwrap())));
        t.commit().unwrap();
        fx
    }

    fn run(&mut self, text: &str) -> sql::RunReport {
        let mut ctx = SqlContext::new(self.qsys, JobHandle(2), self.names.clone());
        ctx.now = Some(NOW);
        let opts = RunOpts { naming: Naming::Sql, ..RunOpts::default() };
        runsqlstm(&mut self.store, &mut ctx, text, &opts).unwrap()
    }

    /// `text` as BOB: the listing, and whether it failed.
    fn run_as_bob(&mut self, text: &str) -> (bool, String) {
        let bob = Rc::clone(&self.bob);
        let r = as_job(&bob, || self.run(text));
        (r.errors > 0, r.listing)
    }

    fn id(&mut self, lib: &str, name: &str, ty: u16) -> ObjectId {
        let mut t = self.store.begin().unwrap();
        if ty == types::LIB {
            return ops::find_library(&mut t, self.qsys, n(name)).unwrap().id;
        }
        let id = ops::resolve(&mut t, self.qsys, &self.names, LibSpec::Name(n(lib)), n(name), ty).unwrap().id;
        t.commit().unwrap();
        id
    }

    /// `*PUBLIC`'s authority to each object.
    fn public(&mut self, objects: &[(ObjectId, Auth)]) {
        let mut t = self.store.begin().unwrap();
        for (id, a) in objects {
            grant::set_public(&mut t, *id, Some(*a)).unwrap();
        }
        t.commit().unwrap();
    }

    /// `f` as BOB in one transaction, rolled back.
    fn as_bob<T>(&mut self, f: impl FnOnce(&mut store::txn::Txn<'_>) -> T) -> T {
        let bob = Rc::clone(&self.bob);
        let mut t = self.store.begin().unwrap();
        let r = as_job(&bob, || f(&mut t));
        drop(t);
        r
    }
}

fn n(s: &str) -> Name {
    Name::parse(s).unwrap()
}

fn create(t: &mut store::txn::Txn<'_>, library: ObjectId, name: &str, type_code: u16) -> ObjectId {
    ops::create(
        t,
        &CreateParams {
            library,
            name: n(name),
            type_code,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap()
}

fn refused<T: std::fmt::Debug>(r: db::Result<T>) {
    match r {
        Err(DbError::Store(StoreError::NotAuthorized { msgid: "CPF2189", .. })) => {}
        other => panic!("not refused with CPF2189: {other:?}"),
    }
}

fn denied(listing: &str) -> bool {
    listing.contains("SQL0551")
}

fn with(words: &[&str]) -> Auth {
    Auth::parse(words, false).unwrap()
}

#[test]
fn next_value_needs_usage() {
    let mut fx = Fx::new("CREATE SEQUENCE QSTEST.SEQ1;");
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    let seq = fx.id("QSTEST", "SEQ1", types::DTAARA);
    fx.public(&[(lib, Auth::USE), (seq, Auth::USE)]);
    let (failed, l) = fx.run_as_bob("VALUES NEXT VALUE FOR QSTEST.SEQ1;");
    assert!(failed && denied(&l), "{l}");
    fx.public(&[(seq, Auth::CHANGE)]);
    let (failed, l) = fx.run_as_bob("VALUES NEXT VALUE FOR QSTEST.SEQ1;");
    assert!(!failed, "{l}");
}

#[test]
fn alter_table_needs_alter_before_anything_changes() {
    let mut fx = Fx::new("CREATE TABLE QSTEST.T1 (ID INT NOT NULL, TS TIMESTAMP NOT NULL GENERATED ALWAYS FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP);");
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    let t1 = fx.id("QSTEST", "T1", types::FILE);
    fx.public(&[(lib, Auth::USE), (t1, Auth::CHANGE)]);
    // DROP GENERATED changes no record format: it was not checked at all.
    let (failed, l) = fx.run_as_bob("ALTER TABLE QSTEST.T1 ALTER COLUMN TS DROP GENERATED;");
    assert!(failed && denied(&l), "{l}");
    fx.public(&[(t1, Auth::ALL)]);
    let (failed, l) = fx.run_as_bob("ALTER TABLE QSTEST.T1 ALTER COLUMN TS DROP GENERATED;");
    assert!(!failed, "{l}");
}

#[test]
fn a_foreign_key_needs_objref_to_the_parent() {
    let mut fx = Fx::new(
        "CREATE TABLE QSTEST.PAR (ID INT NOT NULL PRIMARY KEY);
         CREATE TABLE QSTEST.CHD (ID INT NOT NULL, PID INT);",
    );
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    let par = fx.id("QSTEST", "PAR", types::FILE);
    let chd = fx.id("QSTEST", "CHD", types::FILE);
    fx.public(&[(lib, Auth::USE), (par, Auth::USE), (chd, Auth::ALL)]);
    let fk = NewCst { parent: par, ..NewCst::new(Kind::Referential, vec![n("PID")]) };
    refused(fx.as_bob(|t| db::cst::ops::add(t, chd, &fk, NOW)));
    let (failed, l) = fx.run_as_bob("ALTER TABLE QSTEST.CHD ADD FOREIGN KEY (PID) REFERENCES QSTEST.PAR (ID);");
    assert!(failed && denied(&l), "{l}");
    // *OBJREF, or *OBJMGT, is enough.
    fx.public(&[(par, with(&["*USE", "*OBJREF"]))]);
    let (failed, l) = fx.run_as_bob("ALTER TABLE QSTEST.CHD ADD FOREIGN KEY (PID) REFERENCES QSTEST.PAR (ID);");
    assert!(!failed, "{l}");
    // A constraint on a file needs *OBJMGT or *OBJALTER, not *OBJOPR.
    fx.public(&[(chd, Auth::CHANGE)]);
    let uq = NewCst::new(Kind::Unique, vec![n("ID")]);
    refused(fx.as_bob(|t| db::cst::ops::add(t, chd, &uq, NOW)));
    fx.public(&[(chd, with(&["*OBJALTER"]))]);
    assert!(fx.as_bob(|t| db::cst::ops::add(t, chd, &uq, NOW)).is_ok());
}

#[test]
fn cpyf_replace_needs_objmgt_and_dlt() {
    let mut fx = Fx::new(
        "CREATE TABLE QSTEST.FROMF (ID INT NOT NULL);
         CREATE TABLE QSTEST.TOF (ID INT NOT NULL);
         INSERT INTO QSTEST.FROMF VALUES (1), (2);",
    );
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    let from = fx.id("QSTEST", "FROMF", types::FILE);
    let to = fx.id("QSTEST", "TOF", types::FILE);
    fx.public(&[(lib, Auth::USE), (from, Auth::USE), (to, with(&["*OBJOPR", "*ADD"]))]);
    let opts = |mbropt| CpyfOpts { mbropt, now: NOW, ..CpyfOpts::default() };
    assert!(fx.as_bob(|t| cpyf(t, JobHandle(5), from, &Target::File(to), &opts(MbrOpt::Add))).is_ok());
    refused(fx.as_bob(|t| cpyf(t, JobHandle(5), from, &Target::File(to), &opts(MbrOpt::Replace))));
    refused(fx.as_bob(|t| cpyf(t, JobHandle(5), from, &Target::File(to), &opts(MbrOpt::UpdAdd))));
    fx.public(&[(to, with(&["*OBJOPR", "*OBJMGT", "*ADD", "*DLT"]))]);
    assert!(fx.as_bob(|t| cpyf(t, JobHandle(5), from, &Target::File(to), &opts(MbrOpt::Replace))).is_ok());
}

#[test]
fn addpftrg_needs_the_file_and_the_program() {
    let mut fx = Fx::new("CREATE TABLE QSTEST.TRGT (ID INT NOT NULL);");
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    let file = fx.id("QSTEST", "TRGT", types::FILE);
    let pgm = fx.id("QSTEST", "TRGPGM", types::PGM);
    fx.public(&[(lib, Auth::USE), (file, Auth::CHANGE), (pgm, Auth::USE)]);
    refused(fx.as_bob(|t| authorize_add(t, file, pgm, false)));
    refused(fx.as_bob(|t| authorize_change(t, file)));
    fx.public(&[(file, with(&["*USE", "*OBJALTER"]))]);
    assert!(fx.as_bob(|t| authorize_add(t, file, pgm, false)).is_ok());
    assert!(fx.as_bob(|t| authorize_change(t, file)).is_ok());
    // ALWREPCHG(*YES) needs *UPD too.
    refused(fx.as_bob(|t| authorize_add(t, file, pgm, true)));
    // The program: *EXECUTE.
    fx.public(&[(pgm, Auth::EXCLUDE)]);
    refused(fx.as_bob(|t| authorize_add(t, file, pgm, false)));
}

#[test]
fn rmvjrnchg_needs_the_files() {
    let mut fx = Fx::new(
        "CREATE SCHEMA QSJRN;
         CREATE TABLE QSJRN.J1 (ID INT NOT NULL);
         INSERT INTO QSJRN.J1 VALUES (1);",
    );
    let lib = fx.id("QSYS", "QSJRN", types::LIB);
    let file = fx.id("QSJRN", "J1", types::FILE);
    let jrn = fx.id("QSJRN", "QSQJRN", jrn::JRN);
    fx.public(&[(lib, Auth::USE), (file, Auth::CHANGE), (jrn, Auth::USE)]);
    let range = db::journal::JrnRange::default();
    refused(fx.as_bob(|t| db::journal::rmvjrnchg(t, JobHandle(6), jrn, Some(&[file]), &range, false, NOW)));
    refused(fx.as_bob(|t| db::journal::apyjrnchg(t, JobHandle(6), jrn, Some(&[file]), &range, false, NOW)));
    fx.public(&[(file, with(&["*CHANGE", "*OBJMGT"]))]);
    assert!(fx.as_bob(|t| db::journal::rmvjrnchg(t, JobHandle(6), jrn, Some(&[file]), &range, false, NOW)).is_ok());
    // APYJRNCHG needs *OBJEXIST as well.
    refused(fx.as_bob(|t| db::journal::apyjrnchg(t, JobHandle(6), jrn, Some(&[file]), &range, false, NOW)));
}

#[test]
fn a_package_is_found_only_in_a_usable_library() {
    let mut fx = Fx::new("");
    let lib = fx.id("QSYS", "QSTEST", types::LIB);
    fx.public(&[(lib, Auth::EXCLUDE)]);
    let e = fx.as_bob(|t| db::sqlpkg::find(t, lib, n("PKG1"))).unwrap_err();
    assert!(matches!(e, DbError::Store(StoreError::NotAuthorized { msgid: "CPF9820", .. })), "{e:?}");
    fx.public(&[(lib, Auth::USE)]);
    assert_eq!(fx.as_bob(|t| db::sqlpkg::find(t, lib, n("PKG1"))).unwrap(), None);
}
