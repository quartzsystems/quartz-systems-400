// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The requester against the server in memory, the server driving the
//! real database: SQL through `sql` and record-level access through the
//! `db` open data path, over a store on a memory disk.

// The SQLCA is the error the backends return.
#![allow(clippy::result_large_err)]
// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::odp::{KeyArg, Odp, OpenOpts, Usage};
use ddm::client::{ConnectOpts, Connection};
use ddm::file::{self, Rec, Ufcb, ufcb};
use ddm::sec::{Mechanism, Proof};
use ddm::server::{
    Authenticator, Config, FileBackend, FileError, FileInfo, FileOpen, FileRecord, ReadAt, Section,
    Session, SqlBackend, SqlResult,
};
use ddm::sql::{ColType, Column, Kind, Sqlca, Value};
use qdecimal::Decimal;
use qsname::{MemberSpec, Name};
use sql::exec::TableInfo;
use sql::{Naming, Outcome, SqlContext, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
const USER: &str = "QSUSER";
const PASSWORD: &str = "Secret9";

/// The store and SQL session both backends share.
struct Engine {
    store: Store,
    ctx: SqlContext,
    prepared: HashMap<Section, sql::Prepared>,
    cursors: HashMap<Section, VecDeque<Vec<Value>>>,
    odps: HashMap<u32, (Odp, TableInfo)>,
    next: u32,
}

fn engine() -> Engine {
    let dev = Arc::new(MemBlockDev::with_seed(32768, 11));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(dev) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut txn = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut txn,
        &CreateParams {
            library: qsys,
            name: Name::parse("QSTEST").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: "DDM test".into(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    txn.commit().unwrap();
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
    );
    ctx.now = Some(NOW);
    let _: ObjectId = lib;
    Engine {
        store,
        ctx,
        prepared: HashMap::new(),
        cursors: HashMap::new(),
        odps: HashMap::new(),
        next: 1,
    }
}

fn ca_of(e: &sql::SqlError) -> Sqlca {
    let c = sql::Sqlca::from_error(e);
    Sqlca::error(c.sqlcode, &c.sqlstate, &c.sqlerrmc)
}

fn columns(rs: &sql::ResultSet) -> Vec<Column> {
    rs.columns
        .iter()
        .map(|c| Column {
            name: c.name.clone(),
            ty: ColType::parse(&c.data_type, c.nullable)
                .unwrap_or_else(|| ColType::new(Kind::VarChar, 256, 0, 0, true)),
            label: None,
        })
        .collect()
}

fn to_wire(v: &sql::Value, t: &ColType) -> Value {
    match v {
        sql::Value::Null => Value::Null,
        sql::Value::Num(d) => match t.kind {
            Kind::SmallInt | Kind::Integer | Kind::BigInt => {
                Value::Int(d.to_string().parse().unwrap_or(0))
            }
            _ => Value::Dec(d.clone()),
        },
        other => Value::Text(other.as_text()),
    }
}

fn from_wire(v: &Value) -> sql::Value {
    match v {
        Value::Null => sql::Value::Null,
        Value::Int(i) => sql::Value::int(*i),
        Value::Dec(d) => sql::Value::Num(d.clone()),
        Value::Double(f) => sql::Value::Num(Decimal::parse(&f.to_string()).unwrap()),
        Value::Text(t) => sql::Value::Str(t.clone()),
    }
}

impl Engine {
    fn run(&mut self, p: &sql::Prepared, params: &[Value], keep: bool) -> Result<Outcome, Sqlca> {
        let vals: Vec<sql::Value> = params.iter().map(from_wire).collect();
        let mut txn = self
            .store
            .begin()
            .map_err(|e| Sqlca::error(-30_000, "58008", &e.to_string()))?;
        match p.execute(&mut txn, &mut self.ctx, &vals) {
            Ok(o) => {
                if keep {
                    txn.commit()
                        .map_err(|e| Sqlca::error(-30_000, "58008", &e.to_string()))?;
                } else {
                    txn.abort();
                }
                Ok(o)
            }
            Err(e) => {
                txn.abort();
                Err(ca_of(&e))
            }
        }
    }
}

struct SqlSide(Rc<RefCell<Engine>>);

impl SqlBackend for SqlSide {
    fn prepare(&mut self, s: &Section, text: &str) -> SqlResult<Vec<Column>> {
        let p = prepare(text, Naming::Sql).map_err(|e| ca_of(&e))?;
        let mut e = self.0.borrow_mut();
        let cols = if matches!(p.statement, sql::Statement::Select(_)) {
            match e.run(&p, &[], false)? {
                Outcome::Rows(rs) => columns(&rs),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        e.prepared.insert(s.clone(), p);
        Ok(cols)
    }
    fn describe(&mut self, s: &Section) -> SqlResult<Vec<Column>> {
        let p = self
            .0
            .borrow()
            .prepared
            .get(s)
            .cloned()
            .ok_or_else(|| Sqlca::error(-518, "07003", ""))?;
        match self.0.borrow_mut().run(&p, &[], false)? {
            Outcome::Rows(rs) => Ok(columns(&rs)),
            _ => Ok(Vec::new()),
        }
    }
    fn execute(&mut self, s: &Section, params: &[Value]) -> SqlResult<Sqlca> {
        let p = self
            .0
            .borrow()
            .prepared
            .get(s)
            .cloned()
            .ok_or_else(|| Sqlca::error(-518, "07003", ""))?;
        match self.0.borrow_mut().run(&p, params, true)? {
            Outcome::Count(n) => Ok(Sqlca::rows(n)),
            _ => Ok(Sqlca::default()),
        }
    }
    fn execute_immediate(&mut self, text: &str) -> SqlResult<Sqlca> {
        let p = prepare(text, Naming::Sql).map_err(|e| ca_of(&e))?;
        match self.0.borrow_mut().run(&p, &[], true)? {
            Outcome::Count(n) => Ok(Sqlca::rows(n)),
            _ => Ok(Sqlca::default()),
        }
    }
    fn open(&mut self, s: &Section, params: &[Value]) -> SqlResult<Vec<Column>> {
        let p = self
            .0
            .borrow()
            .prepared
            .get(s)
            .cloned()
            .ok_or_else(|| Sqlca::error(-518, "07003", ""))?;
        let Outcome::Rows(rs) = self.0.borrow_mut().run(&p, params, true)? else {
            return Err(Sqlca::error(-507, "24501", ""));
        };
        let cols = columns(&rs);
        let rows = rs
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .zip(&cols)
                    .map(|(v, c)| to_wire(v, &c.ty))
                    .collect()
            })
            .collect();
        self.0.borrow_mut().cursors.insert(s.clone(), rows);
        Ok(cols)
    }
    fn fetch(&mut self, s: &Section, max: usize) -> SqlResult<Vec<Vec<Value>>> {
        let mut e = self.0.borrow_mut();
        let q = e
            .cursors
            .get_mut(s)
            .ok_or_else(|| Sqlca::error(-501, "24501", ""))?;
        Ok((0..max).map_while(|_| q.pop_front()).collect())
    }
    fn close(&mut self, s: &Section) -> SqlResult<()> {
        self.0.borrow_mut().cursors.remove(s);
        Ok(())
    }
    fn commit(&mut self) -> SqlResult<()> {
        Ok(())
    }
    fn rollback(&mut self) -> SqlResult<()> {
        Ok(())
    }
}

struct FileSide(Rc<RefCell<Engine>>);

fn ferr(e: impl std::fmt::Display) -> FileError {
    FileError {
        id: "CPF5029".into(),
        text: e.to_string(),
    }
}

impl FileBackend for FileSide {
    fn open(&mut self, req: &FileOpen) -> Result<(u32, FileInfo), FileError> {
        let e = &mut *self.0.borrow_mut();
        let name = sql::ast::ObjectName {
            schema: Some(sql::ast::Ident::new(&req.library)),
            name: sql::ast::Ident::new(&req.file),
        };
        let mut txn = e.store.begin().map_err(ferr)?;
        let t = e.ctx.resolve_table(&mut txn, &name).map_err(ferr)?;
        let mut opts = OpenOpts::input(e.ctx.job);
        opts.member = MemberSpec::Name(t.member.name);
        opts.keyed = req.keyed;
        opts.usage = Usage {
            input: req.input,
            output: req.output,
            update: req.update,
            delete: req.delete,
        };
        let odp = Odp::open(&mut txn, e.ctx.locks.clone(), t.id, opts).map_err(ferr)?;
        txn.commit().map_err(ferr)?;
        let f = &t.format.format;
        let info = FileInfo {
            file: req.file.clone(),
            library: req.library.clone(),
            member: t.member.name.to_string(),
            format: f.name.to_string(),
            reclen: u16::try_from(f.record_len).unwrap(),
            fields: u16::try_from(f.fields.len()).unwrap(),
            records: 0,
            keyed: !f.keys.is_empty(),
            ccsid: 37,
        };
        let h = e.next;
        e.next += 1;
        e.odps.insert(h, (odp, t));
        Ok((h, info))
    }
    fn read(&mut self, h: u32, at: &ReadAt, lock: bool) -> Result<Option<FileRecord>, FileError> {
        let e = &mut *self.0.borrow_mut();
        let (odp, t) = e.odps.get_mut(&h).ok_or_else(|| ferr("not open"))?;
        let mut txn = e.store.begin().map_err(ferr)?;
        let r = match at {
            ReadAt::First => {
                odp.setll(&mut txn, &KeyArg::Start).map_err(ferr)?;
                odp.read(&mut txn, lock)
            }
            ReadAt::Next => odp.read(&mut txn, lock),
            ReadAt::Rrn(n) => odp.chain(&mut txn, &KeyArg::Rrn(u64::from(*n)), lock),
            ReadAt::Key { key, fields } => {
                let f = &t.format.format;
                let mut vals = Vec::new();
                let mut at = 0usize;
                for k in f.keys.iter().take(*fields as usize) {
                    let mut fd = f.fields[k.field].clone();
                    let n = fd.byte_len as usize;
                    fd.offset = 0;
                    vals.push(
                        db::record::read_field(&fd, &key[at..at + n], &[], None).map_err(ferr)?,
                    );
                    at += n;
                }
                odp.chain(&mut txn, &KeyArg::Values(vals), lock)
            }
        }
        .map_err(ferr)?;
        txn.commit().map_err(ferr)?;
        let fields = t.format.format.fields.len();
        Ok(r.map(|r| FileRecord {
            rrn: u32::try_from(r.rrn).unwrap(),
            data: r.data,
            nulls: vec![false; fields],
        }))
    }
    fn write(&mut self, h: u32, data: &[u8], _: &[bool]) -> Result<u32, FileError> {
        let e = &mut *self.0.borrow_mut();
        let (odp, t) = e.odps.get_mut(&h).ok_or_else(|| ferr("not open"))?;
        let nm = vec![0u8; usize::from(t.format.null_map_len)];
        let mut txn = e.store.begin().map_err(ferr)?;
        let rrn = odp.write(&mut txn, data, &nm).map_err(ferr)?;
        txn.commit().map_err(ferr)?;
        Ok(u32::try_from(rrn).unwrap())
    }
    fn update(&mut self, h: u32, data: &[u8], _: &[bool]) -> Result<(), FileError> {
        let e = &mut *self.0.borrow_mut();
        let (odp, t) = e.odps.get_mut(&h).ok_or_else(|| ferr("not open"))?;
        let nm = vec![0u8; usize::from(t.format.null_map_len)];
        let mut txn = e.store.begin().map_err(ferr)?;
        odp.update(&mut txn, data, &nm).map_err(ferr)?;
        txn.commit().map_err(ferr)
    }
    fn delete(&mut self, h: u32) -> Result<(), FileError> {
        let e = &mut *self.0.borrow_mut();
        let (odp, _) = e.odps.get_mut(&h).ok_or_else(|| ferr("not open"))?;
        let mut txn = e.store.begin().map_err(ferr)?;
        odp.delete(&mut txn).map_err(ferr)?;
        txn.commit().map_err(ferr)
    }
    fn feod(&mut self, _: u32) -> Result<(), FileError> {
        Ok(())
    }
    fn close(&mut self, h: u32) -> Result<(), FileError> {
        let e = &mut *self.0.borrow_mut();
        let (odp, _) = e.odps.remove(&h).ok_or_else(|| ferr("not open"))?;
        let mut txn = e.store.begin().map_err(ferr)?;
        odp.close(&mut txn);
        txn.commit().map_err(ferr)
    }
    fn command(&mut self, cmd: &str) -> Result<Vec<FileError>, FileError> {
        Err(ferr(format!("no commands here: {cmd}")))
    }
}

struct Passwords;

impl Authenticator for Passwords {
    fn check(&mut self, user: &str, proof: &Proof) -> Result<(), u8> {
        if user != USER {
            return Err(0x13);
        }
        let ok = match proof {
            Proof::Password(p) => p == PASSWORD,
            Proof::Substitute {
                substitute,
                client_seed,
                server_seed,
            } => {
                let level = match substitute.len() {
                    8 => hostsvr::crypt::Level::Des,
                    20 => hostsvr::crypt::Level::Sha1,
                    _ => hostsvr::crypt::Level::Sha512,
                };
                let seeds = hostsvr::crypt::Seeds {
                    client: *client_seed,
                    server: *server_seed,
                };
                hostsvr::crypt::substitute(level, USER, PASSWORD, &seeds)
                    .is_ok_and(|s| &s == substitute)
            }
        };
        if ok { Ok(()) } else { Err(0x0F) }
    }
}

/// A byte stream whose other end is a `Session`.
#[derive(Debug)]
struct Pipe {
    session: Session,
    inbox: Vec<u8>,
    outbox: VecDeque<u8>,
}

impl Write for Pipe {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.inbox.extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let chain =
            ddm::wire::decode_chain(&std::mem::take(&mut self.inbox)).expect("a whole chain");
        let replies = self.session.handle(&chain);
        self.outbox.extend(ddm::wire::encode_chain(&replies));
        Ok(())
    }
}

impl Read for Pipe {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = b.len().min(self.outbox.len());
        for x in b.iter_mut().take(n) {
            *x = self.outbox.pop_front().unwrap();
        }
        Ok(n)
    }
}

