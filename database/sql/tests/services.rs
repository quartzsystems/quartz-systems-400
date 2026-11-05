// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` services over a host in memory: pub400's columns, the
//! host's values, null where QS/400 has none. Value forms follow
//! tools/conformance/sql/expected/services/SVCVALS.out.

#![allow(clippy::too_many_lines)]

use std::rc::Rc;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use sql::exec::catalog::services::host::{ActiveJob, SysValue, SystemHost, SystemStatus};
use sql::exec::catalog::services::{self, host, procs};
use sql::{Naming, Outcome, SqlContext, Value, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, types};
use store::{FormatParams, Store, StoreConfig};

/// A host with a few of everything.
#[derive(Default)]
struct Mock;

fn job(number: u32, name: &str, user: &str, kind: char, sbs: &str) -> wm::table::JobInfo {
    wm::table::JobInfo {
        name: name.into(),
        user: user.into(),
        number,
        kind,
        subtype: ' ',
        subsystem: sbs.into(),
        pid: 100 + number,
        status: wm::job::Status::from_code("RUN").map_or(0, wm::job::Status::byte),
        function: "PGM-QCMD".into(),
        started: 1_767_225_600,
        handle: u64::from(number),
        jobd: "QGPL/QDFTJOBD".into(),
        class: "QGPL/QINTER".into(),
        pool: "*INTERACT".into(),
    }
}

impl SystemHost for Mock {
    fn library_list(&self) -> Option<wm::job::LibList> {
        Some(wm::job::LibList {
            sys: vec!["QSYS".into(), "QSYS2".into()],
            prd: Vec::new(),
            cur: Some("QSTEST".into()),
            usr: vec!["QGPL".into(), "QTEMP".into()],
        })
    }

    fn system_values(&self) -> Option<Vec<SysValue>> {
        Some(vec![
            SysValue {
                name: "QMAXJOB".into(),
                number: Some(163_520),
                text: None,
                description: "Most jobs".into(),
                group: "*SYSCTL".into(),
                shipped: "163520".into(),
                changeable: true,
            },
            SysValue {
                name: "QDATFMT".into(),
                number: None,
                text: Some("MDY".into()),
                description: "Date format".into(),
                group: "*DATTIM".into(),
                shipped: "MDY".into(),
                changeable: true,
            },
        ])
    }

    fn active_jobs(&self) -> Option<Vec<ActiveJob>> {
        let a = |j: wm::table::JobInfo| ActiveJob {
            attrs: Some(wm::job::JobAttrs {
                jobq: "QGPL/QBATCH".into(),
                outq: "QGPL/QPRINT".into(),
                jobd: "QGPL/QDFTJOBD".into(),
                ccsid: 37,
                dftccsid: 37,
                langid: "ENU".into(),
                datfmt: "*MDY".into(),
                datsep: "/".into(),
                timsep: ":".into(),
                maxthd: u32::MAX,
                cputime: u32::MAX,
                maxtmpstg: u32::MAX,
                runpty: 20,
                ..wm::job::JobAttrs::default()
            }),
            cpu_ms: 1500,
            cpu_tenths: 12,
            threads: 1,
            temp_mb: 3,
            aux_io: 40,
            elapsed_aux_io: 4,
            pool_number: 4,
            transactions: 2,
            response_ms: 400,
            elapsed_ms: 5000,
            job: j,
        };
        Some(vec![
            a(job(1, "QSYSARB", "QSYS", 'S', "")),
            a(job(5, "QINTER", "QSYS", 'M', "QINTER")),
            a(job(9, "QPADEV0001", "CODY", 'I', "QINTER")),
        ])
    }

    fn http(&self, req: &host::HttpRequest) -> Option<Result<host::HttpResponse, String>> {
        if req.url.contains("missing") {
            return Some(Ok(host::HttpResponse { status: 404, headers: Vec::new(), body: b"gone".to_vec() }));
        }
        let auth = req.headers.iter().find(|(n, _)| n == "Authorization").map_or(String::new(), |(_, v)| v.clone());
        let body = format!("{} {} [{}] {}", req.method, req.url, auth, String::from_utf8_lossy(&req.body));
        Some(Ok(host::HttpResponse { status: 200, headers: vec![("Content-Type".into(), "text/plain".into())], body: body.into_bytes() }))
    }

    fn spooled_files(&self) -> Option<Vec<wm::obj::splf::SplfAttrs>> {
        Some(vec![wm::obj::splf::SplfAttrs {
            id: 7,
            number: 1,
            name: "QSYSPRT".into(),
            job: ("QPADEV0001".into(), "CODY".into(), 9),
            pages: 1,
            copies: 1,
            outpty: 5,
            page_len: 66,
            page_width: 132,
            lpi: 6,
            cpi: 10,
            data_len: 2000,
            created: 1_767_225_600_000_123,
            outq: "QGPL/QPRINT".into(),
            prtf: "QSYS/QSYSPRT".into(),
            ..wm::obj::splf::SplfAttrs::default()
        }])
    }

