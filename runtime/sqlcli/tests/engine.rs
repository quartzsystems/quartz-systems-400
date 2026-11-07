// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CLI's functions over a store in memory: the program's buffers
//! are byte vectors by pointer, statements run in an embedded SQL
//! session, and server-mode connections in a session of their own.

#![allow(clippy::many_single_char_names)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::embed::{Embedded, Reply, Request, Session};
use sql::{SqlContext, Value};
use sqlcli::consts::{conn_attr, env_attr, fetch, free, htype, len, param, rc, txn, ty};
use sqlcli::server::{self, Runner};
use sqlcli::{Cli, Host};
use store::obj::library::Library;
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

/// A store with `QSTEST`.
fn store() -> (
    Rc<RefCell<Store>>,
    store::obj::ObjectId,
    store::obj::ObjectId,
) {
    let d = Arc::new(MemBlockDev::with_seed(65536, 9));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
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
            name: Name::parse("QSTEST").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: "Test library".into(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    let _ = Library::load(&mut t, lib).unwrap();
    t.commit().unwrap();
    (Rc::new(RefCell::new(store)), qsys, lib)
}

fn session(
    st: &Rc<RefCell<Store>>,
    qsys: store::obj::ObjectId,
    lib: store::obj::ObjectId,
    handle: u64,
    job: &str,
) -> Session {
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(handle),
        NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
    );
    ctx.now = Some(NOW);
    ctx.current_schema = Some(Name::parse("QSTEST").unwrap());
    ctx.registers.job_name = Some(job.into());
    Session::with_ctx(Box::new(Rc::clone(st)), ctx)
}

/// The program's storage: one vector per pointer.
struct Mem {
    bufs: HashMap<u128, Vec<u8>>,
    next: u128,
    sql: Session,
}

impl Mem {
    fn buf(&mut self, b: &[u8]) -> u128 {
        self.next += 1;
        self.bufs.insert(self.next, b.to_vec());
        self.next
    }

    fn zeros(&mut self, n: usize) -> u128 {
        self.buf(&vec![0; n])
    }

    fn text(&mut self, s: &str) -> u128 {
        let mut b = ccsid::to_ebcdic(s).unwrap();
        b.push(0);
        self.buf(&b)
    }

    fn int(&self, p: u128) -> i32 {
        let b = &self.bufs[&p];
        i32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }

    fn short(&self, p: u128) -> i16 {
        let b = &self.bufs[&p];
        i16::from_be_bytes([b[0], b[1]])
    }

    fn set_int(&mut self, p: u128, v: i32) {
        self.bufs.get_mut(&p).unwrap()[..4].copy_from_slice(&v.to_be_bytes());
    }

    fn cstr(&self, p: u128) -> String {
        let b = &self.bufs[&p];
        let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        ccsid::from_ebcdic(&b[..end])
    }
}

impl Host for Mem {
    fn read(&self, p: u128, n: usize) -> Option<Vec<u8>> {
        self.bufs
            .get(&p)
            .filter(|b| b.len() >= n)
            .map(|b| b[..n].to_vec())
    }

    fn avail(&self, p: u128) -> usize {
        self.bufs.get(&p).map_or(0, Vec::len)
    }

    fn write(&mut self, p: u128, b: &[u8]) -> bool {
        match self.bufs.get_mut(&p) {
            Some(x) if x.len() >= b.len() => {
                x[..b.len()].copy_from_slice(b);
                true
            }
            _ => false,
        }
    }

    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>) {
        (self.sql.run(req), Vec::new())
    }
}

/// A server job's session.
struct Server(Session);

impl Runner for Server {
    fn run(&mut self, req: &Request) -> (Reply, Vec<(usize, Value)>) {
        (self.0.run(req), Vec::new())
    }
}

/// Allocate the environment, a connection and a statement, connected.
fn connected(cli: &mut Cli, m: &mut Mem, server_mode: bool) -> (i32, i32, i32) {
    let out = m.zeros(4);
    assert_eq!(cli.alloc_env(m, Some(out)), rc::SUCCESS);
    let env = m.int(out);
    if server_mode {
        let on = m.buf(&1i32.to_be_bytes());
        assert_eq!(
            cli.set_env_attr(m, env, env_attr::SERVER_MODE, Some(on), 0),
            rc::SUCCESS
        );
    }
    assert_eq!(cli.alloc_connect(m, env, Some(out)), rc::SUCCESS);
    let dbc = m.int(out);
    let dsn = m.text("*LOCAL");
    assert_eq!(
        cli.connect(m, dbc, Some(dsn), -3, None, 0, None, 0),
        rc::SUCCESS
    );
    let none = m.buf(&txn::NO_COMMIT.to_be_bytes());
    assert_eq!(
        cli.set_connect_attr(m, dbc, conn_attr::COMMIT, Some(none), 0),
        rc::SUCCESS
    );
    assert_eq!(cli.alloc_stmt(m, dbc, Some(out)), rc::SUCCESS);
    (env, dbc, m.int(out))
}