fn pipe(e: &Rc<RefCell<Engine>>) -> Pipe {
    let mut cfg = Config::new("QS400", "QS400");
    cfg.mechanisms.push(Mechanism::UsrSbsPwd);
    let session = Session::new(
        cfg,
        Box::new(Passwords),
        Box::new(SqlSide(e.clone())),
        Box::new(FileSide(e.clone())),
    );
    Pipe {
        session,
        inbox: Vec::new(),
        outbox: VecDeque::new(),
    }
}

fn connect(e: &Rc<RefCell<Engine>>, mech: Mechanism) -> Connection<Pipe> {
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![mech];
    Connection::connect(pipe(e), &o).unwrap_or_else(|err| panic!("{mech:?}: {err}"))
}

#[test]
fn every_mechanism_and_a_bad_password() {
    let e = Rc::new(RefCell::new(engine()));
    for m in [
        Mechanism::UsrSsbPwd,
        Mechanism::UsrEncPwd,
        Mechanism::EUsrIdPwd,
        Mechanism::UsrSbsPwd,
    ] {
        let c = connect(&e, m);
        assert_eq!(c.mechanism, Some(m));
        assert_eq!(c.sqlam, 7);
    }
    let mut o = ConnectOpts::new("QS400", USER, "wrong");
    o.mechanisms = vec![Mechanism::EUsrIdPwd];
    let err = Connection::connect(pipe(&e), &o).unwrap_err();
    assert!(err.to_string().contains("SECCHKCD 0x0f"), "{err}");
    // USRIDPWD is not offered: the requester finds no common mechanism.
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![Mechanism::UsrIdPwd];
    assert!(Connection::connect(pipe(&e), &o).is_err());
}

