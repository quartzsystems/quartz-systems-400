// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `callproc` trace with the SQL procedure made and called for real:
//! `CREATE PROCEDURE` compiles its body to a `*PGM`, the `CALL` runs it
//! in a job's runtime as a database server job does.

mod common;

use std::sync::Arc;

use common::{TraceAuth, accept, replay, trace};
use hostserver::database::{Database, DbConfig};
use hostserver::{Handler, NoExits};
use ile::{PgmKind, ProgramObject};
use mi::act::ObjectSource;
use qsname::Name;
use sql::embed::{Embedded, Reply, Request, Session};
use sql::value::Value;
use store::obj::library::Library;
use store::obj::{ObjectId, types};

type Shared = std::rc::Rc<std::cell::RefCell<store::Store>>;

/// Programs from the store's `QSTEST`.
struct StoreSource {
    store: Shared,
    lib: ObjectId,
}

impl ObjectSource for StoreSource {
    fn program(&mut self, name: &str, _lib: &str, kind: PgmKind) -> Option<(Arc<ProgramObject>, String)> {
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().ok()?;
        let ty = if kind == PgmKind::Pgm { types::PGM } else { types::SRVPGM };
        let id = Library::load(&mut txn, self.lib).ok()?.lookup(&mut txn, ty, Name::parse(name).ok()?).ok()??;
        let p = ile::store::load_program(&mut txn, id).ok()?;
        txn.abort();
        Some((Arc::new(p), "QSTEST".into()))
    }
}

/// The server's SQL in a job: calls made in its runtime.
struct JobSql {
    job: Box<mi::Job>,
    outs: Vec<(usize, Value)>,
}

impl Embedded for JobSql {
    fn run(&mut self, req: &Request) -> Reply {
        let (r, outs) = qrpgrt::sqlcall::run_request(&mut self.job, req.clone());
        self.outs = outs;
        r
    }

    fn take_outs(&mut self) -> Vec<(usize, Value)> {
        std::mem::take(&mut self.outs)
    }
}

/// As the other database replays mask.
fn masks() -> common::Masks {
    common::Masks {
        template: vec![(0x2800, 4..8)],
        param: vec![(0x3807, 16..96), (0x3807, 100..104), (0x3807, 116..120), (0x3804, 0..2), (0x3804, 110..116), (0x380E, 0..4), (0x3814, 0..10)],
        skip: vec![0x3802, 0x3803],
    }
}

#[test]
fn callproc_in_a_job() {
    let t = trace("database", "callproc");
    let auth = TraceAuth::of(&t);
    let cfg = DbConfig {
        rdb: "S062B175".into(),
        job: ("QZDASOINIT".into(), "QUSER".into(), "007839".into()),
    };
    let diffs = replay(&t, &masks(), &mut || {
        let (store, qsys, names) = common::mem::store();
        let lib = names.curlib.expect("QSTEST");
        let session = Session::new(Box::new(store.clone()), qsys, store::obj::JobHandle(1), names);
        let mut job = mi::Job::boxed(Box::new(StoreSource { store, lib }));
        qrpgrt::install(&mut job);
        job.sql = Some(Box::new(session));
        let e = JobSql { job, outs: Vec::new() };
        Box::new(Database::new(auth.clone(), e, NoExits, cfg.clone())) as Box<dyn Handler>
    });
    accept("database-callproc-job", &diffs);
}