fn direct(cli: &mut Cli, m: &mut Mem, h: i32, text: &str) -> i32 {
    let t = m.text(text);
    cli.exec_direct(m, h, Some(t), len::NTS)
}

/// The first diagnostic of a handle: state and native error.
fn diag(cli: &mut Cli, m: &mut Mem, t: i16, h: i32) -> (String, i32) {
    let (st, nat, msg, ml) = (m.zeros(6), m.zeros(4), m.zeros(200), m.zeros(2));
    assert_eq!(
        cli.get_diag_rec(m, t, h, 1, Some(st), Some(nat), Some(msg), 200, Some(ml)),
        rc::SUCCESS
    );
    (m.cstr(st), m.int(nat))
}

#[test]
fn prepared_markers_bound_storage_and_diagnostics() {
    let (st, qsys, lib) = store();
    let mut m = Mem {
        bufs: HashMap::new(),
        next: 0,
        sql: session(&st, qsys, lib, 1, "000001/QUSER/TEST"),
    };
    let mut cli = Cli::default();
    let (env, dbc, h) = connected(&mut cli, &mut m, false);
    assert_eq!((env, dbc, h), (1, 2, 3));
    assert_ne!(
        direct(
            &mut cli,
            &mut m,
            h,
            "CREATE TABLE QSTEST.T (ID INT NOT NULL, NAME VARCHAR(10), AMT DECIMAL(7, 2))"
        ),
        rc::ERROR
    );
    // An INSERT with markers, its buffers read at each SQLExecute.
    let t = m.text("INSERT INTO QSTEST.T VALUES (?, ?, ?)");
    assert_eq!(cli.prepare(&mut m, h, Some(t), len::NTS), rc::SUCCESS);
    let np = m.zeros(2);
    assert_eq!(cli.num_params(&mut m, h, Some(np)), rc::SUCCESS);
    assert_eq!(m.short(np), 3);
    let (id, name, nl, amt, al) = (m.zeros(4), m.zeros(11), m.zeros(4), m.zeros(4), m.zeros(4));
    assert_eq!(
        cli.bind_param(
            h,
            1,
            param::INPUT,
            ty::INTEGER,
            ty::INTEGER,
            0,
            0,
            Some(id),
            0,
            None
        ),
        rc::SUCCESS
    );
    assert_eq!(
        cli.bind_param(
            h,
            2,
            param::INPUT,
            ty::CHAR,
            ty::VARCHAR,
            10,
            0,
            Some(name),
            0,
            Some(nl)
        ),
        rc::SUCCESS
    );
    assert_eq!(
        cli.bind_param(
            h,
            3,
            param::INPUT,
            ty::DECIMAL,
            ty::DECIMAL,
            7,
            2,
            Some(amt),
            0,
            Some(al)
        ),
        rc::SUCCESS
    );
    for (i, n, a) in [
        (1, "Ann", Some([0x00, 0x10, 0x50, 0x0F])),
        (2, "Bob", None),
        (3, "Cid", Some([0x00, 0x00, 0x72, 0x5D])),
    ] {
        m.set_int(id, i);
        m.bufs.insert(name, {
            let mut b = ccsid::to_ebcdic(n).unwrap();
            b.resize(11, 0);
            b
        });
        m.set_int(nl, len::NTS);
        match a {
            Some(b) => {
                m.bufs.insert(amt, b.to_vec());
                m.set_int(al, 0);
            }
            None => m.set_int(al, len::NULL_DATA),
        }
        assert_eq!(cli.execute(&mut m, h), rc::SUCCESS);
        let rows = m.zeros(4);
        assert_eq!(cli.row_count(&mut m, h, Some(rows)), rc::SUCCESS);
        assert_eq!(m.int(rows), 1);
    }
    // A query with a marker, its columns bound.
    let q = m.text("SELECT ID, NAME, AMT FROM QSTEST.T WHERE ID >= ? ORDER BY ID");
    assert_eq!(cli.prepare(&mut m, h, Some(q), len::NTS), rc::SUCCESS);
    let nc = m.zeros(2);
    assert_eq!(cli.num_result_cols(&mut m, h, Some(nc)), rc::SUCCESS);
    assert_eq!(m.short(nc), 3);
    let (cname, cnl, cty, cdef, csc, cnu) = (
        m.zeros(20),
        m.zeros(2),
        m.zeros(2),
        m.zeros(4),
        m.zeros(2),
        m.zeros(2),
    );
    assert_eq!(
        cli.describe_col(
            &mut m,
            h,
            3,
            Some(cname),
            20,
            Some(cnl),
            Some(cty),
            Some(cdef),
            Some(csc),
            Some(cnu)
        ),
        rc::SUCCESS
    );
    assert_eq!(
        (
            m.cstr(cname),
            m.short(cty),
            m.int(cdef),
            m.short(csc),
            m.short(cnu)
        ),
        ("AMT".into(), ty::DECIMAL, 7, 2, 1)
    );
    let min = m.buf(&2i32.to_be_bytes());
    assert_eq!(
        cli.bind_param(
            h,
            1,
            param::INPUT,
            ty::INTEGER,
            ty::INTEGER,
            0,
            0,
            Some(min),
            0,
            None
        ),
        rc::SUCCESS
    );
    let (oid, oname, onl, oamt, oal) = (m.zeros(4), m.zeros(8), m.zeros(4), m.zeros(4), m.zeros(4));
    assert_eq!(
        cli.bind_col(h, 1, ty::INTEGER, Some(oid), 4, None),
        rc::SUCCESS
    );
    assert_eq!(
        cli.bind_col(h, 2, ty::CHAR, Some(oname), 8, Some(onl)),
        rc::SUCCESS
    );
    assert_eq!(
        cli.bind_col(h, 3, ty::DECIMAL, Some(oamt), 7 * 256 + 2, Some(oal)),
        rc::SUCCESS
    );
    assert_eq!(cli.execute(&mut m, h), rc::SUCCESS);
    assert_eq!(cli.fetch(&mut m, h), rc::SUCCESS);
    assert_eq!(
        (m.int(oid), m.cstr(oname), m.int(onl), m.int(oal)),
        (2, "Bob".into(), len::NTS, len::NULL_DATA)
    );
    assert_eq!(cli.fetch(&mut m, h), rc::SUCCESS);
    assert_eq!(
        (m.int(oid), m.bufs[&oamt].clone()),
        (3, vec![0x00, 0x00, 0x72, 0x5D])
    );
    assert_eq!(cli.fetch(&mut m, h), rc::NO_DATA);
    assert_eq!(cli.close_cursor(&mut m, h), rc::SUCCESS);
    // New values in the same storage, the statement run again.
    m.set_int(min, 3);
    assert_eq!(cli.execute(&mut m, h), rc::SUCCESS);
    assert_eq!(cli.fetch(&mut m, h), rc::SUCCESS);
    assert_eq!(m.int(oid), 3);
    assert_eq!(cli.fetch(&mut m, h), rc::NO_DATA);
    assert_eq!(cli.free_stmt(&mut m, h, free::CLOSE), rc::SUCCESS);
    // Diagnostics: an undefined table, then the record taken.
    assert_eq!(
        direct(&mut cli, &mut m, h, "SELECT * FROM QSTEST.NOSUCH"),
        rc::ERROR
    );
    assert_eq!(
        diag(&mut cli, &mut m, htype::STMT, h),
        ("42704".into(), -204)
    );
    let (st5, nat, msg, ml) = (m.zeros(6), m.zeros(4), m.zeros(100), m.zeros(2));
    assert_eq!(
        cli.error(
            &mut m,
            env,
            dbc,
            h,
            Some(st5),
            Some(nat),
            Some(msg),
            100,
            Some(ml)
        ),
        rc::SUCCESS
    );
    assert_eq!(
        cli.error(
            &mut m,
            env,
            dbc,
            h,
            Some(st5),
            Some(nat),
            Some(msg),
            100,
            Some(ml)
        ),
        rc::NO_DATA
    );
    // Not connected, freeing in order.
    assert_eq!(cli.disconnect(&mut m, dbc), rc::SUCCESS);
    assert_eq!(cli.disconnect(&mut m, dbc), rc::ERROR);
    assert_eq!(
        diag(&mut cli, &mut m, htype::DBC, dbc),
        ("08003".into(), -843)
    );
    assert_eq!(cli.free_connect(dbc), rc::SUCCESS);
    assert_eq!(cli.free_env(env), rc::SUCCESS);
    assert_eq!(cli.free_env(env), rc::INVALID_HANDLE);
}