    fn spooled_data(&self, id: u32) -> Option<Result<Vec<u8>, String>> {
        assert_eq!(id, 7);
        let mut b = ccsid::to_ebcdic("LINE ONE").unwrap();
        b.push(0x15);
        b.extend(ccsid::to_ebcdic("LINE TWO").unwrap());
        b.push(0x15);
        Some(Ok(b))
    }

    fn job_log(&self, job: &str) -> Option<host::JobLog> {
        if job != "*" && job != "000009/CODY/QPADEV0001" {
            return Some(host::JobLog::NoSuchJob);
        }
        let e = |id: &str, kind: &str, t: u64, text: &str| wm::joblog::Entry {
            msgid: id.into(),
            kind: kind.into(),
            time: t,
            from: ("QWTPIIPP".into(), "QSYS".into()),
            to: ("*EXT".into(), String::new()),
            text: text.into(),
            ..wm::joblog::Entry::default()
        };
        Some(host::JobLog::Found(
            "000009/CODY/QPADEV0001".into(),
            vec![e("CPF1124", "Information", 1_767_225_600_000_000, "Job started."), e("", "Request", 1_767_225_660_000_000, "WRKACTJOB")],
        ))
    }

    fn netstat(&self) -> Option<Result<host::Netstat, String>> {
        let lo = "127.0.0.1".parse().unwrap();
        Some(Ok(host::Netstat {
            interfaces: vec![nettools::netstat::Ifc {
                addr: Some(lo),
                prefix: 8,
                line: "*LOOPBACK".into(),
                line_type: "*NONE".into(),
                status: "Active".into(),
                mtu: 576,
                autostart: true,
                ..nettools::netstat::Ifc::default()
            }],
            routes: vec![nettools::netstat::Rte { dest: Some("127.0.0.0".parse().unwrap()), prefix: 8, mtu: 576, available: true, configured: true, ifc: Some(lo), ..nettools::netstat::Rte::default() }],
            connections: vec![nettools::netstat::Conn {
                tcp: true,
                local: "0.0.0.0:21".parse().unwrap(),
                remote: "0.0.0.0:0".parse().unwrap(),
                state: "Listen".into(),
                idle_ms: 311_440,
                user: "QTCP".into(),
                jobs: vec![("QTFTP00001".into(), "QTCP".into(), "000042".into(), "BCH".into(), "QTCP".into())],
                counters: nettools::netstat::Counters::default(),
                passive: true,
            }],
            port_names: vec![(21, true, "ftp-control".into())],
        }))
    }

    fn products(&self) -> Option<Result<Vec<host::Product>, String>> {
        Some(Ok(vec![host::Product {
            id: "QS00SS1".into(),
            option: "*BASE".into(),
            load_id: "5050".into(),
            code: true,
            release: "V1R1M0".into(),
            library: "QSYS".into(),
            text: "QS/400".into(),
        }]))
    }

    fn ptfs(&self) -> Option<Result<Vec<host::Ptf>, String>> {
        Some(Ok(vec![host::Ptf {
            id: "QS00001".into(),
            product: "QS00SS1".into(),
            option: "*BASE".into(),
            release: "V1R1M0".into(),
            status: "APPLIED".into(),
            ipl_action: "NONE".into(),
            changed: 1_767_225_600,
            ..host::Ptf::default()
        }]))
    }

    fn ifs_objects(&self, path: &str, subtree: bool) -> Option<Result<host::IfsWalk, String>> {
        if path != "/home" {
            return Some(Err("ENOENT".into()));
        }
        let o = |p: &str, kind: &str, size: u64| host::IfsObject { path: p.into(), kind: kind.into(), owner: "CODY".into(), size, mode: 0o755, links: 1, ..host::IfsObject::default() };
        let mut v = vec![("/home".to_string(), Ok(o("/home", "*DIR", 0)))];
        if subtree {
            v.push(("/home/a.txt".to_string(), Ok(o("/home/a.txt", "*STMF", 12))));
            v.push(("/home/secret".to_string(), Err("EACCES".into())));
        }
        Some(Ok(v))
    }

    fn ifs_read(&self, path: &str) -> Option<Result<(Vec<u8>, u16), String>> {
        match path {
            "/home/a.txt" => Some(Ok((b"one\r\ntwo\nthree".to_vec(), 1208))),
            "/home/e.txt" => Some(Ok((vec![0xC1, 0x15, 0xC2], 37))),
            _ => Some(Err("ENOENT".into())),
        }
    }

