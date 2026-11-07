// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Long commands take turns: each runs its work on a fiber as the serve
//! loop runs it (`cpp::longcmd::Work`, yielding through
//! `fiber::suspend`), over a store in memory, while at every turn
//! another job's query, catalog list and command answer. No turn holds
//! the others past the bound, the results are those of a run without
//! turns, the locks the rules give hold throughout, and a command ended
//! part way leaves what its rules say.

#![allow(clippy::too_many_lines)]

use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use blockdev::{BlockDev, MemBlockDev};
use qsname::{MemberSpec, Name};
use sql::{Naming, RunOpts, SqlContext};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

use super::fiber::{self, Fiber};
use crate::cpp::longcmd::Work;
use crate::svc::db::{Params, files};

const NOW: u64 = 1_767_225_600;
const ROWS: i64 = 3000;

/// No other job waits past this, debug build included.
const BOUND: Duration = Duration::from_millis(250);

struct Fx {
    store: Rc<RefCell<Store>>,
    qsys: ObjectId,
    lib: ObjectId,
    names: NameContext,
}

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

/// DDS in its columns.
fn dds(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

/// `T` with `ROWS` rows, `K` with 100, and a source file whose members
/// describe logical files over `T`.
fn fixture() -> Fx {
    let d = Arc::new(MemBlockDev::with_seed(65536, 21));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
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
            name: name("QSTEST"),
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
    let src = db::file::create_srcpf(
        &mut t,
        lib,
        name("QDDSSRC"),
        92,
        &db::CreateFileOpts {
            now: NOW,
            max_members: 0,
            member: db::file::MemberOpt::None,
            ..db::CreateFileOpts::default()
        },
    )
    .unwrap();
    for (mbr, unique) in [("TBYNAME", false), ("TUNIQUE", true)] {
        let mut text = String::new();
        if unique {
            let _ = writeln!(text, "{}", dds(&[(6, "A"), (45, "UNIQUE")]));
        }
        let _ = writeln!(
            text,
            "{}",
            dds(&[(6, "A"), (17, "R"), (19, "T"), (45, "PFILE(QSTEST/T)")])
        );
        let _ = writeln!(text, "{}", dds(&[(6, "A"), (17, "K"), (19, "NAME")]));
        db::file::add_member(&mut t, src, name(mbr), "", Some(name("LF")), NOW).unwrap();
        db::source::srcput(
            &mut t,
            JobHandle(9),
            src,
            MemberSpec::Name(name(mbr)),
            &text,
            Some(name("LF")),
            NOW,
        )
        .unwrap();
    }
    t.commit().unwrap();
    let names = NameContext {
        curlib: Some(lib),
        libl: vec![lib],
    };
    let fx = Fx {
        store: Rc::new(RefCell::new(store)),
        qsys,
        lib,
        names,
    };
    let mut text = String::from(
        "CREATE TABLE QSTEST.T (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);
         CREATE TABLE QSTEST.K (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);
         INSERT INTO QSTEST.K VALUES (1, 'K');",
    );
    for chunk in 0..ROWS / 100 {
        let _ = write!(text, "INSERT INTO QSTEST.T VALUES ");
        for i in 0..100 {
            let id = chunk * 100 + i + 1;
            let _ = write!(
                text,
                "{}({id}, 'Row {:05}')",
                if i > 0 { ", " } else { "" },
                ROWS - id
            );
        }
        text.push(';');
    }
    script(&fx, 2, &text).unwrap();
    fx
}

fn ctx(fx: &Fx, job: u64) -> SqlContext {
    let mut c = SqlContext::new(fx.qsys, JobHandle(job), fx.names.clone());
    c.now = Some(NOW);
    c.naming = Naming::Sql;
    c
}

/// A script run as job `job`; the listing on errors.
fn script(fx: &Fx, job: u64, text: &str) -> Result<(), String> {
    let opts = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let mut c = ctx(fx, job);
    let mut st = fx.store.borrow_mut();
    let r = sql::runsqlstm(&mut st, &mut c, text, &opts).map_err(|e| e.to_string())?;
    if r.errors == 0 {
        Ok(())
    } else {
        Err(r.listing)
    }
}

/// A query's rows as text, as job 2.
fn rows(fx: &Fx, text: &str) -> Vec<String> {
    let mut c = ctx(fx, 2);
    let mut st = fx.store.borrow_mut();
    let mut txn = st.begin().unwrap();
    let out = match sql::prepare(text, Naming::Sql)
        .unwrap()
        .execute(&mut txn, &mut c, &[])
        .unwrap()
    {
        sql::Outcome::Rows(rs) => rs.rows.iter().map(|r| format!("{r:?}")).collect(),
        o => panic!("{o:?}"),
    };
    txn.commit().unwrap();
    out
}

fn count(fx: &Fx, text: &str) -> usize {
    rows(fx, text).len()
}

/// A command's console word run as job `job` in one transaction.
fn command(
    fx: &Fx,
    job: u64,
    f: impl FnOnce(&mut store::Txn<'_>, &SqlContext, JobHandle) -> Result<String, String>,
) -> Result<String, String> {
    let c = ctx(fx, job);
    let mut st = fx.store.borrow_mut();
    let mut txn = st.begin().unwrap();
    let r = f(&mut txn, &c, JobHandle(job));
    if r.is_ok() {
        txn.commit().unwrap();
    }
    r
}

/// What the other jobs did at one turn.
#[derive(Debug, Default)]
struct Turn {
    /// Job 3's change of `T`, which the rules may refuse.
    changed_t: bool,
}

/// Run `work` on a fiber as the serve loop does, a turn at every pause,
/// ended at turn `stop` (never for `u32::MAX`). At each turn job 2 queries `K`,
/// lists the catalog and runs a command, and job 3 tries `T`.
fn drive<R: 'static>(
    fx: &Fx,
    stop: u32,
    work: impl FnOnce(&Work<'_>) -> R + 'static,
) -> (R, Duration, Vec<Turn>) {
    let out = Rc::new(RefCell::new(None));
    let put = Rc::clone(&out);
    let lender = Rc::clone(&fx.store);
    let seen = Rc::new(Cell::new(0u32));
    let body = move || {
        let w = Work::new(
            &lender,
            Rc::new(move || {
                fiber::suspend();
                seen.set(seen.get() + 1);
                seen.get() >= stop
            }),
        )
        .every(Duration::ZERO);
        *put.borrow_mut() = Some(work(&w));
    };
    let mut f = Fiber::on(fiber::stack().unwrap(), body);
    let mut held = Duration::ZERO;
    let mut log = Vec::new();
    loop {
        let at = Instant::now();
        let ended = f.resume();
        held = held.max(at.elapsed());
        if ended {
            break;
        }
        // The other jobs, answered in the turn.
        assert_eq!(count(fx, "SELECT * FROM QSTEST.K"), 1);
        assert!(
            count(
                fx,
                "SELECT TABLE_NAME FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = 'QSTEST'"
            ) >= 2
        );
        command(fx, 2, |t, c, j| {
            files::cpyf_in(
                t,
                c,
                j,
                &Params::parse("cpyf QSTEST/K QSTEST/K2 MBROPT(*REPLACE) CRTFILE(*YES)"),
                &mut db::no_pause,
            )
        })
        .unwrap();
        log.push(Turn {
            changed_t: script(fx, 3, "UPDATE QSTEST.T SET NAME = NAME WHERE ID = 1").is_ok(),
        });
    }
    f.recycle();
    let r = out.borrow_mut().take().expect("the work ended");
    (r, held, log)
}

#[test]
fn a_copy_takes_turns() {
    let fx = fixture();
    let line = Params::parse("cpyf FROMFILE(QSTEST/T) TOFILE(QSTEST/C) CRTFILE(*YES)");
    let ctx1 = ctx(&fx, 1);
    let (r, held, turns) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::cpyf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    let text = r.unwrap();
    assert!(
        text.starts_with(&format!("{ROWS} records copied")),
        "{text}"
    );
    assert!(turns.len() >= 5, "{} turns", turns.len());
    assert!(held < BOUND, "others waited {held:?}");
    // The from-file is read, and changed, by others meanwhile.
    assert!(turns.iter().all(|t| t.changed_t), "{turns:?}");
    // Every record, in order, as a copy without turns gives.
    assert_eq!(
        rows(&fx, "SELECT ID, NAME FROM QSTEST.C ORDER BY ID"),
        rows(&fx, "SELECT ID, NAME FROM QSTEST.T ORDER BY ID")
    );

    // Ended part way: the records copied so far stay, as when an error
    // ends a copy; the to-file is free again.
    let line = Params::parse("cpyf FROMFILE(QSTEST/T) TOFILE(QSTEST/D) CRTFILE(*YES)");
    let ctx1 = ctx(&fx, 1);
    let (r, _, _) = drive(&fx, 3, move |w| {
        w.txn(|t, pause| files::cpyf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    assert!(r.is_err(), "{r:?}");
    let n = count(&fx, "SELECT ID FROM QSTEST.D");
    assert!(
        n > 0 && n < usize::try_from(ROWS).unwrap(),
        "{n} copied before it ended"
    );
    script(&fx, 3, "DELETE FROM QSTEST.D").unwrap();
}

#[test]
fn a_logical_file_is_built_taking_turns() {
    let fx = fixture();
    let line = Params::parse("crtlf FILE(QSTEST/TBYNAME) SRCFILE(QSTEST/QDDSSRC)");
    let ctx1 = ctx(&fx, 1);
    let (r, held, turns) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::crtlf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    assert!(r.is_ok(), "{r:?}");
    assert!(turns.len() >= 3, "{} turns", turns.len());
    assert!(held < BOUND, "others waited {held:?}");
    // The based-on table is read by others, never changed, while it is built.
    assert!(turns.iter().all(|t| !t.changed_t), "{turns:?}");
    assert_eq!(
        count(&fx, "SELECT * FROM QSTEST.TBYNAME"),
        usize::try_from(ROWS).unwrap()
    );
    assert!(script(&fx, 3, "UPDATE QSTEST.T SET NAME = NAME WHERE ID = 1").is_ok());

    // UNIQUE over repeated keys: refused, nothing left.
    script(&fx, 3, "INSERT INTO QSTEST.T VALUES (9999, 'Row 00007')").unwrap();
    let line = Params::parse("crtlf FILE(QSTEST/TUNIQUE) SRCFILE(QSTEST/QDDSSRC)");
    let ctx1 = ctx(&fx, 1);
    let (r, _, _) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::crtlf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    let e = r.unwrap_err();
    assert!(e.starts_with("CPF5090"), "{e}");
    assert!(
        script(&fx, 3, "SELECT * FROM QSTEST.TUNIQUE").is_err(),
        "the file is gone"
    );

    // Ended part way: nothing of it stays.
    let line = Params::parse("crtlf FILE(QSTEST/TBYNAME2) SRCFILE(QSTEST/QDDSSRC) SRCMBR(TBYNAME)");
    let ctx1 = ctx(&fx, 1);
    let (r, _, _) = drive(&fx, 2, move |w| {
        w.txn(|t, pause| files::crtlf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    assert!(r.is_err(), "{r:?}");
    assert!(
        script(&fx, 3, "SELECT * FROM QSTEST.TBYNAME2").is_err(),
        "the file is gone"
    );
    assert!(script(&fx, 3, "UPDATE QSTEST.T SET NAME = NAME WHERE ID = 1").is_ok());
}

#[test]
fn a_script_reorganize_and_text_copies_take_turns() {
    let fx = fixture();
    let text = "CREATE TABLE QSTEST.S (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL);
                INSERT INTO QSTEST.S SELECT ID, NAME FROM QSTEST.T;
                DELETE FROM QSTEST.S WHERE MOD(ID, 3) = 0;
                UPDATE QSTEST.S SET NAME = 'Odd' WHERE MOD(ID, 2) = 1";
    // Without turns, for the result.
    let plain = fixture();
    script(&plain, 1, text).unwrap();
    let want = rows(&plain, "SELECT ID, NAME FROM QSTEST.S ORDER BY ID");

    let mut c = ctx(&fx, 1);
    let (r, held, turns) = drive(&fx, u32::MAX, move |w| {
        c.yielder = Some(w.yielder());
        let opts = RunOpts {
            naming: Naming::Sql,
            ..RunOpts::default()
        };
        sql::runsqlstm_lent(w.lender(), &mut c, text, &opts).map_err(|e| e.to_string())
    });
    assert_eq!(r.unwrap().errors, 0);
    assert!(turns.len() >= 5, "{} turns", turns.len());
    assert!(held < BOUND, "others waited {held:?}");
    assert_eq!(rows(&fx, "SELECT ID, NAME FROM QSTEST.S ORDER BY ID"), want);

    // RGZPFM: the deleted rows squeezed out, the member held throughout.
    let line = Params::parse("rgzpfm FILE(QSTEST/S)");
    let ctx1 = ctx(&fx, 1);
    let (r, held, _) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::rgzpfm_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    assert!(r.unwrap().contains(&format!("{} records kept", want.len())));
    assert!(held < BOUND, "others waited {held:?}");
    assert_eq!(rows(&fx, "SELECT ID, NAME FROM QSTEST.S ORDER BY ID"), want);

    // Out as text and back in, taking turns both ways.
    let line = Params::parse("cpytoimpf FROMFILE(QSTEST/S) TOSTMF(BUF)");
    let ctx1 = ctx(&fx, 1);
    let (r, held, turns) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::cpytoimpf_in(t, &ctx1, JobHandle(1), &line, pause))
    });
    let csv = r.unwrap();
    assert!(
        turns.len() >= 3 && held < BOUND,
        "{} turns, {held:?}",
        turns.len()
    );
    script(
        &fx,
        3,
        "CREATE TABLE QSTEST.S2 (ID INT NOT NULL, NAME VARCHAR(20) NOT NULL)",
    )
    .unwrap();
    let line = Params::parse("cpyfrmimpf FROMSTMF(BUF) TOFILE(QSTEST/S2)");
    let ctx1 = ctx(&fx, 1);
    let (r, held, turns) = drive(&fx, u32::MAX, move |w| {
        w.txn(|t, pause| files::cpyfrmimpf_in(t, &ctx1, JobHandle(1), &line, &csv, pause))
    });
    assert!(
        r.unwrap()
            .starts_with(&format!("{} records copied, 0", want.len()))
    );
    assert!(
        turns.len() >= 3 && held < BOUND,
        "{} turns, {held:?}",
        turns.len()
    );
    assert_eq!(
        rows(&fx, "SELECT ID, NAME FROM QSTEST.S2 ORDER BY ID"),
        want
    );
    let _ = fx.lib;
}