#[test]
fn server_mode_runs_in_a_job_of_its_own() {
    let (st, qsys, lib) = store();
    let mut m = Mem {
        bufs: HashMap::new(),
        next: 0,
        sql: session(&st, qsys, lib, 1, "000001/QUSER/TEST"),
    };
    let server_store = Rc::clone(&st);
    server::set_connector(Some(Rc::new(move |_req| {
        let mut s = session(&server_store, qsys, lib, 2, "000002/QUSER/QSQSRVR");
        s.ctx.registers.server_mode_job_name = Some("000001/QUSER/TEST".into());
        Ok(Box::new(Server(s)) as Box<dyn Runner>)
    })));
    let mut cli = Cli::default();
    let (_, dbc, h) = connected(&mut cli, &mut m, true);
    assert_eq!(
        direct(
            &mut cli,
            &mut m,
            h,
            "SELECT QSYS2.JOB_NAME, QSYS2.SERVER_MODE_JOB_NAME FROM SYSIBM.SYSDUMMY1"
        ),
        rc::SUCCESS
    );
    let (a, b) = (m.zeros(30), m.zeros(30));
    assert_eq!(cli.bind_col(h, 1, ty::CHAR, Some(a), 30, None), rc::SUCCESS);
    assert_eq!(cli.bind_col(h, 2, ty::CHAR, Some(b), 30, None), rc::SUCCESS);
    assert_eq!(cli.fetch(&mut m, h), rc::SUCCESS);
    assert_eq!(
        (m.cstr(a), m.cstr(b)),
        ("000002/QUSER/QSQSRVR".into(), "000001/QUSER/TEST".into())
    );
    assert_eq!(cli.close_cursor(&mut m, h), rc::SUCCESS);
    // Scrolling needs a scrollable cursor.
    let on = m.buf(&1i32.to_be_bytes());
    assert_eq!(
        cli.set_stmt_attr(
            &mut m,
            h,
            sqlcli::consts::stmt_attr::CURSOR_SCROLLABLE,
            Some(on)
        ),
        rc::SUCCESS
    );
    assert_eq!(
        direct(&mut cli, &mut m, h, "SELECT * FROM (VALUES 1, 2, 3) X(N)"),
        rc::SUCCESS
    );
    let n = m.zeros(4);
    assert_eq!(
        cli.bind_col(h, 1, ty::INTEGER, Some(n), 4, None),
        rc::SUCCESS
    );
    assert_eq!(cli.fetch_scroll(&mut m, h, fetch::LAST, 0), rc::SUCCESS);
    assert_eq!(m.int(n), 3);
    assert_eq!(cli.fetch_scroll(&mut m, h, fetch::ABSOLUTE, 2), rc::SUCCESS);
    assert_eq!(m.int(n), 2);
    assert_eq!(cli.disconnect(&mut m, dbc), rc::SUCCESS);
    server::set_connector(None);
}