    fn command(&self, cmd: &str) -> Option<Result<(), (String, String)>> {
        Some(if cmd.starts_with("CRTLIB") { Ok(()) } else { Err(("CPF0006".into(), "Errors occurred in command.".into())) })
    }

    fn ifs_write(&self, path: &str, data: &[u8], append: bool, ccsid: u16) -> Option<Result<(), String>> {
        assert_eq!((path, data, append, ccsid), ("/home/w.txt", &b"hi\r\n"[..], true, 1208));
        Some(Ok(()))
    }

    fn system_status(&self) -> Option<SystemStatus> {
        Some(SystemStatus {
            total_jobs: 40,
            max_jobs: 163_520,
            active_jobs: 38,
            signed_on: 1,
            cpus: 4,
            cpu_pct: 3.25,
            main_storage_kb: 8_000_000,
            asp_mb: 20_000,
            asp_used_pct: 1.5,
            host_name: "qs400".into(),
            machine: (String::new(), "QS1".into(), "1234567".into()),
            ipl_mode: "NORMAL".into(),
            ipl_type: "0".into(),
            ..SystemStatus::default()
        })
    }
}

struct Db {
    store: Store,
    ctx: SqlContext,
}

impl Db {
    fn new(host: bool) -> Self {
        let d = Arc::new(MemBlockDev::with_seed(32768, 17));
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 256,
            now: 1_767_225_600,
        };
        let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
        let (qsys, lib) = {
            let mut t = store.begin().unwrap();
            let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
            let mut lib = None;
            for (name, text) in [("QSTEST", "Test library"), ("QGPL", "General purpose"), ("QSYS2", "System library for SQL")] {
                let id = ops::create(
                    &mut t,
                    &CreateParams {
                        library: qsys,
                        name: Name::parse(name).unwrap(),
                        type_code: types::LIB,
                        attribute: String::new(),
                        text: text.into(),
                        owner: "QSECOFR".into(),
                        system: "QS400".into(),
                        now: 1_767_225_600,
                        by_system: false,
                    },
                )
                .unwrap();
                lib = lib.or(Some(id));
            }
            t.commit().unwrap();
            (qsys, lib.unwrap())
        };
        let mut ctx = SqlContext::new(qsys, JobHandle(9), NameContext { curlib: Some(lib), libl: vec![lib] });
        ctx.now = Some(1_767_225_600);
        ctx.user = "CODY".into();
        if host {
            ctx.host = Some(Rc::new(Mock));
        }
        Self { store, ctx }
    }

    fn run(&mut self, text: &str) -> (sql::Sqlca, Option<Outcome>) {
        let p = match prepare(text, Naming::Sql) {
            Ok(p) => p,
            Err(e) => return (sql::Sqlca::from_error(&e), None),
        };
        let mut t = self.store.begin().unwrap();
        let r = p.run(&mut t, &mut self.ctx, &[]);
        if r.0.is_error() {
            t.abort();
        } else {
            t.commit().unwrap();
        }
        r
    }

    fn rows(&mut self, text: &str) -> Vec<Vec<String>> {
        let (ca, o) = self.run(text);
        assert!(!ca.is_error(), "{text}: {}", ca.line());
        match o.unwrap() {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| r.iter().map(|v| if v.is_null() { "-".into() } else { v.to_string().trim_end().to_string() }).collect())
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, text: &str) -> i32 {
        self.run(text).0.sqlcode
    }

    /// A table function's rows, as `TABLE(f(args))` gets them.
    fn function(&mut self, name: &str, args: Vec<(Option<String>, Value)>) -> sql::Result<(Vec<String>, Vec<Vec<Value>>)> {
        let mut t = self.store.begin().unwrap();
        let n = sql::ast::ObjectName { schema: Some(sql::ast::Ident::new("QSYS2")), name: sql::ast::Ident::new(name) };
        let r = services::table_function(&mut t, &self.ctx, &n, args, "X")?.expect("a service table function");
        t.abort();
        Ok((r.0.cols.iter().map(|c| c.name.clone()).collect(), r.1))
    }
}

fn row(cells: &[&str]) -> Vec<String> {
    cells.iter().map(ToString::to_string).collect()
}

#[test]
fn every_service_fills_its_not_null_columns() {
    let mut db = Db::new(true);
    let mut t = db.store.begin().unwrap();
    let faults = services::self_check(&mut t, &db.ctx);
    t.abort();
    assert!(faults.is_empty(), "{faults:#?}");
}

