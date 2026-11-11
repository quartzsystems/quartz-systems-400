// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Members changed through the database, not the IFS: SMB leases on
//! their `/QSYS.LIB` files are broken and change notify hears them
//! (`qsyslib::notice`, `docs/NETSERVER.md` section 6).

#![allow(clippy::too_many_lines)]

use std::sync::Arc;
use std::time::Duration;

use blockdev::{BlockDev, MemBlockDev};
use db::CreateFileOpts;
use db::file::MemberOpt;
use db::odp::{Odp, OpenOpts, Usage, new_shared};
use ifs::backend::mem::MemBackend;
use ifs::lease::defer::{Deferral, deferring};
use ifs::lease::notify::{ChangeKind, DirEvent};
use ifs::lease::state::{H, R, RWH};
use ifs::ops::audit::{FileId, NoAudit};
use ifs::ops::flags::{O_RDWR, O_TRUNC};
use ifs::ops::{Attr, AttrId, Config, Ctx, Ifs, OpenArgs, Owner};
use qsname::{MemberSpec, Name};
use qsyslib::{MemAccess, Notice, StoreEnv, StoreQsys};
use sec::auth::principal::Principal;
use sec::profile::payload::UsrPrf;
use sec::profile::spcaut::SpecialAuth;
use sql::{Naming, SqlContext, prepare};
use store::obj::header::Authority;
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig, StoreError};

const NOW: u64 = 1_767_225_600;
const KEY: [u8; 32] = [7; 32];
/// The SMB job: owner 9, job 9.
const SMB: Owner = Owner(9);
const HELLO: &str = "/QSYS.LIB/QGPL.LIB/QCLSRC.FILE/HELLO.MBR";

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

/// `QGPL/QCLSRC(HELLO)`, `QGPL/CUST` (data, member `CUST`), `QSECOFR`.
fn store() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 9));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let qgpl = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap();
    let opts = CreateFileOpts {
        owner: "QSECOFR".into(),
        now: NOW,
        member: MemberOpt::Named(name("HELLO")),
        max_members: 0,
        ..CreateFileOpts::default()
    };
    let src = db::file::create_srcpf(&mut t, qgpl, name("QCLSRC"), 92, &opts).unwrap();
    db::source::srcput(
        &mut t,
        JobHandle(1),
        src,
        MemberSpec::Name(name("HELLO")),
        "PGM\nENDPGM\n",
        Some(name("CLLE")),
        NOW,
    )
    .unwrap();
    let opts = CreateFileOpts {
        member: MemberOpt::Named(name("CUST")),
        ..opts
    };
    db::file::create_pf_rcdlen(&mut t, qgpl, name("CUST"), 10, &opts).unwrap();
    let p = UsrPrf::new("QSECOFR", 40);
    sec::profile::objects::create(&mut t, qsys, &p, "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
    t.commit().unwrap();
    (store, qsys, qgpl)
}

fn secofr() -> Principal {
    Principal {
        user: "QSECOFR".into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        spcaut: SpecialAuth::ALLOBJ.0,
        ..Principal::default()
    }
}

/// An IFS over the store whose store tells it of database changes; the
/// IFS's calls are job 9's.
struct Sys {
    access: Arc<MemAccess>,
    ifs: Ifs<MemBackend>,
    qgpl: ObjectId,
}

impl Sys {
    fn new() -> Self {
        Self::as_job(9)
    }

    /// With the IFS's store calls made as `job`.
    fn as_job(job: u64) -> Self {
        let (s, qsys, qgpl) = store();
        let mut a = MemAccess::new(s, qsys);
        a.job = JobHandle(job);
        let access = Arc::new(a);
        let ifs = Ifs::new(
            MemBackend::new(),
            Box::new(StoreQsys::new(Arc::clone(&access))),
            Config::default(),
        );
        let notice = Notice::new(ifs.leases_shared());
        access
            .store
            .lock()
            .unwrap()
            .set_watcher(Some(Arc::new(notice)));
        let env = StoreEnv::new(&*access);
        ifs.ensure_layout(&env).unwrap();
        Self { access, ifs, qgpl }
    }

