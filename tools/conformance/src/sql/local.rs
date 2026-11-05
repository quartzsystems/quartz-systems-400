// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A case run in memory: a fresh store with `QSTEST` as `{lib}`, `*SQL`
//! naming, current schema `QSTEST`, each statement its own transaction
//! (autocommit), the outcome written in the kit's form.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Desc, Embedded, Op, Reply, Request, Session};
use sql::{Naming, Outcome, SqlContext, Statement, prepare_dynamic};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

use super::case::{self, Case};
use super::render;

/// The library a case writes in here.
pub const LIBRARY: &str = "QSTEST";
/// The clock: 2026-01-01 00:00:00 UTC.
pub const NOW: u64 = 1_767_225_600;

fn s(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Run `c` on a fresh store; the outcome text.
pub fn run(c: &Case) -> Result<String, String> {
    let dev = Arc::new(MemBlockDev::with_seed(65536, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut store =
        Store::format(vec![Box::new(dev) as Box<dyn BlockDev>], &params, StoreConfig::default()).map_err(s)?;
    let mut txn = store.begin().map_err(s)?;
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").map_err(s)?;
    let lib = ops::create(
        &mut txn,
        &CreateParams {
            library: qsys,
            name: Name::parse(LIBRARY).expect("valid"),
            type_code: types::LIB,
            attribute: String::new(),
            text: "SQL conformance kit".into(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .map_err(s)?;
    txn.commit().map_err(s)?;
    let mut ctx = SqlContext::new(qsys, JobHandle(1), NameContext { curlib: Some(lib), libl: vec![lib] });
    ctx.now = Some(NOW);
    ctx.current_schema = Some(Name::parse(LIBRARY).expect("valid"));
    ctx.user = "QSECOFR".into();

    let store = Rc::new(RefCell::new(store));
    // Trigger programs run in a job of their own, with its own session over
    // the same store and record locks; a change calls them in its middle.
    let mut tctx = SqlContext::with_locks(qsys, JobHandle(1), ctx.names.clone(), ctx.locks.clone());
    tctx.now = Some(NOW);
    tctx.current_schema = ctx.current_schema;
    tctx.user = "QSECOFR".into();
    let tsession = Rc::new(RefCell::new(Session::with_ctx(Box::new(Rc::clone(&store)), tctx)));
    let tjob = Rc::new(RefCell::new(new_job(&store, &tsession, lib)));
    ctx.midcall = Some(Rc::new(move |plan| Some(mi::io::trigger::answer_plan(&mut tjob.borrow_mut(), &plan))));
    let session = Rc::new(RefCell::new(Session::with_ctx(Box::new(Rc::clone(&store)), ctx)));
    let mut job: Option<Box<mi::Job>> = None;

    let mut out = String::from("-- captured from qs400\n");
    for st in &c.statements {
        let text = st.replace("{lib}", LIBRARY);
        let _ = writeln!(out, "> {}", case::first(st));
        let prepared = prepare_dynamic(&text, Naming::Sql);
        let (sqlca, outcome) = if matches!(&prepared, Ok(p) if matches!(p.statement, Statement::Call(_))) {
            // A CALL runs the procedure's program in a job, as on the system.
            let j = job.get_or_insert_with(|| new_job(&store, &session, lib));
            call(j, &session, &text)
        } else {
            // Lent, so a change can give the store back to a trigger program.
            let mut txn = store::txn::Txn::lent(Box::new(|| store.borrow_mut())).map_err(s)?;
            let r = match &prepared {
                Ok(p) => p.run(&mut txn, &mut session.borrow_mut().ctx, &[]),
                Err(e) => (sql::Sqlca::from_error(e), None),
            };
            if r.0.sqlcode < 0 {
                txn.abort();
            } else {
                txn.commit().map_err(s)?;
            }
            r
        };
        let count = match &outcome {
            Some(Outcome::Count(n)) if case::changes(st) => Some(*n),
            _ if case::changes(st) && sqlca.sqlcode >= 0 => Some(u64::try_from(sqlca.sqlerrd[2]).unwrap_or(0)),
            _ => None,
        };
        // JDBC reports warnings apart; the status line has 0 for them. A
        // query with no rows is no warning over JDBC.
        let empty_query = sqlca.sqlcode == 100 && matches!(outcome, Some(Outcome::Rows(_)));
        if empty_query {
            out.push_str(&render::status(0, "00000", "-", count));
            out.push('\n');
        } else if sqlca.sqlcode > 0 {
            out.push_str(&render::status(0, "00000", "-", count));
            out.push('\n');
            let _ = writeln!(out, "warning {} {} {}", sqlca.sqlcode, sqlca.sqlstate, sqlca.msgid());
        } else {
            let msgid = if sqlca.sqlcode == 0 { "-".to_string() } else { sqlca.msgid() };
            out.push_str(&render::status(sqlca.sqlcode, &sqlca.sqlstate, &msgid, count));
            out.push('\n');
        }
        if let Some(Outcome::Rows(rs)) = &outcome {
            out.push_str(&render::rows(rs));
        }
    }
    Ok(out.replace(LIBRARY, "{lib}"))
}

/// The session as the job's embedded SQL.
struct Shared(Rc<RefCell<Session>>);

impl Embedded for Shared {
    fn run(&mut self, req: &Request) -> Reply {
        self.0.borrow_mut().run(req)
    }
}

/// Programs from the store: the procedures' `*PGM`s in `{lib}`.
struct StoreSource {
    store: Rc<RefCell<store::Store>>,
    lib: store::obj::ObjectId,
}

impl mi::act::ObjectSource for StoreSource {
    fn program(&mut self, name: &str, _lib: &str, kind: ile::PgmKind) -> Option<(Arc<ile::ProgramObject>, String)> {
        let mut st = self.store.borrow_mut();
        let mut txn = st.begin().ok()?;
        let ty = if kind == ile::PgmKind::Pgm { types::PGM } else { types::SRVPGM };
        let l = store::obj::library::Library::load(&mut txn, self.lib).ok()?;
        let id = l.lookup(&mut txn, ty, Name::parse(name).ok()?).ok()??;
        let p = ile::store::load_program(&mut txn, id).ok()?;
        txn.abort();
        Some((Arc::new(p), LIBRARY.into()))
    }
}

fn new_job(store: &Rc<RefCell<store::Store>>, session: &Rc<RefCell<Session>>, lib: store::obj::ObjectId) -> Box<mi::Job> {
    let mut job = mi::Job::boxed(Box::new(StoreSource { store: Rc::clone(store), lib }));
    qrpgrt::install(&mut job);
    job.sql = Some(Box::new(Shared(Rc::clone(session))));
    job
}

/// A `CALL` through the job: its outcome, and its first result set as
/// the statement's rows (what JDBC's `execute` shows).
fn call(job: &mut mi::Job, session: &Rc<RefCell<Session>>, text: &str) -> (sql::Sqlca, Option<Outcome>) {
    const SCOPE: &str = "1:SQLKIT";
    let req = Request {
        scope: SCOPE.into(),
        desc: Desc { op: Op::Exec, sql_naming: true, text: text.into(), ..Desc::default() },
        inputs: Vec::new(),
    };
    let (reply, _) = qrpgrt::sqlcall::run_request(job, req);
    let first = reply.rows.first().map(|r| r[0].as_text());
    let rs = first.and_then(|n| session.borrow_mut().take_result_set(SCOPE, n.trim()));
    (reply.sqlca, rs.map(Outcome::Rows))
}