#[test]
fn sql_and_record_access() {
    let e = Rc::new(RefCell::new(engine()));
    let mut c = connect(&e, Mechanism::EUsrIdPwd);
    let ca = c
        .execute_immediate("CREATE TABLE QSTEST.CUST (ID INTEGER NOT NULL, NAME CHAR(10) NOT NULL, BAL DECIMAL(7, 2) NOT NULL)")
        .unwrap();
    assert!(!ca.is_error(), "{ca:?}");
    let ca = c
        .execute_immediate(
            "INSERT INTO QSTEST.CUST VALUES (1, 'Ann', 10.50), (2, 'Bob', 21.00), (3, 'Cy', 0)",
        )
        .unwrap();
    assert_eq!(ca.sqlerrd[2], 3);
    assert!(
        !c.execute_immediate("CREATE INDEX QSTEST.CUSTX ON QSTEST.CUST (ID)")
            .unwrap()
            .is_error()
    );

    let p = c
        .prepare("SELECT ID, NAME, BAL FROM QSTEST.CUST ORDER BY ID")
        .unwrap();
    assert_eq!(p.columns.len(), 3);
    assert_eq!(p.columns[2].ty.kind, Kind::Decimal);
    let rows = c.query(&p).unwrap();
    let text: Vec<String> = rows
        .rows
        .iter()
        .map(|r| r.iter().map(Value::render).collect::<Vec<_>>().join("|"))
        .collect();
    assert_eq!(
        text,
        [
            "1|Ann       |10.50",
            "2|Bob       |21.00",
            "3|Cy        |0.00"
        ]
    );
    assert_eq!(rows.end.as_ref().unwrap().sqlcode, 100);
    assert!(rows.closed);
    assert_eq!(c.commit().unwrap().sqlcode, 0);
    assert_eq!(c.rollback().unwrap().sqlcode, 0);

    // An SQL error comes back as the prepare's SQLCA.
    let bad = c.prepare("SELECT * FROM QSTEST.NOPE").unwrap();
    let ca = bad.sqlca.unwrap();
    assert_eq!(ca.sqlcode, -204, "{ca:?}");
    assert_eq!(ca.sqlstate, "42704");

    // Record-level access: a keyed read through the index, a write
    // through the table, and the table read in arrival order.
    let u = Ufcb {
        file: "CUSTX".into(),
        library: "QSTEST".into(),
        member: "*FIRST".into(),
        flags: ufcb::INPUT,
        arrival: false,
        share: Ufcb::share_for(ufcb::INPUT),
        blocking: 1,
        format: "CUST".into(),
    };
    let fx = c.open_file("F1", &u, 3).unwrap();
    assert_eq!(fx.feedback.reclen, 4 + 10 + 4);
    let key = 2i32.to_be_bytes();
    let r = c.get_key(&fx, &key, 1, false).unwrap();
    assert_eq!(r.records.len(), 1, "{r:?}");
    assert_eq!(ddm::wire::text(&r.records[0].data[4..14]), "Bob");
    let miss = c.get_key(&fx, &9i32.to_be_bytes(), 1, false).unwrap();
    assert!(miss.records.is_empty() && miss.at_end());
    c.close_file(&fx).unwrap();

    let u = Ufcb {
        file: "CUST".into(),
        flags: ufcb::INPUT | ufcb::OUTPUT,
        arrival: true,
        share: Ufcb::share_for(ufcb::INPUT | ufcb::OUTPUT),
        blocking: 10,
        ..u
    };
    let f = c.open_file("F2", &u, 3).unwrap();
    let mut rec = 4i32.to_be_bytes().to_vec();
    rec.extend(ddm::wire::ebcdic_fixed("Dee", 10));
    rec.extend([0x00, 0x01, 0x23, 0x4F]);
    let w = c.put(&f, &[Rec::new(rec, 3)]).unwrap();
    assert_eq!(w.iofb.unwrap().count, 1);
    let all = c
        .get(&f, ddm::cp::s38::S38GET, file::op::FIRST, 0, &[])
        .unwrap();
    assert_eq!(all.records.len(), 4, "{all:?}");
    assert_eq!(all.records[3].rrn, 4);
    let end = c
        .get(&f, ddm::cp::s38::S38GET, file::op::NEXT, 0, &[])
        .unwrap();
    assert!(end.at_end());
    c.close_file(&f).unwrap();

    let p = c
        .prepare("SELECT NAME, BAL FROM QSTEST.CUST WHERE ID = 4")
        .unwrap();
    let rows = c.query(&p).unwrap();
    assert_eq!(rows.rows[0][0], Value::Text("Dee       ".into()));
    assert_eq!(rows.rows[0][1].render(), "12.34");
}