    /// Run `f` in one transaction as the database does.
    fn db<T>(&self, f: impl FnOnce(&mut Txn<'_>) -> T) -> T {
        let mut s = self.access.store.lock().unwrap();
        let mut t = s.begin().unwrap();
        let r = f(&mut t);
        t.commit().unwrap();
        r
    }

    fn file(&self, f: &str) -> ObjectId {
        let qgpl = self.qgpl;
        self.db(|t| {
            let l = store::obj::library::Library::load(t, qgpl).unwrap();
            l.lookup(t, types::FILE, name(f)).unwrap().unwrap()
        })
    }

    /// Run `f` with the SMB job's IFS context.
    fn smb<T>(&self, f: impl FnOnce(&Ifs<MemBackend>, &Ctx<'_>) -> T) -> T {
        let p = secofr();
        let env = StoreEnv::new(&*self.access);
        let cx = Ctx {
            owner: SMB,
            principal: &p,
            env: &env,
            audit: &NoAudit,
            job_ccsid: 37,
        };
        f(&self.ifs, &cx)
    }

    /// The SMB job asks for `want` on `path` as tag 5: what it got.
    fn lease(&self, path: &str, want: u8) -> u8 {
        self.smb(|ifs, cx| {
            let id: FileId = ifs
                .get_attrs(cx, path, &[AttrId::FileId], true)
                .unwrap()
                .into_iter()
                .find_map(|a| match a {
                    Attr::FileId(id) => Some(id),
                    _ => None,
                })
                .unwrap();
            ifs.lease_grant(cx, path, id, 5, KEY, want).unwrap()
        })
    }

    /// The SMB job's break orders and changes now.
    fn news(&self) -> ifs::lease::News {
        self.ifs.leases().orders(SMB, Duration::ZERO).unwrap()
    }

    /// The SMB job's client gave back all but `state`.
    fn ack(&self, state: u8) {
        self.ifs.leases().report(SMB, 5, &[(KEY, state)]);
    }

    fn held(&self, path: &str) -> u8 {
        let node = ifs::vfs::qsys::node_of(
            &path
                .trim_start_matches("/QSYS.LIB/")
                .split('/')
                .map(String::from)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        self.ifs
            .leases()
            .on(ifs::resolve::qsys_key(&node))
            .iter()
            .filter(|(o, _, _)| *o == SMB)
            .fold(0, |s, (_, _, b)| s | b)
    }
}

fn open(t: &mut Txn<'_>, file: ObjectId, usage: Usage) -> db::Result<Odp> {
    let mut opts = OpenOpts::input(JobHandle(1));
    opts.usage = usage;
    Odp::open(t, new_shared(), file, opts)
}

#[test]
fn a_record_written_breaks_the_lease_once_the_open_waited() {
    let s = Sys::new();
    let cust = s.file("CUST");
    let path = "/QSYS.LIB/QGPL.LIB/CUST.FILE/CUST.MBR";
    assert_eq!(s.lease(path, RWH), RWH);
    // The open for change waits for write caching to be given back.
    let (r, d) = deferring(true, || s.db(|t| open(t, cust, Usage::ALL).map(drop)));
    assert!(matches!(r, Err(db::DbError::Store(StoreError::Deferred))));
    let d = d.expect("deferred");
    assert!(matches!(d, Deferral::Breaks { .. }));
    assert_eq!(s.news().breaks, vec![(5, R | H)]);
    assert!(!s.ifs.leases().ready(&d));
    s.ack(R | H);
    assert!(s.ifs.leases().ready(&d));
    // Now it opens; its first record breaks the rest without waiting,
    // and later records order nothing more.
    let (r, d) = deferring(true, || {
        s.db(|t| {
            let mut o = open(t, cust, Usage::ALL)?;
            let nulls = vec![0; o.member().core.null_map_len()];
            o.write(t, &[0xC1; 10], &nulls)?;
            o.write(t, &[0xC2; 10], &nulls)?;
            o.close(t);
            Ok::<(), db::DbError>(())
        })
    });
    r.unwrap();
    assert!(d.is_none());
    assert_eq!(s.news().breaks, vec![(5, 0)]);
}

#[test]
fn no_write_caching_while_a_job_has_the_member_open_for_update() {
    let s = Sys::new();
    let cust = s.file("CUST");
    let path = "/QSYS.LIB/QGPL.LIB/CUST.FILE/CUST.MBR";
    let o = s.db(|t| open(t, cust, Usage::UPDATE)).unwrap();
    assert_eq!(s.lease(path, RWH), R | H);
    // Reading does not count.
    s.db(|t| o.close(t));
    s.ack(0);
    let o = s.db(|t| open(t, cust, Usage::INPUT)).unwrap();
    assert_eq!(s.lease(path, RWH), RWH);
    s.db(|t| o.close(t));
}

#[test]
fn source_put_clrpfm_and_rmvm_break_the_lease() {
    let s = Sys::new();
    let src = s.file("QCLSRC");
    // A source member rewritten (SEU, srcput): every lease broken.
    assert_eq!(s.lease(HELLO, R | H), R | H);
    s.db(|t| {
        db::source::srcput(
            t,
            JobHandle(1),
            src,
            MemberSpec::Name(name("HELLO")),
            "PGM\nRETURN\nENDPGM\n",
            None,
            NOW,
        )
        .unwrap()
    });
    assert_eq!(s.news().breaks, vec![(5, 0)]);
    s.ack(0);
    // CLRPFM.
    assert_eq!(s.lease(HELLO, R | H), R | H);
    s.db(|t| db::util::clrpfm(t, JobHandle(1), src, MemberSpec::Name(name("HELLO"))).unwrap());
    assert_eq!(s.news().breaks, vec![(5, 0)]);
    s.ack(0);
    // RMVM breaks handle caching, as an unlink does.
    assert_eq!(s.lease(HELLO, R | H), R | H);
    s.db(|t| db::file::remove_member(t, JobHandle(1), src, name("HELLO")).unwrap());
    assert_eq!(s.news().breaks, vec![(5, R)]);
}

#[test]
fn sql_update_breaks_the_lease() {
    let s = Sys::new();
    let qgpl = s.qgpl;
    let qsys = s.access.qsys;
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: Some(qgpl),
            libl: vec![qgpl],
        },
    );
    ctx.now = Some(NOW);
    let mut run = |text: &str| {
        let p = prepare(text, Naming::Sys).unwrap();
        s.db(|t| p.execute(t, &mut ctx, &[]).unwrap());
    };
    run("CREATE TABLE QGPL/T (A INT)");
    run("INSERT INTO QGPL/T VALUES (1)");
    let path = "/QSYS.LIB/QGPL.LIB/T.FILE/T.MBR";
    assert_eq!(s.lease(path, RWH), RWH);
    run("UPDATE QGPL/T SET A = 2");
    let news = s.news();
    assert_eq!(news.breaks, vec![(5, 0)], "{news:?}");
}

#[test]
fn the_ifs_own_member_writes_are_not_told_twice() {
    // The store calls of the IFS are another job's than its owner.
    let s = Sys::as_job(8);
    assert_eq!(s.lease(HELLO, RWH), RWH);
    // The holder's own write through `/QSYS.LIB` keeps its lease.
    s.smb(|ifs, cx| {
        let fd = ifs
            .open(cx, HELLO, OpenArgs::new(O_RDWR | O_TRUNC))
            .unwrap();
        ifs.write(cx, fd, &[0x40; 80]).unwrap();
        ifs.close(cx, fd).unwrap();
    });
    assert!(s.news().breaks.is_empty());
    assert_eq!(s.held(HELLO), RWH);
}

fn event(n: &str, kind: ChangeKind, dir: bool) -> DirEvent {
    DirEvent {
        name: n.into(),
        kind,
        dir,
    }
}

#[test]
fn watches_hear_members_and_files_come_and_go() {
    let s = Sys::new();
    let src = s.file("QCLSRC");
    let (wf, wl) = s.smb(|ifs, cx| {
        (
            ifs.watch_add(cx, "/qsys.lib/qgpl.lib/qclsrc.file", false)
                .unwrap(),
            ifs.watch_add(cx, "/QSYS.LIB/QGPL.LIB", false).unwrap(),
        )
    });
    // ADDPFM, records written (one Modified until polled), RNMM, RMVM.
    s.db(|t| {
        db::file::add_member(t, src, name("NEW"), "", None, NOW).unwrap();
        db::source::srcput(
            t,
            JobHandle(1),
            src,
            MemberSpec::Name(name("NEW")),
            "A\nB\nC\n",
            None,
            NOW,
        )
        .unwrap();
        db::file::rename_member(t, src, name("NEW"), name("NEWER"), NOW).unwrap();
        db::file::remove_member(t, JobHandle(1), src, name("NEWER")).unwrap();
    });
    let news = s.news();
    assert_eq!(
        news.changes,
        vec![(
            wf,
            vec![
                event("NEW.MBR", ChangeKind::Added, false),
                event("NEW.MBR", ChangeKind::Modified, false),
                event(
                    "NEWER.MBR",
                    ChangeKind::Renamed {
                        from: "NEW.MBR".into()
                    },
                    false
                ),
                event("NEWER.MBR", ChangeKind::Removed, false),
            ]
        )]
    );
    // A file made and deleted (CRTPF, DLTF) in the library's directory;
    // the file's own watch ends with it.
    let qgpl = s.qgpl;
    let opts = CreateFileOpts {
        owner: "QSECOFR".into(),
        now: NOW,
        ..CreateFileOpts::default()
    };
    let f = s.db(|t| db::file::create_pf_rcdlen(t, qgpl, name("NEWF"), 10, &opts).unwrap());
    s.db(|t| db::file::delete_file(t, JobHandle(1), src).unwrap());
    s.db(|t| db::file::delete_file(t, JobHandle(1), f).unwrap());
    let news = s.news();
    assert_eq!(
        news.changes,
        vec![
            (wf, vec![event("", ChangeKind::Deleted, true),]),
            (
                wl,
                vec![
                    event("NEWF.FILE", ChangeKind::Added, true),
                    event("QCLSRC.FILE", ChangeKind::Removed, true),
                    event("NEWF.FILE", ChangeKind::Removed, true),
                ]
            ),
        ],
    );
}
