// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One `QTEMP` per job: what CL makes (`CRTPF QTEMP/X`), what record I/O
//! opens and what SQL names are the same objects, in the job's
//! temporary root in ASP 1 that the storage service makes at the job's
//! start (`lic/slic/src/stgsvc/jobs.rs`).

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::odp::{self, Odp, OpenOpts, Usage};
use qsname::{LibSpec, Name};
use sql::{Naming, Outcome, SqlContext, prepare};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

struct Job {
    store: Store,
    qsys: ObjectId,
    qtemp: ObjectId,
    ctx: SqlContext,
}

impl Job {
    fn new() -> Self {
        let d = Arc::new(MemBlockDev::with_seed(16384, 12));
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
        // The job's start: its QTEMP, in the library list first.
        let qtemp =
            ops::create_temporary_root(&mut t, Name::parse("QTEMP").unwrap(), "QSECOFR", NOW)
                .unwrap();
        t.commit().unwrap();
        ops::set_job_qtemp(Some(qtemp));
        let mut ctx = SqlContext::new(
            qsys,
            JobHandle(1),
            NameContext {
                curlib: Some(lib),
                libl: vec![qtemp, lib],
            },
        );
        ctx.now = Some(NOW);
        ctx.user = "QSECOFR".into();
        ctx.qtemp.adopt(qtemp, NOW);
        Self {
            store,
            qsys,
            qtemp,
            ctx,
        }
    }

    fn ok(&mut self, text: &str) -> Outcome {
        let p = prepare(text, Naming::Sql).unwrap();
        let mut t = self.store.begin().unwrap();
        let r = p
            .execute(&mut t, &mut self.ctx, &[])
            .unwrap_or_else(|e| panic!("{text}: {e}"));
        t.commit().unwrap();
        r
    }

    fn rows(&mut self, text: &str) -> Vec<String> {
        match self.ok(text) {
            Outcome::Rows(rs) => rs
                .rows
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|v| v.to_string().trim_end().to_string())
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect(),
            o => panic!("{text}: {o:?}"),
        }
    }

    /// A file as record I/O resolves it: `LIB/FILE` or `*LIBL`.
    fn resolve(&mut self, lib: LibSpec, file: &str) -> ObjectId {
        let mut t = self.store.begin().unwrap();
        let f = ops::resolve(
            &mut t,
            self.qsys,
            &self.ctx.names,
            lib,
            Name::parse(file).unwrap(),
            types::FILE,
        )
        .unwrap();
        t.abort();
        f.id
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        ops::set_job_qtemp(None);
    }
}

#[test]
fn cl_rla_and_sql_share_the_jobs_qtemp() {
    let mut job = Job::new();
    let qtemp_name = Name::parse("QTEMP").unwrap();
    // CRTPF QTEMP/CLF RCDLEN(8): the library by name, as CL finds it.
    let mut t = job.store.begin().unwrap();
    let lib = ops::find_library(&mut t, job.qsys, qtemp_name).unwrap().id;
    assert_eq!(lib, job.qtemp);
    let clf = db::file::create_pf_rcdlen(
        &mut t,
        lib,
        Name::parse("CLF").unwrap(),
        8,
        &db::CreateFileOpts::default(),
    )
    .unwrap();
    t.commit().unwrap();
    // SQL sees it, qualified and through the library list.
    job.ok("INSERT INTO QTEMP.CLF VALUES ('fromsql')");
    assert_eq!(job.rows("SELECT COUNT(*) FROM CLF"), ["1"]);

    // An SQL table: record I/O opens it by QTEMP/ and *LIBL.
    job.ok("CREATE TABLE QTEMP.SQLT (A CHAR(4) NOT NULL)");
    let by_name = job.resolve(LibSpec::Name(qtemp_name), "SQLT");
    assert_eq!(by_name, job.resolve(LibSpec::Libl, "SQLT"));
    assert_eq!(job.resolve(LibSpec::Name(qtemp_name), "CLF"), clf);
    let locks = odp::new_shared();
    let mut t = job.store.begin().unwrap();
    let opts = OpenOpts {
        usage: Usage::ALL,
        keyed: false,
        ..OpenOpts::input(JobHandle(1))
    };
    let mut o = Odp::open(&mut t, locks, by_name, opts).unwrap();
    o.write(&mut t, &ccsid::to_ebcdic("rla1").unwrap(), &[])
        .unwrap();
    o.close(&mut t);
    t.commit().unwrap();
    assert_eq!(job.rows("SELECT A FROM QTEMP.SQLT"), ["rla1"]);

    // A view over both, in QTEMP.
    job.ok("CREATE VIEW QTEMP.BOTH AS SELECT A FROM QTEMP.SQLT UNION ALL SELECT SUBSTR(CLF, 1, 4) AS A FROM QTEMP.CLF");
    assert_eq!(
        job.rows("SELECT * FROM QTEMP.BOTH ORDER BY 1"),
        ["from", "rla1"]
    );

    // What DSPLIB QTEMP lists: every one of them.
    let mut t = job.store.begin().unwrap();
    let names: Vec<String> = ops::list(&mut t, job.qtemp, Some(types::FILE))
        .unwrap()
        .into_iter()
        .map(|(n, _)| n.name.to_string().trim().to_string())
        .collect();
    t.abort();
    for n in ["BOTH", "CLF", "SQLT"] {
        assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
    }

    // The job's end drops the library and all in it.
    let mut t = job.store.begin().unwrap();
    sql::exec::qtemp::drop_library(&mut t, job.qsys, JobHandle(1), job.qtemp).unwrap();
    t.commit().unwrap();
    let mut t = job.store.begin().unwrap();
    assert!(ops::read_header(&mut t, clf).is_err());
    assert!(ops::find_library(&mut t, job.qsys, qtemp_name).is_err());
    assert!(ops::temporary_roots(&mut t).unwrap().is_empty());
    t.abort();
}