/// The packages a bind made: collection, package, statements.
type Bound = Rc<RefCell<Vec<(String, String, Vec<(u16, String)>)>>>;

/// A procedure's result sets: `C1` of `ID INTEGER NOT NULL, NAME
/// VARCHAR(3) NOT NULL` with `first` rows, `C2` of `TEN INTEGER` with one.
struct CallSide {
    first: usize,
    open: HashMap<Section, VecDeque<Vec<Value>>>,
    /// `QSOUTP` was prepared: the call leaves no result sets.
    outp: bool,
    /// The packages bound: collection, package, statements.
    bound: Bound,
}

impl SqlBackend for CallSide {
    fn prepare(&mut self, _: &Section, text: &str) -> SqlResult<Vec<Column>> {
        self.outp = text.contains("QSOUTP");
        Ok(Vec::new())
    }
    fn describe(&mut self, _: &Section) -> SqlResult<Vec<Column>> {
        Ok(Vec::new())
    }
    fn execute(&mut self, _: &Section, _: &[Value]) -> SqlResult<Sqlca> {
        if self.outp {
            return Ok(Sqlca::default());
        }
        let mut ca = Sqlca::error(466, "0100C", "");
        ca.sqlerrd[1] = 2;
        Ok(ca)
    }
    fn execute_immediate(&mut self, _: &str) -> SqlResult<Sqlca> {
        Ok(Sqlca::default())
    }
    fn open(&mut self, _: &Section, _: &[Value]) -> SqlResult<Vec<Column>> {
        Err(Sqlca::error(-507, "24501", ""))
    }
    fn fetch(&mut self, s: &Section, max: usize) -> SqlResult<Vec<Vec<Value>>> {
        let q = self
            .open
            .get_mut(s)
            .ok_or_else(|| Sqlca::error(-501, "24501", ""))?;
        Ok((0..max).map_while(|_| q.pop_front()).collect())
    }
    fn close(&mut self, s: &Section) -> SqlResult<()> {
        self.open.remove(s);
        Ok(())
    }
    fn commit(&mut self) -> SqlResult<()> {
        Ok(())
    }
    fn rollback(&mut self) -> SqlResult<()> {
        Ok(())
    }
    fn result_sets(&mut self) -> (String, Vec<String>) {
        ("QSTEST.QSDRSP".into(), vec!["C1".into(), "C2".into()])
    }
    fn bind(
        &mut self,
        collection: &str,
        package: &str,
        _: [u8; 8],
        stmts: &[(u16, String)],
    ) -> SqlResult<()> {
        self.bound
            .borrow_mut()
            .push((collection.into(), package.into(), stmts.to_vec()));
        Ok(())
    }
    fn call_outputs(&mut self, _: &Section) -> SqlResult<Vec<(ColType, Value)>> {
        // QSOUTP(IN N INTEGER, OUT W VARCHAR(10), INOUT A DECIMAL(7, 2)).
        Ok(vec![
            (ColType::new(Kind::Integer, 0, 10, 0, false), Value::Null),
            (
                ColType::new(Kind::VarChar, 10, 0, 0, true),
                Value::Text("DONE".into()),
            ),
            (
                ColType::new(Kind::Decimal, 0, 7, 2, true),
                Value::Dec("36.75".parse().unwrap()),
            ),
        ])
    }
    fn open_result_set(&mut self, s: &Section, index: usize) -> SqlResult<Vec<Column>> {
        let col = |name: &str, ty: ColType| Column {
            name: name.into(),
            ty,
            label: None,
        };
        let (cols, rows): (Vec<Column>, Vec<Vec<Value>>) = if index == 0 {
            let names = ["ANN", "BOB", "CY"];
            (
                vec![
                    col("ID", ColType::new(Kind::Integer, 0, 10, 0, false)),
                    col("NAME", ColType::new(Kind::VarChar, 3, 0, 0, false)),
                ],
                (0..self.first)
                    .map(|i| {
                        vec![
                            Value::Int(i64::try_from(i + 1).unwrap()),
                            Value::Text(names[i % 3].into()),
                        ]
                    })
                    .collect(),
            )
        } else {
            (
                vec![col("TEN", ColType::new(Kind::Integer, 0, 10, 0, true))],
                vec![vec![Value::Int(20)]],
            )
        };
        self.open.insert(s.clone(), rows.into());
        Ok(cols)
    }
}