#[test]
fn library_list_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT ORDINAL_POSITION, SCHEMA_NAME, SYSTEM_SCHEMA_NAME, TYPE, TEXT_DESCRIPTION FROM QSYS2.LIBRARY_LIST_INFO"),
        vec![
            row(&["1", "QSYS", "QSYS", "SYSTEM", "System library"]),
            row(&["2", "QSYS2", "QSYS2", "SYSTEM", "System library for SQL"]),
            row(&["3", "QSTEST", "QSTEST", "CURRENT", "Test library"]),
            row(&["4", "QGPL", "QGPL", "USER", "General purpose"]),
            row(&["5", "QTEMP", "QTEMP", "USER", "-"]),
        ]
    );
}

#[test]
fn system_value_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT SYSTEM_VALUE_NAME, SYSTEM_VALUE, CURRENT_NUMERIC_VALUE, CURRENT_CHARACTER_VALUE, CATEGORY, CHANGEABLE FROM QSYS2.SYSTEM_VALUE_INFO"),
        vec![row(&["QDATFMT", "MDY", "-", "MDY", "*DATTIM", "YES"]), row(&["QMAXJOB", "163520", "163520", "-", "*SYSCTL", "YES"])]
    );
}

#[test]
fn system_status_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT TOTAL_JOBS_IN_SYSTEM, ACTIVE_JOBS_IN_SYSTEM, CONFIGURED_CPUS, ELAPSED_CPU_USED, HOST_NAME, RESTRICTED_STATE, PARTITION_ID FROM QSYS2.SYSTEM_STATUS_INFO"),
        vec![row(&["40", "38", "4", "3.25", "qs400", "NO", "-"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM QSYS2.SYSTEM_STATUS_INFO_BASIC"), vec![row(&["1"])]);
}

#[test]
fn without_a_host_the_system_state_is_sql0443() {
    let mut db = Db::new(false);
    assert_eq!(db.code("SELECT * FROM QSYS2.SYSTEM_STATUS_INFO"), -443);
    // The library list is the statement's own then, its parts unknown.
    assert_eq!(db.rows("SELECT SYSTEM_SCHEMA_NAME, TYPE FROM QSYS2.LIBRARY_LIST_INFO"), vec![row(&["QSTEST", "-"])]);
}

#[test]
fn active_job_info() {
    let mut db = Db::new(true);
    let (cols, rows) = db.function("ACTIVE_JOB_INFO", Vec::new()).unwrap();
    assert_eq!(cols.len(), 125);
    assert_eq!(rows.len(), 3);
    let at = |r: &[Value], c: &str| r[cols.iter().position(|x| x == c).unwrap()].to_string();
    assert_eq!(at(&rows[2], "JOB_NAME"), "000009/CODY/QPADEV0001");
    assert_eq!(at(&rows[2], "JOB_TYPE"), "INT");
    assert_eq!(at(&rows[2], "FUNCTION_TYPE"), "PGM");
    assert_eq!(at(&rows[2], "FUNCTION"), "QCMD");
    assert_eq!(at(&rows[2], "SUBSYSTEM"), "QINTER");
    assert_eq!(at(&rows[1], "JOB_TYPE"), "SBS");
    let (_, rows) = db.function("ACTIVE_JOB_INFO", vec![(Some("JOB_NAME_FILTER".into()), Value::Str("*".into()))]).unwrap();
    assert_eq!(rows.len(), 1);
    let (_, rows) = db.function("ACTIVE_JOB_INFO", vec![(Some("SUBSYSTEM_LIST_FILTER".into()), Value::Str("QINTER".into()))]).unwrap();
    assert_eq!(rows.len(), 2);
    let e = db.function("ACTIVE_JOB_INFO", vec![(Some("NO_SUCH".into()), Value::Null)]).unwrap_err();
    assert!(e.to_string().contains("NO_SUCH"), "{e}");
}

#[test]
fn job_info() {
    let mut db = Db::new(true);
    let (cols, rows) = db.function("JOB_INFO", Vec::new()).unwrap();
    assert_eq!(rows.len(), 1, "the user's own jobs by default");
    let at = |r: &[Value], c: &str| r[cols.iter().position(|x| x == c).unwrap()].to_string();
    assert_eq!(at(&rows[0], "JOB_STATUS"), "ACTIVE");
    let (_, rows) = db.function("JOB_INFO", vec![(Some("JOB_USER_FILTER".into()), Value::Str("*ALL".into()))]).unwrap();
    assert_eq!(rows.len(), 3);
    let (_, rows) = db.function("JOB_INFO", vec![(None, Value::Str("*JOBQ".into()))]).unwrap();
    assert!(rows.is_empty());
}

#[test]
fn shapes_load() {
    let s = services::shape::view("QSYS2", "SYSTEM_VALUE_INFO").unwrap();
    assert_eq!(s.cols.len(), 8);
    assert!(services::shape::routine("QSYS2", "ACTIVE_JOB_INFO", 'T').is_some());
    let _ = host::HttpRequest::default();
}

#[test]
fn table_functions_by_sql_with_named_arguments() {
    let mut db = Db::new(true);
    assert_eq!(db.rows("SELECT JOB_NAME FROM TABLE(QSYS2.ACTIVE_JOB_INFO(JOB_NAME_FILTER => '*')) X"), vec![row(&["000009/CODY/QPADEV0001"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.ACTIVE_JOB_INFO()) X WHERE JOB_TYPE = 'SBS'"), vec![row(&["1"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(ACTIVE_JOB_INFO('NO', 'QINTER')) X"), vec![row(&["2"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.ACTIVE_JOB_INFO(BOGUS => 1)) X"), -440);
}

#[test]
fn base64_and_url_as_pub400() {
    let mut db = Db::new(true);
    assert_eq!(db.rows("SELECT QSYS2.BASE64_ENCODE(X'00FF10'), BASE64_ENCODE('x') FROM SYSIBM.SYSDUMMY1"), vec![row(&["AP8Q", "pw=="])]);
    assert_eq!(db.rows("SELECT HEX(QSYS2.BASE64_DECODE('SGVsbG8sIFFTLzQwMA==')) FROM SYSIBM.SYSDUMMY1"), vec![row(&["48656C6C6F2C2051532F343030"])]);
    assert_eq!(db.code("SELECT QSYS2.BASE64_DECODE('not base64!') FROM SYSIBM.SYSDUMMY1"), -443);
    assert_eq!(
        db.rows("SELECT QSYS2.URL_ENCODE('a b&c=d/e?f~g.h-i_j*k+l%') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["a+b%26c%3Dd%2Fe%3Ff%7Eg.h-i_j%2Ak%2Bl%25"])]
    );
    assert_eq!(db.rows("SELECT QSYS2.URL_DECODE('caf%C3%A9'), QSYS2.URL_DECODE('a+b%26c') FROM SYSIBM.SYSDUMMY1"), vec![row(&["café", "a b&c"])]);
    assert_eq!(db.code("SELECT QSYS2.URL_DECODE('bad%zz') FROM SYSIBM.SYSDUMMY1"), -443);
    assert_eq!(db.code("SELECT QSYS2.URL_ENCODE('a b', '%20') FROM SYSIBM.SYSDUMMY1"), -171);
}

#[test]
fn http_functions_through_the_host() {
    let mut db = Db::new(true);
    assert_eq!(db.rows("SELECT QSYS2.HTTP_GET('http://h/x') FROM SYSIBM.SYSDUMMY1"), vec![row(&["GET http://h/x []"])]);
    assert_eq!(
        db.rows("SELECT QSYS2.HTTP_POST('http://h/x', 'hi', '{\"basicAuth\":\"u,p\"}') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["POST http://h/x [Basic dTpw] hi"])]
    );
    assert_eq!(db.code("SELECT QSYS2.HTTP_GET('http://h/missing') FROM SYSIBM.SYSDUMMY1"), -443);
    assert_eq!(db.code("SELECT QSYS2.HTTP_GET('http://h/x', '{\"proxy\":\"p:1\"}') FROM SYSIBM.SYSDUMMY1"), -443);
    assert_eq!(
        db.rows("SELECT RESPONSE_HTTP_HEADER FROM TABLE(QSYS2.HTTP_GET_VERBOSE('http://h/missing')) X"),
        vec![row(&["{\"HTTP_STATUS_CODE\":404}"])]
    );
}

#[test]
fn object_statistics() {
    let mut db = Db::new(true);
    let (ca, _) = db.run("CREATE TABLE QSTEST.ORDERS (ID INT)");
    assert!(!ca.is_error(), "{}", ca.line());
    let (ca, _) = db.run("CREATE VIEW QSTEST.ORDERV AS SELECT ID FROM QSTEST.ORDERS");
    assert!(!ca.is_error(), "{}", ca.line());
    assert_eq!(
        db.rows("SELECT OBJNAME, OBJTYPE, OBJLIB, SQL_OBJECT_TYPE, IASP_NAME FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*FILE')) X ORDER BY OBJNAME"),
        vec![row(&["ORDERS", "*FILE", "QSTEST", "TABLE", "*SYSBAS"]), row(&["ORDERV", "*FILE", "QSTEST", "VIEW", "*SYSBAS"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*ALL', 'ORDERS')) X"), vec![row(&["1"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*FILE', OBJECT_NAME => 'ORD*')) X"), vec![row(&["2"])]);
    assert_eq!(db.rows("SELECT OBJNAME FROM TABLE(QSYS2.OBJECT_STATISTICS('QSYS', '*LIB', 'QGPL')) X"), vec![row(&["QGPL"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.OBJECT_STATISTICS('*LIBL', '*LIB')) X"), -443);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*NOTATYPE')) X"), -443);
}

#[test]
fn user_info_and_groups() {
    let mut db = Db::new(true);
    {
        let mut t = db.store.begin().unwrap();
        let mut g = sec::profile::UsrPrf::new("DEVS", 40);
        g.text = "Developers".into();
        g.gid = 500;
        sec::profile::objects::create(&mut t, db.ctx.qsys, &g, "QSECOFR", store::obj::header::Authority::EXCLUDE, 1_767_225_600).unwrap();
        let mut u = sec::profile::UsrPrf::new("CODY", 40);
        u.text = "Cody".into();
        u.grpprf = "DEVS".into();
        sec::profile::objects::create(&mut t, db.ctx.qsys, &u, "QSECOFR", store::obj::header::Authority::EXCLUDE, 1_767_225_600).unwrap();
        t.commit().unwrap();
    }
    assert_eq!(
        db.rows("SELECT AUTHORIZATION_NAME, STATUS, USER_CLASS_NAME, GROUP_PROFILE_NAME, TEXT_DESCRIPTION FROM QSYS2.USER_INFO ORDER BY 1"),
        vec![row(&["CODY", "*ENABLED", "*USER", "DEVS", "Cody"]), row(&["DEVS", "*ENABLED", "*USER", "*NONE", "Developers"])]
    );
    assert_eq!(db.rows("SELECT GROUP_MEMBER_INDICATOR FROM QSYS2.USER_INFO_BASIC WHERE AUTHORIZATION_NAME = 'DEVS'"), vec![row(&["YES"])]);
    assert_eq!(db.rows("SELECT * FROM QSYS2.GROUP_PROFILE_ENTRIES"), vec![row(&["DEVS", "CODY", "Cody"])]);
}

#[test]
fn output_queue_entries_and_spooled_file_data() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT OUTPUT_QUEUE_NAME, OUTPUT_QUEUE_LIBRARY_NAME, SPOOLED_FILE_NAME, STATUS, JOB_NAME, SIZE, DEVICE_FILE_NAME FROM QSYS2.OUTPUT_QUEUE_ENTRIES"),
        vec![row(&["QPRINT", "QGPL", "QSYSPRT", "READY", "000009/CODY/QPADEV0001", "2", "QSYSPRT"])]
    );
    assert_eq!(db.rows("SELECT TOTAL_PAGES FROM QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC"), vec![row(&["1"])]);
    assert_eq!(
        db.rows("SELECT ORDINAL_POSITION, SPOOLED_DATA FROM TABLE(SYSTOOLS.SPOOLED_FILE_DATA(JOB_NAME => '000009/CODY/QPADEV0001', SPOOLED_FILE_NAME => 'QSYSPRT')) X"),
        vec![row(&["1", "LINE ONE"]), row(&["2", "LINE TWO"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(SYSTOOLS.SPOOLED_FILE_DATA('000001/X/Y')) X"), vec![row(&["0"])]);
}

#[test]
fn joblog_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT ORDINAL_POSITION, MESSAGE_ID, MESSAGE_TYPE, MESSAGE_TEXT, MESSAGE_FILE FROM TABLE(QSYS2.JOBLOG_INFO('*')) X"),
        vec![row(&["1", "CPF1124", "INFORMATIONAL", "Job started.", "QCPFMSG"]), row(&["2", "-", "REQUEST", "WRKACTJOB", "-"])]
    );
    assert_eq!(db.rows("SELECT MESSAGE_TEXT FROM TABLE(QSYS2.JOBLOG_INFO('*', MESSAGE_ORDER => 'DESCENDING', MESSAGE_LIMIT => 1)) X"), vec![row(&["WRKACTJOB"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.JOBLOG_INFO('000001/X/Y')) X"), -443);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.JOBLOG_INFO('000001/X/Y', IGNORE_ERRORS => 'YES')) X"), vec![row(&["0"])]);
}

#[test]
fn netstat_views() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT CONNECTION_TYPE, LOCAL_PORT, LOCAL_PORT_NAME, PROTOCOL, TCP_STATE, IDLE_TIME, BIND_USER, CONNECTION_OPEN_TYPE FROM QSYS2.NETSTAT_INFO"),
        vec![row(&["IPV4", "21", "ftp-control", "TCP", "LISTEN", "311.440", "QTCP", "PASSIVE"])]
    );
    assert_eq!(db.rows("SELECT JOB_NAME, JOB_TYPE FROM QSYS2.NETSTAT_JOB_INFO"), vec![row(&["000042/QTCP/QTFTP00001", "BATCH"])]);
    assert_eq!(
        db.rows("SELECT INTERNET_ADDRESS, NETWORK_ADDRESS, SUBNET_MASK, LINE_DESCRIPTION, INTERFACE_LINE_TYPE, INTERFACE_STATUS FROM QSYS2.NETSTAT_INTERFACE_INFO"),
        vec![row(&["127.0.0.1", "127.0.0.0", "255.0.0.0", "*LOOPBACK", "NONE", "ACTIVE"])]
    );
    assert_eq!(
        db.rows("SELECT ROUTE_DESTINATION, SUBNET_MASK, NEXT_HOP, ROUTE_STATUS, ROUTE_TYPE, ROUTE_SOURCE, LOCAL_BINDING_LINE_DESCRIPTION FROM QSYS2.NETSTAT_ROUTE_INFO"),
        vec![row(&["127.0.0.0", "255.0.0.0", "*DIRECT", "YES", "DIRECT", "CFG", "*LOOPBACK"])]
    );
}

#[test]
fn products_and_ptfs() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT PRODUCT_ID, PRODUCT_OPTION, LOAD_TYPE, RELEASE_LEVEL, SYMBOLIC_LOAD_STATE, PRODUCT_LIBRARIES FROM QSYS2.SOFTWARE_PRODUCT_INFO"),
        vec![row(&["QS00SS1", "*BASE", "CODE", "V1R1M0", "INSTALLED", "QSYS"])]
    );
    assert_eq!(
        db.rows("SELECT PTF_IDENTIFIER, PTF_PRODUCT_ID, PTF_LOADED_STATUS, PTF_IPL_ACTION, PTF_ACTION_PENDING FROM QSYS2.PTF_INFO"),
        vec![row(&["QS00001", "QS00SS1", "APPLIED", "NONE", "NO"])]
    );
}

#[test]
fn history_log_info() {
    let mut db = Db::new(true);
    let (ca, _) = db.run(
        "CREATE TABLE QSYS.QHST260101 (QHDATE CHAR(7), QHTIME CHAR(6), QHJOB CHAR(10), QHUSER CHAR(10), QHNBR CHAR(6),          QHMSGID CHAR(7), QHSEV NUMERIC(2, 0), QHTYPE CHAR(2), QHMSGF CHAR(10), QHMSGL CHAR(10), QHTEXT CHAR(256))",
    );
    assert!(!ca.is_error(), "{}", ca.line());
    let (ca, _) = db.run(
        "INSERT INTO QSYS.QHST260101 VALUES ('1260101', '000100', 'QPADEV0001', 'CODY', '000009', 'CPF1124', 0, '04', 'QCPFMSG', 'QSYS', 'Job started.'),          ('1251230', '235959', 'OLD', 'CODY', '000001', 'CPF1164', 0, '04', 'QCPFMSG', 'QSYS', 'Job ended.')",
    );
    assert!(!ca.is_error(), "{}", ca.line());
    assert_eq!(
        db.rows("SELECT MESSAGE_ID, MESSAGE_TYPE, MESSAGE_TIMESTAMP, FROM_JOB, MESSAGE_TEXT FROM TABLE(QSYS2.HISTORY_LOG_INFO()) X"),
        vec![row(&["CPF1124", "INFORMATIONAL", "2026-01-01 00:01:00.000000", "000009/CODY/QPADEV0001", "Job started."])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.HISTORY_LOG_INFO(TIMESTAMP('2025-01-01-00.00.00'))) X"), vec![row(&["2"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.HISTORY_LOG_INFO(EOF_DELAY => 5)) X"), -443);
}

#[test]
fn ifs_services() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT PATH_NAME, OBJECT_TYPE, DATA_SIZE, OBJECT_OWNER FROM TABLE(QSYS2.IFS_OBJECT_STATISTICS('/home')) X"),
        vec![row(&["/home", "*DIR", "0", "CODY"]), row(&["/home/a.txt", "*STMF", "12", "CODY"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.IFS_OBJECT_STATISTICS('/home', SUBTREE_DIRECTORIES => 'NO')) X"), vec![row(&["1"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.IFS_OBJECT_STATISTICS('/home', OBJECT_TYPE_LIST => '*STMF')) X"), vec![row(&["1"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.IFS_OBJECT_STATISTICS('/home', IGNORE_ERRORS => 'NO')) X"), -443);
    assert_eq!(
        db.rows("SELECT LINE_NUMBER, LINE FROM TABLE(QSYS2.IFS_READ('/home/a.txt')) X"),
        vec![row(&["1", "one"]), row(&["2", "two"]), row(&["3", "three"])]
    );
    assert_eq!(db.rows("SELECT LINE FROM TABLE(QSYS2.IFS_READ_UTF8(PATH_NAME => '/home/e.txt')) X"), vec![row(&["A"]), row(&["B"])]);
    assert_eq!(db.rows("SELECT HEX(LINE) FROM TABLE(QSYS2.IFS_READ_BINARY('/home/e.txt')) X"), vec![row(&["C115C2"])]);
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.IFS_READ('/nope')) X"), vec![row(&["0"])]);
    assert_eq!(db.code("SELECT * FROM TABLE(QSYS2.IFS_READ('/nope', IGNORE_ERRORS => 'NO')) X"), -443);
}

#[test]
fn qcmdexc_and_procedures() {
    let mut db = Db::new(true);
    assert_eq!(db.rows("SELECT QSYS2.QCMDEXC('CRTLIB X'), QCMDEXC('DLTLIB X') FROM SYSIBM.SYSDUMMY1"), vec![row(&["1", "-1"])]);
    assert_eq!(db.code("SELECT QSYS2.QCMDEXC('CRTLIB X', 'PRINT') FROM SYSIBM.SYSDUMMY1"), -171);
    let mut t = db.store.begin().unwrap();
    assert!(procs::is_procedure(Some("QSYS2"), "QCMDEXC"));
    let ok = procs::call(&mut t, &db.ctx, Some("QSYS2"), "QCMDEXC", &[(None, Value::Str("CRTLIB X".into()))]).unwrap();
    assert!(ok.is_ok());
    let bad = procs::call(&mut t, &db.ctx, Some("QSYS2"), "QCMDEXC", &[(None, Value::Str("DLTLIB X".into())), (None, Value::int(8))]).unwrap();
    assert_eq!(bad.unwrap_err().sqlcode(), -443);
    let w = procs::call(
        &mut t,
        &db.ctx,
        Some("QSYS2"),
        "IFS_WRITE_UTF8",
        &[(Some("PATH_NAME".into()), Value::Str("/home/w.txt".into())), (Some("LINE".into()), Value::Str("hi".into()))],
    )
    .unwrap();
    assert!(w.is_ok(), "{w:?}");
    assert!(procs::call(&mut t, &db.ctx, None, "NOT_A_PROC", &[]).is_none());
    t.abort();
}

#[test]
fn job_description_info() {
    let mut db = Db::new(true);
    {
        let mut t = db.store.begin().unwrap();
        let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse("QGPL").unwrap()).unwrap().id;
        let id = ops::create(
            &mut t,
            &CreateParams {
                library: lib,
                name: Name::parse("QDFTJOBD").unwrap(),
                type_code: types::JOBD,
                attribute: String::new(),
                text: "Default".into(),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now: 1_767_225_600,
                by_system: true,
            },
        )
        .unwrap();
        let d = wm::obj::jobd::JobD { text: "Default".into(), ..wm::obj::jobd::JobD::default() };
        wm::obj::write(&mut t, id, &d.encode()).unwrap();
        t.commit().unwrap();
    }
    assert_eq!(
        db.rows("SELECT JOB_DESCRIPTION_LIBRARY, JOB_DESCRIPTION, AUTHORIZATION_NAME, JOB_QUEUE, LIBRARY_LIST, TEXT_DESCRIPTION FROM QSYS2.JOB_DESCRIPTION_INFO"),
        vec![row(&["QGPL", "QDFTJOBD", "*RQD", "QBATCH", "*SYSVAL", "Default"])]
    );
}

#[test]
fn call_of_service_procedures() {
    let mut db = Db::new(true);
    assert_eq!(db.code("CALL QSYS2.QCMDEXC('CRTLIB X')"), 0);
    assert_eq!(db.code("CALL QCMDEXC('CRTLIB X')"), 0);
    assert_eq!(db.code("CALL QSYS2.QCMDEXC('DLTLIB X')"), -443);
    assert_eq!(db.code("CALL QSYS2.QCMDEXC('CRTLIB X', 8)"), 0);
    assert_eq!(db.code("CALL QSYS2.IFS_WRITE_UTF8(PATH_NAME => '/home/w.txt', LINE => 'hi')"), 0);
    assert_eq!(db.code("CALL QSYS2.IFS_WRITE_UTF8(PATH_NAME => '/home/w.txt', BOGUS => 'hi')"), -440);
}
