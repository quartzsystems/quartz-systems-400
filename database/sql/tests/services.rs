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
        let sv = |name: &str, number: Option<i64>, text: Option<&str>| SysValue {
            name: name.into(),
            number,
            text: text.map(Into::into),
            description: String::new(),
            group: "*SEC".into(),
            shipped: String::new(),
            changeable: true,
        };
        let mut sec: Vec<SysValue> = [
            ("QSECURITY", "40"),
            ("QPWDLVL", "0"),
            ("QPWDEXPWRN", "7"),
            ("QPWDLMTAJC", "0"),
            ("QPWDLMTREP", "0"),
            ("QPWDPOSDIF", "0"),
            ("QPWDRQDDGT", "0"),
            ("QPWDMAXLEN", "8"),
            ("QPWDMINLEN", "6"),
            ("QPWDRQDDIF", "0"),
            ("QMAXSGNACN", "3"),
            ("QVFYOBJRST", "1"),
            ("QLMTSECOFR", "1"),
            ("QAUTOCFG", "1"),
            ("QAUTORMT", "1"),
        ]
        .iter()
        .map(|(n, v)| sv(n, v.parse().ok(), None))
        .collect();
        for (n, v) in [
            ("QPWDCHGBLK", "*NONE"),
            ("QPWDEXPITV", "*NOMAX"),
            ("QPWDLMTCHR", "*NONE"),
            ("QPWDRULES", "*PWDSYSVAL"),
            ("QPWDVLDPGM", "*NONE"),
            ("QCRTAUT", "*CHANGE"),
            ("QCRTOBJAUD", "*NONE"),
            ("QMAXSIGN", "3"),
            ("QALWOBJRST", "*ALL"),
            ("QUSEADPAUT", "*NONE"),
            ("QALWUSRDMN", "*ALL"),
            ("QINACTITV", "*NONE"),
            ("QINACTMSGQ", "*ENDJOB"),
            ("QDSCJOBITV", "240"),
            ("QAUDCTL", "*NONE"),
            ("QAUDLVL", "*NONE"),
            ("QAUDLVL2", "*NONE"),
        ] {
            sec.push(sv(n, None, Some(v)));
        }
        Some(
            sec.into_iter()
                .chain(vec![
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
                .collect(),
        )
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
            return Some(Ok(host::HttpResponse {
                status: 404,
                headers: Vec::new(),
                body: b"gone".to_vec(),
            }));
        }
        let auth = req
            .headers
            .iter()
            .find(|(n, _)| n == "Authorization")
            .map_or(String::new(), |(_, v)| v.clone());
        let body = format!(
            "{} {} [{}] {}",
            req.method,
            req.url,
            auth,
            String::from_utf8_lossy(&req.body)
        );
        Some(Ok(host::HttpResponse {
            status: 200,
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: body.into_bytes(),
        }))
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
            vec![
                e(
                    "CPF1124",
                    "Information",
                    1_767_225_600_000_000,
                    "Job started.",
                ),
                e("", "Request", 1_767_225_660_000_000, "WRKACTJOB"),
            ],
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
            routes: vec![nettools::netstat::Rte {
                dest: Some("127.0.0.0".parse().unwrap()),
                prefix: 8,
                mtu: 576,
                available: true,
                configured: true,
                ifc: Some(lo),
                ..nettools::netstat::Rte::default()
            }],
            connections: vec![nettools::netstat::Conn {
                tcp: true,
                local: "0.0.0.0:21".parse().unwrap(),
                remote: "0.0.0.0:0".parse().unwrap(),
                state: "Listen".into(),
                idle_ms: 311_440,
                user: "QTCP".into(),
                jobs: vec![(
                    "QTFTP00001".into(),
                    "QTCP".into(),
                    "000042".into(),
                    "BCH".into(),
                    "QTCP".into(),
                )],
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

    fn server_shares(&self) -> Option<Result<Vec<host::ServerShare>, String>> {
        Some(Ok(vec![
            host::ServerShare {
                name: "LASER".into(),
                print: true,
                spool_type: "USERASCII".into(),
                outq: ("QUSRSYS".into(), "PRT01".into()),
                driver: "HP LaserJet 4".into(),
                current: 1,
                ..host::ServerShare::default()
            },
            host::ServerShare {
                name: "QIBM".into(),
                text: "Product data".into(),
                path: "/QIBM".into(),
                max_users: -1,
                extension_count: 1,
                extensions: vec![0, 0, 0, 3, 0xE3, 0xE7, 0xE3],
                ..host::ServerShare::default()
            },
        ]))
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
        let o = |p: &str, kind: &str, size: u64| host::IfsObject {
            path: p.into(),
            kind: kind.into(),
            owner: "CODY".into(),
            size,
            mode: 0o755,
            links: 1,
            ..host::IfsObject::default()
        };
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
        Some(if cmd.starts_with("CRTLIB") {
            Ok(())
        } else {
            Err(("CPF0006".into(), "Errors occurred in command.".into()))
        })
    }

    fn ifs_write(
        &self,
        path: &str,
        data: &[u8],
        append: bool,
        ccsid: u16,
    ) -> Option<Result<(), String>> {
        assert_eq!(
            (path, data, append, ccsid),
            ("/home/w.txt", &b"hi\r\n"[..], true, 1208)
        );
        Some(Ok(()))
    }

    fn memory_pools(&self) -> Option<Vec<host::MemoryPool>> {
        Some(vec![
            host::MemoryPool {
                number: 1,
                name: "*MACHINE".into(),
                size_kb: 524_288,
                reserved_kb: 102_400,
                max_active: 32_767,
                elapsed_ms: 5000,
                ..host::MemoryPool::default()
            },
            host::MemoryPool {
                number: 3,
                name: "QINTER/2".into(),
                subsystem: Some(("QSYS".into(), "QINTER".into())),
                size_kb: 20_480,
                max_active: 10,
                active: 2,
                elapsed_ms: 5000,
                db_faults: 1.25,
                ..host::MemoryPool::default()
            },
        ])
    }

    fn disk_units(&self) -> Option<Vec<host::DiskUnit>> {
        Some(vec![host::DiskUnit {
            unit: 1,
            type_code: "SSD".into(),
            model: "QEMU".into(),
            serial: "QM00001".into(),
            status: "Active".into(),
            protection: "None".into(),
            capacity: 20_000_000_000,
            free: 15_000_000_000,
            totals: [100, 50, 800, 400, 20],
            elapsed: [10, 5, 80, 40, 500],
            elapsed_ms: 5000,
        }])
    }

    fn hardware(&self) -> Option<Result<Vec<host::HardwareResource>, String>> {
        let r = |name: &str, cat: &str, parent: &str| host::HardwareResource {
            name: name.into(),
            category: cat.into(),
            operational: true,
            type_model: ("QN00".into(), "001".into()),
            serial: "0123456789ABC".into(),
            text: "Thing".into(),
            parent: parent.into(),
            ..host::HardwareResource::default()
        };
        Some(Ok(vec![
            r("CMB01", "PACKAGING", ""),
            r("LIN01", "COMM", "CMB01"),
        ]))
    }

    fn media_libraries(&self) -> Option<Vec<host::MediaLibrary>> {
        Some(vec![host::MediaLibrary {
            device: "TAPMLB01".into(),
            status: "VARIED ON".into(),
            type_model: ("63B0".into(), "001".into()),
            resource: "TAPMLB01".into(),
            text: "Virtual".into(),
        }])
    }

    fn collection_services(&self) -> Option<host::CollectionServices> {
        Some(host::CollectionServices {
            active: Some((
                "QPFRDATA".into(),
                "Q001000002".into(),
                "*STANDARDP".into(),
                1_767_225_600,
            )),
            library: "QPFRDATA".into(),
            profile: "*STANDARDP".into(),
            interval: 900,
            retention_hours: Some(24),
            cycle: (0, 24),
            create_files: true,
            categories: vec!["*SYSBUS".into(), "*POOL".into()],
        })
    }

    fn exit_points(&self) -> Option<Result<Vec<host::ExitPoint>, String>> {
        Some(Ok(vec![host::ExitPoint {
            name: "QIBM_QTF_TRANSFER".into(),
            format: "TRAN0100".into(),
            registered: true,
            allow_deregistration: true,
            allow_change: true,
            max_programs: Some(1),
            text: "FTP transfer".into(),
            programs: vec![host::ExitProgram {
                number: 1,
                library: "QGPL".into(),
                name: "FTPEXIT".into(),
                threadsafe: "*UNKNOWN".into(),
                action: "*SYSVAL".into(),
                ..host::ExitProgram::default()
            }],
        }]))
    }

    fn environment_variables(&self) -> Option<Vec<host::EnvVar>> {
        Some(vec![
            host::EnvVar {
                system: true,
                name: "QIBM_CCSID".into(),
                value: "1208".into(),
                ccsid: 37,
            },
            host::EnvVar {
                system: false,
                name: "HOME".into(),
                value: "/home/CODY".into(),
                ccsid: 37,
            },
        ])
    }

    fn ptf_groups(&self) -> Option<Result<Vec<host::PtfGroup>, String>> {
        Some(Ok(vec![host::PtfGroup {
            name: "QS00CUM".into(),
            text: "Cumulative".into(),
            level: 3,
            release: "V1R1M0".into(),
            status: "INSTALLED".into(),
            ptfs: vec![("QS00001".into(), "QS00SS1".into(), "APPLIED".into())],
        }]))
    }

    fn connection(&self) -> Option<host::Connection> {
        Some(host::Connection {
            host_name: "qs400.example".into(),
            client: Some("10.0.0.9:50001".parse().unwrap()),
            server: Some("10.0.0.1:8471".parse().unwrap()),
        })
    }

    fn dns_lookup(
        &self,
        name: &str,
        _server: Option<&str>,
    ) -> Option<Result<Vec<std::net::IpAddr>, String>> {
        Some(if name == "localhost" {
            Ok(vec!["127.0.0.1".parse().unwrap(), "::1".parse().unwrap()])
        } else {
            Err("no such host".into())
        })
    }

    fn ifs_authorities(&self, path: &str) -> Option<Result<host::IfsAuthorities, String>> {
        use store::obj::header::Authority as A;
        if path != "/home/a.txt" {
            return Some(Err("ENOENT".into()));
        }
        Some(Ok(host::IfsAuthorities {
            kind: "*STMF".into(),
            owner: "CODY".into(),
            group: String::new(),
            autl: String::new(),
            entries: vec![
                ("*PUBLIC".into(), "*R".into(), A::OBJOPR | A::READ),
                (
                    "CODY".into(),
                    "*RWX".into(),
                    A::OBJOPR
                        | A::READ
                        | A::ADD
                        | A::UPD
                        | A::DLT
                        | A::EXECUTE
                        | A::OBJMGT
                        | A::OBJEXIST
                        | A::OBJALTER
                        | A::OBJREF,
                ),
            ],
        }))
    }

    fn ifs_opens(&self, _path: &str) -> Option<Result<Vec<host::IfsOpen>, String>> {
        Some(Ok(vec![
            host::IfsOpen {
                job: "000009/CODY/QPADEV0001".into(),
                access: (true, false),
                share: "RW".into(),
            },
            host::IfsOpen {
                job: "000009/CODY/QPADEV0001".into(),
                access: (true, true),
                share: "NONE".into(),
            },
        ]))
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
        let mut store = Store::format(
            vec![Box::new(d) as Box<dyn BlockDev>],
            &params,
            StoreConfig::default(),
        )
        .unwrap();
        let (qsys, lib) = {
            let mut t = store.begin().unwrap();
            let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
            let mut lib = None;
            for (name, text) in [
                ("QSTEST", "Test library"),
                ("QGPL", "General purpose"),
                ("QSYS2", "System library for SQL"),
            ] {
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
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(9),
            NameContext {
                curlib: Some(lib),
                libl: vec![lib],
            },
        );
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
                .map(|r| {
                    r.iter()
                        .map(|v| {
                            if v.is_null() {
                                "-".into()
                            } else {
                                v.to_string().trim_end().to_string()
                            }
                        })
                        .collect()
                })
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    fn code(&mut self, text: &str) -> i32 {
        self.run(text).0.sqlcode
    }

    /// A table function's rows, as `TABLE(f(args))` gets them.
    fn function(
        &mut self,
        name: &str,
        args: Vec<(Option<String>, Value)>,
    ) -> sql::Result<(Vec<String>, Vec<Vec<Value>>)> {
        let mut t = self.store.begin().unwrap();
        let n = sql::ast::ObjectName {
            schema: Some(sql::ast::Ident::new("QSYS2")),
            name: sql::ast::Ident::new(name),
        };
        let r = services::table_function(&mut t, &self.ctx, &n, args, "X")?
            .expect("a service table function");
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
    audit_journal(&mut db);
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
        db.rows("SELECT SYSTEM_VALUE_NAME, SYSTEM_VALUE, CURRENT_NUMERIC_VALUE, CURRENT_CHARACTER_VALUE, CATEGORY, CHANGEABLE FROM QSYS2.SYSTEM_VALUE_INFO WHERE SYSTEM_VALUE_NAME IN ('QDATFMT', 'QMAXJOB')"),
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
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SYSTEM_STATUS_INFO_BASIC"),
        vec![row(&["1"])]
    );
}

#[test]
fn without_a_host_the_system_state_is_sql0443() {
    let mut db = Db::new(false);
    assert_eq!(db.code("SELECT * FROM QSYS2.SYSTEM_STATUS_INFO"), -443);
    // The library list is the statement's own then, its parts unknown.
    assert_eq!(
        db.rows("SELECT SYSTEM_SCHEMA_NAME, TYPE FROM QSYS2.LIBRARY_LIST_INFO"),
        vec![row(&["QSTEST", "-"])]
    );
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
    let (_, rows) = db
        .function(
            "ACTIVE_JOB_INFO",
            vec![(Some("JOB_NAME_FILTER".into()), Value::Str("*".into()))],
        )
        .unwrap();
    assert_eq!(rows.len(), 1);
    let (_, rows) = db
        .function(
            "ACTIVE_JOB_INFO",
            vec![(
                Some("SUBSYSTEM_LIST_FILTER".into()),
                Value::Str("QINTER".into()),
            )],
        )
        .unwrap();
    assert_eq!(rows.len(), 2);
    let e = db
        .function(
            "ACTIVE_JOB_INFO",
            vec![(Some("NO_SUCH".into()), Value::Null)],
        )
        .unwrap_err();
    assert!(e.to_string().contains("NO_SUCH"), "{e}");
}

#[test]
fn job_info() {
    let mut db = Db::new(true);
    let (cols, rows) = db.function("JOB_INFO", Vec::new()).unwrap();
    assert_eq!(rows.len(), 1, "the user's own jobs by default");
    let at = |r: &[Value], c: &str| r[cols.iter().position(|x| x == c).unwrap()].to_string();
    assert_eq!(at(&rows[0], "JOB_STATUS"), "ACTIVE");
    let (_, rows) = db
        .function(
            "JOB_INFO",
            vec![(Some("JOB_USER_FILTER".into()), Value::Str("*ALL".into()))],
        )
        .unwrap();
    assert_eq!(rows.len(), 3);
    let (_, rows) = db
        .function("JOB_INFO", vec![(None, Value::Str("*JOBQ".into()))])
        .unwrap();
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
    assert_eq!(
        db.rows("SELECT JOB_NAME FROM TABLE(QSYS2.ACTIVE_JOB_INFO(JOB_NAME_FILTER => '*')) X"),
        vec![row(&["000009/CODY/QPADEV0001"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.ACTIVE_JOB_INFO()) X WHERE JOB_TYPE = 'SBS'"),
        vec![row(&["1"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(ACTIVE_JOB_INFO('NO', 'QINTER')) X"),
        vec![row(&["2"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.ACTIVE_JOB_INFO(BOGUS => 1)) X"),
        -440
    );
}

#[test]
fn base64_and_url_as_pub400() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT QSYS2.BASE64_ENCODE(X'00FF10'), BASE64_ENCODE('x') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["AP8Q", "pw=="])]
    );
    assert_eq!(
        db.rows("SELECT HEX(QSYS2.BASE64_DECODE('SGVsbG8sIFFTLzQwMA==')) FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["48656C6C6F2C2051532F343030"])]
    );
    assert_eq!(
        db.code("SELECT QSYS2.BASE64_DECODE('not base64!') FROM SYSIBM.SYSDUMMY1"),
        -443
    );
    assert_eq!(
        db.rows("SELECT QSYS2.URL_ENCODE('a b&c=d/e?f~g.h-i_j*k+l%') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["a+b%26c%3Dd%2Fe%3Ff%7Eg.h-i_j%2Ak%2Bl%25"])]
    );
    assert_eq!(db.rows("SELECT QSYS2.URL_DECODE('caf%C3%A9'), QSYS2.URL_DECODE('a+b%26c') FROM SYSIBM.SYSDUMMY1"), vec![row(&["café", "a b&c"])]);
    assert_eq!(
        db.code("SELECT QSYS2.URL_DECODE('bad%zz') FROM SYSIBM.SYSDUMMY1"),
        -443
    );
    assert_eq!(
        db.code("SELECT QSYS2.URL_ENCODE('a b', '%20') FROM SYSIBM.SYSDUMMY1"),
        -171
    );
}

#[test]
fn http_functions_through_the_host() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT QSYS2.HTTP_GET('http://h/x') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["GET http://h/x []"])]
    );
    assert_eq!(
        db.rows("SELECT QSYS2.HTTP_POST('http://h/x', 'hi', '{\"basicAuth\":\"u,p\"}') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["POST http://h/x [Basic dTpw] hi"])]
    );
    assert_eq!(
        db.code("SELECT QSYS2.HTTP_GET('http://h/missing') FROM SYSIBM.SYSDUMMY1"),
        -443
    );
    assert_eq!(
        db.code("SELECT QSYS2.HTTP_GET('http://h/x', '{\"proxy\":\"p:1\"}') FROM SYSIBM.SYSDUMMY1"),
        -443
    );
    assert_eq!(
        db.rows(
            "SELECT RESPONSE_HTTP_HEADER FROM TABLE(QSYS2.HTTP_GET_VERBOSE('http://h/missing')) X"
        ),
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
    assert_eq!(
        db.rows(
            "SELECT COUNT(*) FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*ALL', 'ORDERS')) X"
        ),
        vec![row(&["1"])]
    );
    assert_eq!(db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*FILE', OBJECT_NAME => 'ORD*')) X"), vec![row(&["2"])]);
    assert_eq!(
        db.rows("SELECT OBJNAME FROM TABLE(QSYS2.OBJECT_STATISTICS('QSYS', '*LIB', 'QGPL')) X"),
        vec![row(&["QGPL"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.OBJECT_STATISTICS('*LIBL', '*LIB')) X"),
        -443
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.OBJECT_STATISTICS('QSTEST', '*NOTATYPE')) X"),
        -443
    );
}

#[test]
fn user_info_and_groups() {
    let mut db = Db::new(true);
    {
        let mut t = db.store.begin().unwrap();
        let mut g = sec::profile::UsrPrf::new("DEVS", 40);
        g.text = "Developers".into();
        g.gid = 500;
        sec::profile::objects::create(
            &mut t,
            db.ctx.qsys,
            &g,
            "QSECOFR",
            store::obj::header::Authority::EXCLUDE,
            1_767_225_600,
        )
        .unwrap();
        let mut u = sec::profile::UsrPrf::new("CODY", 40);
        u.text = "Cody".into();
        u.grpprf = "DEVS".into();
        sec::profile::objects::create(
            &mut t,
            db.ctx.qsys,
            &u,
            "QSECOFR",
            store::obj::header::Authority::EXCLUDE,
            1_767_225_600,
        )
        .unwrap();
        t.commit().unwrap();
    }
    assert_eq!(
        db.rows("SELECT AUTHORIZATION_NAME, STATUS, USER_CLASS_NAME, GROUP_PROFILE_NAME, TEXT_DESCRIPTION FROM QSYS2.USER_INFO ORDER BY 1"),
        vec![row(&["CODY", "*ENABLED", "*USER", "DEVS", "Cody"]), row(&["DEVS", "*ENABLED", "*USER", "*NONE", "Developers"])]
    );
    assert_eq!(db.rows("SELECT GROUP_MEMBER_INDICATOR FROM QSYS2.USER_INFO_BASIC WHERE AUTHORIZATION_NAME = 'DEVS'"), vec![row(&["YES"])]);
    // Disabled for NetServer: the service's table, not the profile.
    let users = sec::netsvr::users();
    users.started(Some(1));
    assert!(users.failed("CODY", Some(1), 1_767_225_600));
    assert_eq!(
        db.rows(
            "SELECT AUTHORIZATION_NAME, STATUS, NETSERVER_DISABLED FROM QSYS2.USER_INFO ORDER BY 1"
        ),
        vec![
            row(&["CODY", "*ENABLED", "YES"]),
            row(&["DEVS", "*ENABLED", "NO"])
        ]
    );
    assert!(sec::netsvr::enable_netserver_user("CODY"));
    assert_eq!(
        db.rows(
            "SELECT NETSERVER_DISABLED FROM QSYS2.USER_INFO_BASIC WHERE AUTHORIZATION_NAME = 'CODY'"
        ),
        vec![row(&["NO"])]
    );
    assert_eq!(
        db.rows("SELECT * FROM QSYS2.GROUP_PROFILE_ENTRIES"),
        vec![row(&["DEVS", "CODY", "Cody"])]
    );
}

#[test]
fn output_queue_entries_and_spooled_file_data() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT OUTPUT_QUEUE_NAME, OUTPUT_QUEUE_LIBRARY_NAME, SPOOLED_FILE_NAME, STATUS, JOB_NAME, SIZE, DEVICE_FILE_NAME FROM QSYS2.OUTPUT_QUEUE_ENTRIES"),
        vec![row(&["QPRINT", "QGPL", "QSYSPRT", "READY", "000009/CODY/QPADEV0001", "2", "QSYSPRT"])]
    );
    assert_eq!(
        db.rows("SELECT TOTAL_PAGES FROM QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC"),
        vec![row(&["1"])]
    );
    assert_eq!(
        db.rows("SELECT ORDINAL_POSITION, SPOOLED_DATA FROM TABLE(SYSTOOLS.SPOOLED_FILE_DATA(JOB_NAME => '000009/CODY/QPADEV0001', SPOOLED_FILE_NAME => 'QSYSPRT')) X"),
        vec![row(&["1", "LINE ONE"]), row(&["2", "LINE TWO"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(SYSTOOLS.SPOOLED_FILE_DATA('000001/X/Y')) X"),
        vec![row(&["0"])]
    );
}

#[test]
fn joblog_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT ORDINAL_POSITION, MESSAGE_ID, MESSAGE_TYPE, MESSAGE_TEXT, MESSAGE_FILE FROM TABLE(QSYS2.JOBLOG_INFO('*')) X"),
        vec![row(&["1", "CPF1124", "INFORMATIONAL", "Job started.", "QCPFMSG"]), row(&["2", "-", "REQUEST", "WRKACTJOB", "-"])]
    );
    assert_eq!(db.rows("SELECT MESSAGE_TEXT FROM TABLE(QSYS2.JOBLOG_INFO('*', MESSAGE_ORDER => 'DESCENDING', MESSAGE_LIMIT => 1)) X"), vec![row(&["WRKACTJOB"])]);
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.JOBLOG_INFO('000001/X/Y')) X"),
        -443
    );
    assert_eq!(
        db.rows(
            "SELECT COUNT(*) FROM TABLE(QSYS2.JOBLOG_INFO('000001/X/Y', IGNORE_ERRORS => 'YES')) X"
        ),
        vec![row(&["0"])]
    );
}

#[test]
fn netstat_views() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT CONNECTION_TYPE, LOCAL_PORT, LOCAL_PORT_NAME, PROTOCOL, TCP_STATE, IDLE_TIME, BIND_USER, CONNECTION_OPEN_TYPE FROM QSYS2.NETSTAT_INFO"),
        vec![row(&["IPV4", "21", "ftp-control", "TCP", "LISTEN", "311.440", "QTCP", "PASSIVE"])]
    );
    assert_eq!(
        db.rows("SELECT JOB_NAME, JOB_TYPE FROM QSYS2.NETSTAT_JOB_INFO"),
        vec![row(&["000042/QTCP/QTFTP00001", "BATCH"])]
    );
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
fn server_share_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT SERVER_SHARE_NAME, SHARE_TYPE, ENCRYPTION_REQUIRED, SPOOLED_FILE_TYPE, OUTPUT_QUEUE, PUBLISH_PRINT_SHARE, CURRENT_CONNECTIONS FROM QSYS2.SERVER_SHARE_INFO WHERE SHARE_TYPE = 'PRINT'"),
        vec![row(&["LASER", "PRINT", "NO", "USERASCII", "PRT01", "NO", "1"])]
    );
    assert_eq!(
        db.rows("SELECT PATH_NAME, PERMISSIONS, COALESCE(MAXIMUM_CONNECTIONS, -9), FILE_EXTENSION_COUNT, LENGTH(FILE_EXTENSIONS), COALESCE(OUTPUT_QUEUE, '-') FROM QSYS2.SERVER_SHARE_INFO WHERE SERVER_SHARE_NAME = 'QIBM'"),
        vec![row(&["/QIBM", "RW", "-9", "1", "7", "-"])]
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
    assert_eq!(
        db.rows(
            "SELECT COUNT(*) FROM TABLE(QSYS2.HISTORY_LOG_INFO(TIMESTAMP('2025-01-01-00.00.00'))) X"
        ),
        vec![row(&["2"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.HISTORY_LOG_INFO(EOF_DELAY => 5)) X"),
        -443
    );
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
    assert_eq!(
        db.code(
            "SELECT * FROM TABLE(QSYS2.IFS_OBJECT_STATISTICS('/home', IGNORE_ERRORS => 'NO')) X"
        ),
        -443
    );
    assert_eq!(
        db.rows("SELECT LINE_NUMBER, LINE FROM TABLE(QSYS2.IFS_READ('/home/a.txt')) X"),
        vec![row(&["1", "one"]), row(&["2", "two"]), row(&["3", "three"])]
    );
    assert_eq!(
        db.rows("SELECT LINE FROM TABLE(QSYS2.IFS_READ_UTF8(PATH_NAME => '/home/e.txt')) X"),
        vec![row(&["A"]), row(&["B"])]
    );
    assert_eq!(
        db.rows("SELECT HEX(LINE) FROM TABLE(QSYS2.IFS_READ_BINARY('/home/e.txt')) X"),
        vec![row(&["C115C2"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.IFS_READ('/nope')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.IFS_READ('/nope', IGNORE_ERRORS => 'NO')) X"),
        -443
    );
}

#[test]
fn qcmdexc_and_procedures() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT QSYS2.QCMDEXC('CRTLIB X'), QCMDEXC('DLTLIB X') FROM SYSIBM.SYSDUMMY1"),
        vec![row(&["1", "-1"])]
    );
    assert_eq!(
        db.code("SELECT QSYS2.QCMDEXC('CRTLIB X', 'PRINT') FROM SYSIBM.SYSDUMMY1"),
        -171
    );
    let mut t = db.store.begin().unwrap();
    assert!(procs::is_procedure(Some("QSYS2"), "QCMDEXC"));
    let ok = procs::call(
        &mut t,
        &mut db.ctx,
        Some("QSYS2"),
        "QCMDEXC",
        &[(None, Value::Str("CRTLIB X".into()))],
    )
    .unwrap();
    assert!(ok.is_ok());
    let bad = procs::call(
        &mut t,
        &mut db.ctx,
        Some("QSYS2"),
        "QCMDEXC",
        &[(None, Value::Str("DLTLIB X".into())), (None, Value::int(8))],
    )
    .unwrap();
    assert_eq!(bad.unwrap_err().sqlcode(), -443);
    let w = procs::call(
        &mut t,
        &mut db.ctx,
        Some("QSYS2"),
        "IFS_WRITE_UTF8",
        &[
            (Some("PATH_NAME".into()), Value::Str("/home/w.txt".into())),
            (Some("LINE".into()), Value::Str("hi".into())),
        ],
    )
    .unwrap();
    assert!(w.is_ok(), "{w:?}");
    assert!(procs::call(&mut t, &mut db.ctx, None, "NOT_A_PROC", &[]).is_none());
    t.abort();
}

/// Create `lib/name` of `ty` holding `payload`.
fn put_object(db: &mut Db, lib: &str, name: &str, ty: u16, payload: &[u8]) {
    let mut t = db.store.begin().unwrap();
    let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse(lib).unwrap())
        .unwrap()
        .id;
    let id = ops::create(
        &mut t,
        &CreateParams {
            library: lib,
            name: Name::parse(name).unwrap(),
            type_code: ty,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: 1_767_225_600,
            by_system: true,
        },
    )
    .unwrap();
    wm::obj::write(&mut t, id, payload).unwrap();
    t.commit().unwrap();
}

#[test]
fn job_queues_and_subsystems() {
    let mut db = Db::new(true);
    let mut q = wm::obj::jobq::JobQ {
        text: "Interactive queue".into(),
        autchk: "*OWNER".into(),
        ..wm::obj::jobq::JobQ::default()
    };
    q.jobs.push(wm::obj::jobq::QJob {
        number: 50,
        name: "NIGHTLY".into(),
        user: "CODY".into(),
        jobpty: 5,
        held: true,
        ..wm::obj::jobq::QJob::default()
    });
    put_object(&mut db, "QGPL", "QINTER", types::JOBQ, &q.encode());
    let mut s = wm::obj::sbsd::Sbsd {
        text: "Interactive".into(),
        ..wm::obj::sbsd::Sbsd::default()
    };
    s.jobqes.push(wm::obj::sbsd::Jobqe {
        jobq: ("QINTER".into(), "QGPL".into()),
        seq: 10,
        maxact: 1,
        maxpty: [0; 9],
    });
    put_object(&mut db, "QGPL", "QINTER", types::SBSD, &s.encode());
    assert_eq!(
        db.rows("SELECT JOB_QUEUE_NAME, JOB_QUEUE_STATUS, NUMBER_OF_JOBS, HELD_JOBS, HELD_JOBS_PRIORITY_5, SUBSYSTEM_NAME, SEQUENCE_NUMBER FROM QSYS2.JOB_QUEUE_INFO"),
        vec![row(&["QINTER", "RELEASED", "1", "1", "1", "QINTER", "10"])]
    );
    assert_eq!(
        db.rows("SELECT SUBSYSTEM_DESCRIPTION, STATUS, CURRENT_ACTIVE_JOBS, SUBSYSTEM_MONITOR_JOB FROM QSYS2.SUBSYSTEM_INFO"),
        vec![row(&["QINTER", "ACTIVE", "1", "000005/QSYS/QINTER"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.SUBSYSTEM_POOL_INFO"),
        vec![row(&[&s.pools.len().to_string()])]
    );
}

#[test]
fn job_description_info() {
    let mut db = Db::new(true);
    {
        let mut t = db.store.begin().unwrap();
        let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse("QGPL").unwrap())
            .unwrap()
            .id;
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
        let d = wm::obj::jobd::JobD {
            text: "Default".into(),
            ..wm::obj::jobd::JobD::default()
        };
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
    assert_eq!(
        db.code("CALL QSYS2.IFS_WRITE_UTF8(PATH_NAME => '/home/w.txt', LINE => 'hi')"),
        0
    );
    assert_eq!(
        db.code("CALL QSYS2.IFS_WRITE_UTF8(PATH_NAME => '/home/w.txt', BOGUS => 'hi')"),
        -440
    );
}

#[test]
fn object_privileges_and_ownership() {
    let mut db = Db::new(true);
    let (ca, _) = db.run("CREATE TABLE QSTEST.T1 (A INT)");
    assert!(!ca.is_error(), "{}", ca.line());
    let rows = db.rows("SELECT AUTHORIZATION_NAME, OBJECT_AUTHORITY, DATA_READ FROM QSYS2.OBJECT_PRIVILEGES WHERE SYSTEM_OBJECT_NAME = 'T1' ORDER BY 1");
    assert!(rows.iter().any(|r| r[0] == "*PUBLIC"), "{rows:?}");
    assert!(
        rows.iter().any(|r| r[1] == "*ALL" && r[2] == "YES"),
        "{rows:?}"
    );
    assert_eq!(db.rows("SELECT OBJECT_LIBRARY, OBJECT_TYPE FROM QSYS2.OBJECT_OWNERSHIP WHERE OBJECT_NAME = 'T1'"), vec![row(&["QSTEST", "*FILE"])]);
}

#[test]
fn journal_views() {
    let mut db = Db::new(true);
    for st in ["CREATE SCHEMA JSCH", "CREATE TABLE JSCH.T1 (A INT)"] {
        let (ca, _) = db.run(st);
        assert!(!ca.is_error(), "{st}: {}", ca.line());
    }
    assert_eq!(
        db.rows("SELECT JOURNAL_NAME, JOURNAL_LIBRARY, NUMBER_JOURNAL_RECEIVERS, JOURNALED_FILES FROM QSYS2.JOURNAL_INFO"),
        vec![row(&["QSQJRN", "JSCH", "1", "1"])]
    );
    assert_eq!(
        db.rows("SELECT JOURNAL_NAME, STATUS FROM QSYS2.JOURNAL_RECEIVER_INFO"),
        vec![row(&["QSQJRN", "ATTACHED"])]
    );
    assert_eq!(
        db.rows("SELECT OBJECT_TYPE, OBJECT_LIBRARY, OBJECT_NAME, FILE_TYPE FROM QSYS2.JOURNALED_OBJECTS"),
        vec![row(&["*FILE", "JSCH", "T1", "PHYSICAL"])]
    );
}

#[test]
fn spooled_file_info_and_system_status() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT SPOOLED_FILE_NAME, STATUS, QUALIFIED_JOB_NAME, OUTPUT_QUEUE FROM TABLE(QSYS2.SPOOLED_FILE_INFO()) X"),
        vec![row(&["QSYSPRT", "READY", "000009/CODY/QPADEV0001", "QPRINT"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.SPOOLED_FILE_INFO(USER_NAME => 'OTHER')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.SPOOLED_FILE_INFO(STATUS => '*HELD')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT ACTIVE_JOBS_IN_SYSTEM FROM TABLE(QSYS2.SYSTEM_STATUS()) X"),
        vec![row(&["38"])]
    );
}

#[test]
fn library_info() {
    let mut db = Db::new(true);
    let (ca, _) = db.run("CREATE TABLE QSTEST.T1 (A INT)");
    assert!(!ca.is_error(), "{}", ca.line());
    assert_eq!(db.rows("SELECT OBJECT_COUNT, LIBRARY_SIZE_COMPLETE, TEXT_DESCRIPTION FROM TABLE(QSYS2.LIBRARY_INFO('QSTEST')) X"), vec![row(&["1", "YES", "Test library"])]);
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.LIBRARY_INFO('NOPE')) X"),
        vec![row(&["0"])]
    );
}

/// `QSYS/QAUDJRN` with one `PW` entry.
fn audit_journal(db: &mut Db) {
    let mut t = db.store.begin().unwrap();
    let qsys = db.ctx.qsys;
    let rcv = jrn::rcv::crtjrnrcv(
        &mut t,
        qsys,
        Name::parse("AUDRCV0001").unwrap(),
        &jrn::rcv::CrtJrnRcvOpts::default(),
    )
    .unwrap();
    let opts = jrn::CrtJrnOpts {
        text: String::new(),
        mngrcv_system: true,
        dltrcv: false,
        minentdta: false,
        fixlendta: false,
        rcvsizopt: 0,
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: 1_767_225_600,
    };
    jrn::journal::crtjrn(&mut t, qsys, Name::parse("QAUDJRN").unwrap(), rcv, &opts).unwrap();
    let ev = sec::audit::Event::new(sec::audit::Kind::by_code("PW").unwrap())
        .set("ETYP", "P")
        .set("USRP", "BADGUY")
        .set("DEVN", "QPADEV0009");
    let job = jrn::JobInfo {
        name: "QPADEV0009".into(),
        user: "QSYS".into(),
        number: 12,
    };
    sec::audit::write::write(&mut t, qsys, &[(ev, job)]).unwrap();
    t.commit().unwrap();
}

#[test]
fn message_file_data() {
    let mut db = Db::new(true);
    {
        let mut t = db.store.begin().unwrap();
        let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse("QGPL").unwrap())
            .unwrap()
            .id;
        let c = msgf::store::Ctx {
            now: 1_767_225_600,
            owner: "QSYS".into(),
            system: "QS400".into(),
        };
        let id = msgf::store::crtmsgf(
            &mut t,
            &c,
            lib,
            Name::parse("MYMSGF").unwrap(),
            "Mine",
            false,
        )
        .unwrap();
        let d = msgf::MsgDesc {
            id: "USR0001".into(),
            severity: 30,
            text: "File &1 not found.".into(),
            help: "Look again.".into(),
            vars: vec![msgf::VarType::Char(10)],
            dft_reply: Some("C".into()),
            valid_replies: vec!["C".into(), "R".into()],
            dump: Vec::new(),
        };
        msgf::store::addmsgd(&mut t, id, d).unwrap();
        t.commit().unwrap();
    }
    assert_eq!(
        db.rows("SELECT MESSAGE_FILE, MESSAGE_ID, MESSAGE_TEXT, SEVERITY, MESSAGE_DATA_COUNT, MESSAGE_DATA, DEFAULT_REPLY, VALID_REPLY_VALUES, CREATION_DATE FROM QSYS2.MESSAGE_FILE_DATA"),
        vec![row(&["MYMSGF", "USR0001", "File &1 not found.", "30", "1", "*CHAR 10", "C", "C R", "-"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.MESSAGE_FILE_DATA('QGPL', 'MYMSGF')) X"),
        vec![row(&["1"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.MESSAGE_FILE_DATA('QGPL', 'NONE')) X"),
        vec![row(&["0"])]
    );
}

#[test]
fn security_info() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT SECURITY_LEVEL, PASSWORD_LEVEL, AUDIT_JOURNAL_EXISTS, PASSWORD_EXPIRATION_INTERVAL, MAXIMUM_SIGNON_ATTEMPTS, PENDING_SECURITY_LEVEL FROM QSYS2.SECURITY_INFO"),
        vec![row(&["40", "0", "NO", "*NOMAX", "3", "-"])]
    );
    audit_journal(&mut db);
    assert_eq!(db.rows("SELECT AUDIT_JOURNAL_EXISTS, AUDIT_JOURNAL_RECEIVER FROM TABLE(QSYS2.SECURITY_INFO()) X"), vec![row(&["YES", "AUDRCV0001"])]);
}

#[test]
fn audit_journal_and_display_journal() {
    let mut db = Db::new(true);
    audit_journal(&mut db);
    assert_eq!(
        db.rows("SELECT QUALIFIED_JOB_NAME, VIOLATION_TYPE, AUDIT_USER_NAME, DEVICE_NAME, RECEIVER_NAME FROM TABLE(SYSTOOLS.AUDIT_JOURNAL_PW()) X"),
        vec![row(&["000012/QSYS/QPADEV0009", "P", "BADGUY", "QPADEV0009", "AUDRCV0001"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(SYSTOOLS.AUDIT_JOURNAL_AF()) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(SYSTOOLS.AUDIT_JOURNAL_PW(USER_NAME => 'NOBODY')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT JOURNAL_CODE, JOURNAL_ENTRY_TYPE, JOB_NUMBER FROM TABLE(QSYS2.DISPLAY_JOURNAL('QSYS', 'QAUDJRN')) X"),
        vec![row(&["T", "PW", "000012"])]
    );
    for st in [
        "CREATE SCHEMA JSCH",
        "CREATE TABLE JSCH.T1 (A INT)",
        "INSERT INTO JSCH.T1 VALUES 1, 2",
    ] {
        let (ca, _) = db.run(st);
        assert!(!ca.is_error(), "{st}: {}", ca.line());
    }
    let rows = db.rows("SELECT JOURNAL_ENTRY_TYPE, OBJECT, COUNT_OR_RRN FROM TABLE(QSYS2.DISPLAY_JOURNAL('JSCH', 'QSQJRN', JOURNAL_ENTRY_TYPES => 'PT')) X");
    assert_eq!(
        rows,
        vec![
            row(&["PT", "T1        JSCH      T1", "1"]),
            row(&["PT", "T1        JSCH      T1", "2"])
        ]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.DISPLAY_JOURNAL('JSCH', 'QSQJRN', EOF_DELAY => 5)) X"),
        -443
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.DISPLAY_JOURNAL('JSCH', 'NOPE')) X"),
        -443
    );
}

#[test]
fn services_qs400_does_not_have_answer_no_rows() {
    let mut db = Db::new(true);
    for v in [
        "FUNCTION_INFO",
        "FUNCTION_USAGE",
        "LICENSE_INFO",
        "SYSLIMTBL",
        "SYSLIMITS",
        "ELECTRONIC_SERVICE_AGENT_INFO",
    ] {
        assert_eq!(
            db.rows(&format!("SELECT COUNT(*) FROM QSYS2.{v}")),
            vec![row(&["0"])],
            "{v}"
        );
    }
}

#[test]
fn pools_activity_asp_and_disks() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT SYSTEM_POOL_ID, POOL_NAME, CURRENT_SIZE, CURRENT_THREADS, SUBSYSTEM_NAME, ELAPSED_DATABASE_FAULTS, PAGING_OPTION FROM QSYS2.MEMORY_POOL_INFO"),
        vec![row(&["1", "*MACHINE", "512.00", "0", "-", "0.0", "*FIXED"]), row(&["3", "2", "20.00", "2", "QINTER", "1.2", "*FIXED"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.MEMORY_POOL()) X"),
        vec![row(&["2"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.MEMORY_POOL(RESET_STATISTICS => 'YES')) X"),
        -443
    );
    assert_eq!(db.rows("SELECT AVERAGE_CPU_UTILIZATION, MAXIMUM_CPU_UTILIZATION FROM TABLE(QSYS2.SYSTEM_ACTIVITY_INFO()) X"), vec![row(&["3.25", "-"])]);
    assert_eq!(
        db.rows("SELECT ASP_NUMBER, NUMBER_OF_DISK_UNITS, TOTAL_CAPACITY, TOTAL_CAPACITY_AVAILABLE, ASP_TYPE FROM QSYS2.ASP_INFO"),
        vec![row(&["1", "1", "19073", "14305", "-"])]
    );
    assert_eq!(
        db.rows("SELECT UNIT_NUMBER, DISK_TYPE, UNIT_TYPE, PERCENT_USED, TOTAL_READ_REQUESTS, ELAPSED_IO_REQUESTS, ELAPSED_PERCENT_BUSY FROM QSYS2.SYSDISKSTAT"),
        vec![row(&["1", "SSD", "1", "25.000", "100", "3.0", "10.0"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM QSYS2.ASP_VARY_INFO"),
        vec![row(&["0"])]
    );
}

#[test]
fn hardware_media_and_collection_services() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT ID, PARENT_ID, RESOURCE_CATEGORY, RESOURCE_NAME, STATUS, SERIAL_NUMBER_SHORT FROM QSYS2.HARDWARE_RESOURCE_INFO"),
        vec![row(&["1", "-", "PACKAGING", "CMB01", "OPERATIONAL", "3456789ABC"]), row(&["2", "1", "COMM", "LIN01", "OPERATIONAL", "3456789ABC"])]
    );
    assert_eq!(
        db.rows("SELECT RESOURCE_NAME FROM TABLE(QSYS2.HARDWARE_RESOURCE_INFO('*CMN')) X"),
        vec![row(&["LIN01"])]
    );
    assert_eq!(db.rows("SELECT DEVICE_NAME, DEVICE_STATUS, RESOURCE_ALLOCATION_PRIORITY FROM QSYS2.MEDIA_LIBRARY_INFO"), vec![row(&["TAPMLB01", "VARIED ON", "-"])]);
    assert_eq!(
        db.rows("SELECT ACTIVE_COLLECTION_NAME, COLLECTION_LIBRARY, DEFAULT_COLLECTION_INTERVAL, CATEGORY_LIST_COUNT, CATEGORY_LIST FROM QSYS2.COLLECTION_SERVICES_INFO"),
        vec![row(&["Q001000002", "QPFRDATA", "900", "2", "*SYSBUS *POOL"])]
    );
}

#[test]
fn exits_environment_ptf_groups_and_network() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT EXIT_POINT_NAME, EXIT_POINT_FORMAT, REGISTERED, EXIT_PROGRAMS, MAXIMUM_EXIT_PROGRAMS FROM QSYS2.EXIT_POINT_INFO"),
        vec![row(&["QIBM_QTF_TRANSFER", "TRAN0100", "YES", "1", "1"])]
    );
    assert_eq!(
        db.rows("SELECT EXIT_PROGRAM_NUMBER, EXIT_PROGRAM_LIBRARY, EXIT_PROGRAM, THREADSAFE, MULTITHREADED_JOB_ACTION FROM QSYS2.EXIT_PROGRAM_INFO"),
        vec![row(&["1", "QGPL", "FTPEXIT", "UNK", "SYSVAL"])]
    );
    assert_eq!(
        db.rows("SELECT ENVIRONMENT_VARIABLE_TYPE, ENVIRONMENT_VARIABLE_NAME, ENVIRONMENT_VARIABLE_VALUE, HEX(ENVIRONMENT_VARIABLE_BINARY_NAME) FROM QSYS2.ENVIRONMENT_VARIABLE_INFO"),
        vec![row(&["JOB", "HOME", "/home/CODY", "C8D6D4C5"]), row(&["SYSTEM", "QIBM_CCSID", "1208", "D8C9C2D46DC3C3E2C9C4"])]
    );
    assert_eq!(
        db.rows(
            "SELECT PTF_GROUP_NAME, PTF_GROUP_LEVEL, PTF_GROUP_STATUS FROM QSYS2.GROUP_PTF_INFO"
        ),
        vec![row(&["QS00CUM", "3", "INSTALLED"])]
    );
    assert_eq!(
        db.rows(
            "SELECT PTF_IDENTIFIER, PTF_STATUS FROM TABLE(SYSTOOLS.GROUP_PTF_DETAILS('QS00CUM')) X"
        ),
        vec![row(&["QS00001", "APPLIED"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(SYSTOOLS.GROUP_PTF_DETAILS('OTHER')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.rows("SELECT LOCAL_HOST_NAME, CLIENT_IP_ADDRESS_TYPE, CLIENT_IP_ADDRESS, SERVER_PORT_NUMBER FROM QSYS2.TCPIP_INFO"),
        vec![row(&["qs400.example", "IPV4", "10.0.0.9", "8471"])]
    );
    assert_eq!(
        db.rows(
            "SELECT ADDRESS_SPACE_TYPE, IP_ADDRESS FROM TABLE(QSYS2.DNS_LOOKUP('localhost')) X"
        ),
        vec![row(&["IPV4", "127.0.0.1"]), row(&["IPV6", "::1"])]
    );
    assert_eq!(
        db.code("SELECT * FROM TABLE(QSYS2.DNS_LOOKUP('nowhere')) X"),
        -443
    );
}

#[test]
fn object_lock_info() {
    let mut db = Db::new(true);
    let (ca, _) = db.run("CREATE TABLE QSTEST.LK (A INT)");
    assert!(!ca.is_error(), "{}", ca.line());
    {
        let mut t = db.store.begin().unwrap();
        let lib = ops::find_library(&mut t, db.ctx.qsys, Name::parse("QSTEST").unwrap())
            .unwrap()
            .id;
        let (_, id) = ops::list(&mut t, lib, Some(types::FILE))
            .unwrap()
            .into_iter()
            .find(|(k, _)| k.name.to_string() == "LK")
            .unwrap();
        t.abort();
        db.store
            .locks_mut()
            .lock(JobHandle(9), id, store::obj::lock::LockState::SharedRead)
            .unwrap();
    }
    assert_eq!(
        db.rows("SELECT SYSTEM_OBJECT_SCHEMA, SYSTEM_OBJECT_NAME, OBJECT_TYPE, LOCK_STATE, LOCK_STATUS, LOCK_COUNT FROM QSYS2.OBJECT_LOCK_INFO WHERE SYSTEM_OBJECT_NAME = 'LK'"),
        vec![row(&["QSTEST", "LK", "*FILE", "*SHRRD", "HELD", "1"])]
    );
    assert_eq!(
        db.rows(
            "SELECT LOCK_STATE FROM TABLE(QSYS2.OBJECT_LOCK_INFO('QSTEST', 'LK', '*FILE', 1)) X"
        ),
        vec![row(&["*SHRRD"])]
    );
}

#[test]
fn change_user_profile_and_refused_procedures() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT USER_NAME, CHANGE_ATTEMPTED, CHGUSRPRF_COMMAND FROM TABLE(SYSTOOLS.CHANGE_USER_PROFILE(P_USER_NAME => 'BOB', P_STATUS => '*DISABLED', P_TEXT => 'Bob''s')) X"),
        vec![row(&["BOB", "NO", "CHGUSRPRF USRPRF(BOB) STATUS(*DISABLED) TEXT('Bob''s')"])]
    );
    assert_eq!(
        db.rows("SELECT CHANGE_ATTEMPTED, CHANGE_SUCCESSFUL, FAILURE_MESSAGE_ID FROM TABLE(SYSTOOLS.CHANGE_USER_PROFILE('BOB', P_STATUS => '*DISABLED', PREVIEW => 'NO')) X"),
        vec![row(&["YES", "NO", "CPF0006"])]
    );
    assert_eq!(db.code("SELECT * FROM TABLE(SYSTOOLS.CHANGE_USER_PROFILE('BOB', P_SPECIAL_AUTHORITY_ACTION => '*ADD', P_SPECIAL_AUTHORITY => '*JOBCTL')) X"), -84);
    assert_eq!(
        db.code("CALL QSYS2.SET_SERVER_SBS_ROUTING('BOB', 'QZDASOINIT', 'QSERVER')"),
        -84
    );
    assert_eq!(
        db.code("CALL QSYS2.SET_PASE_SHELL_INFO('BOB', '/QOpenSys/pkgs/bin/bash')"),
        -84
    );
    assert_eq!(db.code("CALL SYSTOOLS.LPRINTF('hello')"), -84);
    assert_eq!(
        db.code(
            "VALUES SYSTOOLS.GENERATE_SPREADSHEET('/home/x', 'SELECT 1 FROM SYSIBM.SYSDUMMY1')"
        ),
        -84
    );
    let (ca, _) = db.run("CREATE TABLE QSTEST.OT (A INT)");
    assert!(!ca.is_error(), "{}", ca.line());
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_TABLE('QSTEST', 'OT', '*BUF256KB')"),
        0
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_TABLE('QSTEST', 'OT', 'LOTS')"),
        -171
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_TABLE('QSTEST', 'NOPE', '*DEFAULT')"),
        -204
    );
}

/// The records `GENERATE_SQL` left open for its caller.
fn generated(db: &Db) -> Vec<String> {
    let c = db.ctx.cursors.get("QSQGENSQL").expect("the result set");
    c.rows.iter().map(|r| r[2].to_string()).collect()
}

/// A record as pub400 shows it: the text, a blank and a line feed,
/// padded to 80.
fn rec(s: &str) -> String {
    format!("{s:<80}")
}

#[test]
fn generate_sql_as_pub400_lays_it_out() {
    let mut db = Db::new(true);
    for st in [
        "CREATE SCHEMA GS",
        "CREATE TABLE GS.GSQ_DEPT (DEPTNO CHAR(3) NOT NULL PRIMARY KEY, DEPTNAME VARCHAR(36) NOT NULL, BUDGET DECIMAL(11, 2) DEFAULT 0)",
        "CREATE TABLE GS.GSQ_EMP (EMPNO INTEGER GENERATED ALWAYS AS IDENTITY (START WITH 100 INCREMENT BY 1), LASTNAME VARCHAR(20) NOT NULL, DEPTNO CHAR(3), SALARY DECIMAL(9, 2) CHECK (SALARY >= 0), CONSTRAINT GSQ_EMP_PK PRIMARY KEY (EMPNO), CONSTRAINT GSQ_EMP_FK FOREIGN KEY (DEPTNO) REFERENCES GS.GSQ_DEPT (DEPTNO) ON DELETE SET NULL, CONSTRAINT GSQ_EMP_U UNIQUE (LASTNAME, DEPTNO))",
        "LABEL ON TABLE GS.GSQ_EMP IS 'Employees'",
        "LABEL ON COLUMN GS.GSQ_EMP (LASTNAME IS 'Last name')",
        "CREATE INDEX GS.GSQ_EMP_IX ON GS.GSQ_EMP (LASTNAME DESC, DEPTNO)",
        "CREATE SEQUENCE GS.GSQ_SEQ AS INTEGER START WITH 10 INCREMENT BY 5 MAXVALUE 1000 CYCLE",
        "CREATE ALIAS GS.GSQ_ALIAS FOR GS.GSQ_DEPT",
    ] {
        let (ca, _) = db.run(st);
        assert!(!ca.is_error(), "{st}: {}", ca.line());
    }
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('GSQ_DEPT', 'GS', 'TABLE', HEADER_OPTION => '0', PRIVILEGES_OPTION => '0', DATABASE_SOURCE_FILE_LIBRARY_NAME => 'GS')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    let pk = db.rows("SELECT CONSTRAINT_NAME FROM QSYS2.SYSCST WHERE TABLE_NAME = 'GSQ_DEPT'")[0]
        [0]
    .clone();
    let dept = vec![
        rec("CREATE TABLE GS.GSQ_DEPT ( \n"),
        rec("\tDEPTNO CHAR(3) CCSID 37 NOT NULL , \n"),
        rec("\tDEPTNAME VARCHAR(36) CCSID 37 NOT NULL , \n"),
        rec("\tBUDGET DECIMAL(11, 2) DEFAULT 0 , \n"),
        rec(&format!(
            "\tCONSTRAINT GS.{pk} PRIMARY KEY( DEPTNO ) )   \n"
        )),
        rec("\t  \n"),
        rec("\tRCDFMT GSQ_DEPT   ; \n"),
        rec("  \n"),
    ];
    assert_eq!(generated(&db), dept);
    // The member has the same records.
    let member: Vec<String> = db
        .rows("SELECT SRCDTA FROM GS.Q_GENSQL ORDER BY SRCSEQ")
        .into_iter()
        .map(|r| r[0].clone())
        .collect();
    assert_eq!(
        member,
        dept.iter()
            .map(|r| r.trim_end().to_string())
            .collect::<Vec<_>>()
    );

    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('GSQ_EMP', 'GS', 'TABLE', HEADER_OPTION => '0', PRIVILEGES_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    let ck = db.rows("SELECT CONSTRAINT_NAME FROM QSYS2.SYSCST WHERE TABLE_NAME = 'GSQ_EMP' AND CONSTRAINT_TYPE = 'CHECK'")[0][0].clone();
    let emp: Vec<String> = [
        "CREATE TABLE GS.GSQ_EMP ( ",
        "\tEMPNO INTEGER GENERATED ALWAYS AS IDENTITY ( ",
        "\tSTART WITH 100 INCREMENT BY 1 ",
        "\tNO MINVALUE NO MAXVALUE ",
        "\tNO CYCLE NO ORDER ",
        "\tCACHE 20 ) ",
        "\t, ",
        "\tLASTNAME VARCHAR(20) CCSID 37 NOT NULL , ",
        "\tDEPTNO CHAR(3) CCSID 37 DEFAULT NULL , ",
        "\tSALARY DECIMAL(9, 2) DEFAULT NULL , ",
        "\tCONSTRAINT GS.GSQ_EMP_PK PRIMARY KEY( EMPNO ) )   ",
        "\t  ",
        "\tRCDFMT GSQ_EMP    ; ",
        "  ",
        "ALTER TABLE GS.GSQ_EMP ",
        "\tADD CONSTRAINT GS.GSQ_EMP_U ",
        "\tUNIQUE( LASTNAME , DEPTNO ) ; ",
        "  ",
        "ALTER TABLE GS.GSQ_EMP ",
        "\tADD CONSTRAINT GS.GSQ_EMP_FK ",
        "\tFOREIGN KEY( DEPTNO ) ",
        "\tREFERENCES GS.GSQ_DEPT ( DEPTNO ) ",
        "\tON DELETE SET NULL ",
        "\tON UPDATE NO ACTION ; ",
        "  ",
        "ALTER TABLE GS.GSQ_EMP ",
        &format!("\tADD CONSTRAINT GS.{ck} "),
        "\tCHECK( SALARY >= 0 ) ; ",
        "  ",
        "LABEL ON TABLE GS.GSQ_EMP ",
        "\tIS 'Employees' ; ",
        "  ",
        "LABEL ON COLUMN GS.GSQ_EMP ",
        "( LASTNAME IS 'Last name' ) ; ",
        "  ",
    ]
    .iter()
    .map(|l| rec(&format!("{l}\n")))
    .collect();
    assert_eq!(generated(&db), emp);

    let (ca, _) =
        db.run("CALL QSYS2.GENERATE_SQL('GSQ_EMP_IX', 'GS', 'INDEX', HEADER_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    assert_eq!(
        generated(&db),
        [
            "CREATE INDEX GS.GSQ_EMP_IX ",
            "\tON GS.GSQ_EMP ( LASTNAME DESC , DEPTNO ASC )   ",
            "\t; ",
            "  "
        ]
        .iter()
        .map(|l| rec(&format!("{l}\n")))
        .collect::<Vec<_>>()
    );
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('GSQ_SEQ', 'GS', 'SEQUENCE', HEADER_OPTION => '0', PRIVILEGES_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    assert_eq!(
        generated(&db),
        [
            "CREATE SEQUENCE GS.GSQ_SEQ ",
            "\tAS INTEGER ",
            "\tSTART WITH 10 ",
            "\tINCREMENT BY 5 ",
            "\tMINVALUE 10 ",
            "\tMAXVALUE 1000 ",
            "\tCYCLE CACHE 20 NO ORDER ; ",
            "  "
        ]
        .iter()
        .map(|l| rec(&format!("{l}\n")))
        .collect::<Vec<_>>()
    );
    let (ca, _) =
        db.run("CALL QSYS2.GENERATE_SQL('GSQ_ALIAS', 'GS', 'ALIAS', HEADER_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    assert_eq!(
        generated(&db),
        ["CREATE ALIAS GS.GSQ_ALIAS ", "\tFOR GS.GSQ_DEPT ; ", "  "]
            .iter()
            .map(|l| rec(&format!("{l}\n")))
            .collect::<Vec<_>>()
    );

    // Unformatted, unqualified, with a DROP, no system names.
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('GSQ_DEPT', 'GS', 'TABLE', HEADER_OPTION => '0', DROP_OPTION => '1', STATEMENT_FORMATTING_OPTION => '0', SYSTEM_NAME_OPTION => '0', QUALIFIED_NAME_OPTION => '1', PRIVILEGES_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    assert_eq!(
        generated(&db),
        [
            "DROP TABLE IF EXISTS GSQ_DEPT ;",
            "",
            "CREATE TABLE GSQ_DEPT (",
            "  DEPTNO CHAR(3) CCSID 37 NOT NULL ,",
            "  DEPTNAME VARCHAR(36) CCSID 37 NOT NULL ,",
            "  BUDGET DECIMAL(11, 2) DEFAULT 0 ,",
            &format!("  CONSTRAINT {pk} PRIMARY KEY( DEPTNO ) )  "),
            "  ;",
            "",
        ]
        .iter()
        .map(|l| rec(l))
        .collect::<Vec<_>>()
    );
    assert_eq!(
        db.run("CALL QSYS2.GENERATE_SQL('NOSUCH', 'GS', 'TABLE')")
            .0
            .sqlcode,
        462
    );
    assert_eq!(
        db.code("CALL QSYS2.GENERATE_SQL('GSQ_DEPT', 'GS', 'BOGUS')"),
        -443
    );
    assert_eq!(
        db.code("CALL QSYS2.GENERATE_SQL('X', 'GS', 'PROCEDURE')"),
        -84
    );
    assert_eq!(
        db.code("CALL QSYS2.GENERATE_SQL('GSQ_DEPT', 'GS', 'TABLE', TEMPORAL_OPTION => '1')"),
        -84
    );
}

#[test]
fn generate_sql_of_views_privileges_and_object_lists() {
    let mut db = Db::new(true);
    for st in [
        "CREATE SCHEMA GS",
        "CREATE TABLE GS.T1 (A INT NOT NULL, B VARCHAR(10))",
        "CREATE VIEW GS.V1 (A, B) AS SELECT A, B FROM GS.T1 WHERE A > 1",
        "CREATE TABLE GS.OBJS (OBJECT_SCHEMA VARCHAR(128), OBJECT_NAME VARCHAR(128), SQL_OBJECT_TYPE VARCHAR(10))",
        "INSERT INTO GS.OBJS VALUES ('GS', 'V1', 'VIEW'), ('GS', 'T1', 'TABLE')",
    ] {
        let (ca, _) = db.run(st);
        assert!(!ca.is_error(), "{st}: {}", ca.line());
    }
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('V1', 'GS', 'VIEW', HEADER_OPTION => '0', PRIVILEGES_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    assert_eq!(
        generated(&db),
        [
            "CREATE VIEW GS.V1 ( ",
            "\tA , ",
            "\tB ) ",
            "\tAS ",
            "\tSELECT A, B FROM GS.T1 WHERE A > 1   ",
            "\tRCDFMT T1         ; ",
            "  "
        ]
        .iter()
        .map(|l| rec(&format!("{l}\n")))
        .collect::<Vec<_>>()
    );
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL('T1', 'GS', 'TABLE', HEADER_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    let g = generated(&db);
    assert!(
        g.iter()
            .any(|l| l.starts_with("GRANT ") && l.contains("SELECT")),
        "{g:#?}"
    );
    let (ca, _) = db.run("CALL QSYS2.GENERATE_SQL_OBJECTS('OBJS', 'GS', HEADER_OPTION => '0', PRIVILEGES_OPTION => '0')");
    assert_eq!(ca.sqlcode, 466, "{}", ca.line());
    let g = generated(&db);
    assert!(g[0].starts_with("CREATE VIEW GS.V1"), "{g:#?}");
    assert!(
        g.iter().any(|l| l.starts_with("CREATE TABLE GS.T1")),
        "{g:#?}"
    );
    // QTEMP.OBJS: the job's QTEMP has no such table.
    assert_eq!(db.code("CALL QSYS2.GENERATE_SQL_OBJECTS('OBJS')"), -204);
}

#[test]
fn override_qaqqini() {
    let mut db = Db::new(true);
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'PARALLEL_DEGREE', '*MAX')"),
        -443
    );
    assert_eq!(db.code("CALL QSYS2.OVERRIDE_QAQQINI(1)"), 0);
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'PARALLEL_DEGREE', '*MAX')"),
        0
    );
    assert_eq!(db.ctx.qaqqini.get("PARALLEL_DEGREE"), "*MAX");
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'ALLOW_TEMPORARY_INDEXES', '*ONLY_REQUIRED')"),
        0
    );
    // The governor's limits are honoured (`plan::govern`); options that
    // change results are not yet.
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'QUERY_TIME_LIMIT', '30')"),
        0
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'SQL_DECFLOAT_WARNINGS', '*YES')"),
        -84
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'SQL_DECFLOAT_WARNINGS', '*DEFAULT')"),
        0
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'NO_SUCH_OPTION', '*YES')"),
        -443
    );
    assert_eq!(
        db.code("CALL QSYS2.OVERRIDE_QAQQINI(2, 'PARALLEL_DEGREE', '*SOMETIMES')"),
        -443
    );
    assert_eq!(db.code("CALL QSYS2.OVERRIDE_QAQQINI(3)"), 0);
    assert_eq!(db.ctx.qaqqini.get("PARALLEL_DEGREE"), "*DEFAULT");
    assert_eq!(db.code("CALL QSYS2.OVERRIDE_QAQQINI(4)"), -443);
}

#[test]
fn ifs_privileges_and_locks() {
    let mut db = Db::new(true);
    assert_eq!(
        db.rows("SELECT AUTHORIZATION_NAME, DATA_AUTHORITY, OBJECT_EXISTENCE, DATA_READ, DATA_ADD FROM TABLE(QSYS2.IFS_OBJECT_PRIVILEGES('/home/a.txt')) X"),
        vec![row(&["*PUBLIC", "*R", "NO", "YES", "NO"]), row(&["CODY", "*RWX", "YES", "YES", "YES"])]
    );
    assert_eq!(
        db.rows("SELECT COUNT(*) FROM TABLE(QSYS2.IFS_OBJECT_PRIVILEGES('/nope')) X"),
        vec![row(&["0"])]
    );
    assert_eq!(
        db.code(
            "SELECT * FROM TABLE(QSYS2.IFS_OBJECT_PRIVILEGES('/nope', IGNORE_ERRORS => 'NO')) X"
        ),
        -443
    );
    assert_eq!(
        db.rows("SELECT JOB_NAME, RO_COUNT, RW_COUNT, RO_SHARE_RW_COUNT, RW_SHARE_NONE_COUNT, SHARE_NONE_COUNT, SAVE_LOCK FROM TABLE(QSYS2.IFS_OBJECT_LOCK_INFO('/home/a.txt')) X"),
        vec![row(&["000009/CODY/QPADEV0001", "1", "1", "1", "1", "1", "NO"])]
    );
}