fn call_pipe(first: usize) -> Pipe {
    call_pipe_binding(first, Rc::default())
}

fn call_pipe_binding(first: usize, bound: Bound) -> Pipe {
    let e = Rc::new(RefCell::new(engine()));
    let session = Session::new(
        Config::new("QS400", "QS400"),
        Box::new(Passwords),
        Box::new(CallSide {
            first,
            open: HashMap::new(),
            outp: false,
            bound,
        }),
        Box::new(FileSide(e)),
    );
    Pipe {
        session,
        inbox: Vec::new(),
        outbox: VecDeque::new(),
    }
}

/// The reply to the `EXCSQLSTT` of `drda-call.trace` (pub400): each
/// object's code point and bytes.
fn oracle_call_reply() -> Vec<(u16, Vec<u8>)> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/ddm/traces/drda-call.trace");
    let t = ddm::trace::Trace::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut out = Vec::new();
    let mut corr = None;
    for ch in t.connections[0].chunks() {
        let (d, _) = ddm::Dss::decode(&ch.bytes).unwrap();
        if ch.dir == ddm::Dir::Request && d.code() == Some(ddm::cp::cmd::EXCSQLSTT) {
            corr = Some(d.corr);
        } else if ch.dir == ddm::Dir::Reply && Some(d.corr) == corr {
            let o = d.first().unwrap();
            out.push((o.cp, o.bytes().to_vec()));
        }
    }
    out
}