#[test]
fn server_mode_without_a_connector_is_refused() {
    let (st, qsys, lib) = store();
    let mut m = Mem {
        bufs: HashMap::new(),
        next: 0,
        sql: session(&st, qsys, lib, 1, "000001/QUSER/TEST"),
    };
    server::set_connector(None);
    let mut cli = Cli::default();
    let out = m.zeros(4);
    cli.alloc_env(&mut m, Some(out));
    let env = m.int(out);
    let on = m.buf(&1i32.to_be_bytes());
    assert_eq!(
        cli.set_env_attr(&mut m, env, env_attr::SERVER_MODE, Some(on), 0),
        rc::SUCCESS
    );
    cli.alloc_connect(&mut m, env, Some(out));
    let dbc = m.int(out);
    assert_eq!(
        cli.connect(&mut m, dbc, None, 0, None, 0, None, 0),
        rc::ERROR
    );
    assert_eq!(diag(&mut cli, &mut m, htype::DBC, dbc).0, "08001");
    // Once a connection exists the environment's attributes stay.
    assert_eq!(
        cli.set_env_attr(&mut m, env, env_attr::SERVER_MODE, Some(on), 0),
        rc::ERROR
    );
    assert_eq!(diag(&mut cli, &mut m, htype::ENV, env).0, "HY010");
}