#[test]
fn a_calls_result_sets_come_as_ibm_i_sends_them() {
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![Mechanism::EUsrIdPwd];
    let mut c = Connection::connect(call_pipe(2), &o).unwrap();
    let p = c.prepare("CALL QSTEST.QSDRSP(2)").unwrap();
    let corr = c.next_corr();
    let cmd = ddm::wire::Object::coll(
        ddm::cp::cmd::EXCSQLSTT,
        vec![
            p.pkg.to_object(),
            ddm::wire::Object::u32(ddm::cp::param::QRYBLKSZ, 32_767),
            ddm::wire::Object::u16(ddm::cp::param::MAXRSLCNT, 0xFFFF),
        ],
    );
    let replies = c.exchange(vec![ddm::Dss::request(corr, cmd)]).unwrap();
    let ours: Vec<(u16, Vec<u8>)> = replies
        .iter()
        .filter_map(|d| d.first())
        .map(|o| (o.cp, o.bytes().to_vec()))
        .collect();
    let oracle = oracle_call_reply();
    assert_eq!(
        ours.iter().map(|x| x.0).collect::<Vec<_>>(),
        oracle.iter().map(|x| x.0).collect::<Vec<_>>()
    );
    // SQLRSLRD, each set's SQLCINRD, QRYDSC and rows, byte for byte.
    for (i, (cp, b)) in oracle.iter().enumerate() {
        if matches!(*cp, 0x240E | 0x240B | 0x241A | 0x241B) {
            assert_eq!(&ours[i].1, b, "object {i} ({cp:04X})");
        }
    }
    // The sections name the procedure's package.
    let rsl = replies[0].first().unwrap();
    let secs: Vec<ddm::msg::Pkgnamcsn> = rsl
        .all(ddm::cp::param::PKGSNLST)
        .flat_map(|l| l.all(ddm::cp::param::PKGNAMCSN))
        .map(|x| ddm::msg::Pkgnamcsn::decode(x.bytes()).unwrap())
        .collect();
    assert_eq!(
        secs.iter()
            .map(|s| (s.collection.as_str(), s.package.as_str(), s.section))
            .collect::<Vec<_>>(),
        [("QSTEST", "QSDRSP", 1), ("QSTEST", "QSDRSP", 2)]
    );
}

#[test]
fn a_large_result_set_continues_at_its_section() {
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![Mechanism::EUsrIdPwd];
    let mut c = Connection::connect(call_pipe(70), &o).unwrap();
    let p = c.prepare("CALL QSTEST.QSDRSP(70)").unwrap();
    let r = c.call(&p).unwrap();
    assert_eq!(r.sqlca.as_ref().map(|s| s.sqlcode), Some(466));
    assert_eq!(
        r.sets.iter().map(|s| s.rows.len()).collect::<Vec<_>>(),
        [70, 1]
    );
    assert_eq!(r.sets[0].rows[69][1].render(), "ANN");
    assert_eq!(r.sets[1].rows[0][0].render(), "20");
}

/// The `SQLDTARD` of `tools/conformance/ddm/outexp-primary.txt`.
fn oracle_outexp() -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/ddm/outexp-primary.txt");
    let text = std::fs::read_to_string(path).unwrap();
    let mut b = Vec::new();
    for line in text.lines().filter(|l| l.starts_with("  ")) {
        for h in line.split_whitespace().skip(1) {
            b.push(u8::from_str_radix(h, 16).unwrap());
        }
    }
    b
}

#[test]
fn a_calls_output_values_come_as_the_primary_sends_them() {
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![Mechanism::EUsrIdPwd];
    let mut c = Connection::connect(call_pipe(0), &o).unwrap();
    let p = c.prepare("CALL QSTEST.QSOUTP(?, ?, ?)").unwrap();
    let corr = c.next_corr();
    let cmd = ddm::wire::Object::coll(
        ddm::cp::cmd::EXCSQLSTT,
        vec![
            p.pkg.to_object(),
            ddm::wire::Object::byte(ddm::cp::param::OUTEXP, 0xF1),
        ],
    );
    let replies = c.exchange(vec![ddm::Dss::request(corr, cmd)]).unwrap();
    let objs: Vec<&ddm::wire::Object> = replies.iter().filter_map(|d| d.first()).collect();
    assert_eq!(objs.len(), 1, "{objs:?}");
    assert_eq!(objs[0].cp, ddm::cp::obj::SQLDTARD);
    let mut ours = Vec::new();
    for x in objs[0].items() {
        ours.extend(x.encode());
    }
    assert_eq!(ours, oracle_outexp());
}

#[test]
fn a_bind_makes_the_package() {
    let bound = Rc::default();
    let mut o = ConnectOpts::new("QS400", USER, PASSWORD);
    o.mechanisms = vec![Mechanism::EUsrIdPwd];
    o.collection = "QSTEST".into();
    o.package = "QSDDMPKG".into();
    let mut c = Connection::connect(call_pipe_binding(0, Rc::clone(&bound)), &o).unwrap();
    assert_eq!(c.bind_dynamic(2).unwrap().sqlcode, 0);
    let b = bound.borrow();
    assert_eq!(b.len(), 1);
    assert_eq!((b[0].0.as_str(), b[0].1.as_str()), ("QSTEST", "QSDDMPKG"));
    assert_eq!(
        b[0].2,
        [
            (1, "DECLARE C1 CURSOR FOR S1".to_string()),
            (2, "DECLARE C2 CURSOR FOR S2".to_string())
        ]
    );
}
