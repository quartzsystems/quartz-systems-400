// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Members changed through the database, not the IFS: a client's lease on
//! a member breaks and its watch hears them (`qsyslib::notice`), over a
//! `/QSYS.LIB` that is a store in memory with the watcher installed.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::CreateFileOpts;
use db::file::MemberOpt;
use db::odp::{Odp, OpenOpts, Usage, new_shared};
use qsname::{MemberSpec, Name};
use qsyslib::{MemAccess, Notice, StoreQsys};
use sec::profile::payload::UsrPrf;
use store::obj::header::Authority;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::txn::Txn;
use store::{FormatParams, Store, StoreConfig};

use super::*;

const NOW: u64 = 1_767_225_600;
/// The database job: none of the `NetServer` jobs' owners.
const DBJOB: JobHandle = JobHandle(500);
const HELLO: &str = r"QGPL.LIB\QCLSRC.FILE\HELLO.MBR";

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

/// A store with `QGPL/QCLSRC(HELLO)` owned by `ALICE`; the IFS's calls
/// to it are job 77's.
fn store() -> (Arc<MemAccess>, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 3));
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
            owner: "ALICE".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap();
    let opts = CreateFileOpts {
        owner: "ALICE".into(),
        now: NOW,
        member: MemberOpt::Named(name("HELLO")),
        max_members: 0,
        ..CreateFileOpts::default()
    };
    let src = db::file::create_srcpf(&mut t, qgpl, name("QCLSRC"), 92, &opts).unwrap();
    db::source::srcput(
        &mut t,
        DBJOB,
        src,
        MemberSpec::Name(name("HELLO")),
        "PGM\nENDPGM\n",
        None,
        NOW,
    )
    .unwrap();
    for u in ["ALICE", "QSECOFR"] {
        let p = UsrPrf::new(u, 40);
        sec::profile::objects::create(&mut t, qsys, &p, "QSECOFR", Authority::EXCLUDE, NOW)
            .unwrap();
    }
    t.commit().unwrap();
    let mut a = MemAccess::new(store, qsys);
    a.job = JobHandle(77);
    (Arc::new(a), src)
}

/// The IFS over the store, which tells it of database changes.
fn fs_with_store() -> (Arc<Fs>, Arc<MemAccess>, ObjectId) {
    let (access, src) = store();
    let fs = Fs::over(Box::new(StoreQsys::new(Arc::clone(&access))));
    let notice = Notice::new(fs.ifs.leases_shared());
    access
        .store
        .lock()
        .unwrap()
        .set_watcher(Some(Arc::new(notice)));
    (fs, access, src)
}

/// Run `f` in one transaction of the store on a thread of its own, as a
/// database job does.
fn db<T: Send + 'static>(
    access: &Arc<MemAccess>,
    f: impl FnOnce(&mut Txn<'_>) -> T + Send + 'static,
) -> std::thread::JoinHandle<T> {
    let a = Arc::clone(access);
    std::thread::spawn(move || db_here(&a, f))
}

/// Run `f` in one transaction of the store on this thread.
fn db_here<T>(access: &MemAccess, f: impl FnOnce(&mut Txn<'_>) -> T) -> T {
    let mut s = access.store.lock().unwrap();
    let mut t = s.begin().unwrap();
    let r = f(&mut t);
    t.commit().unwrap();
    r
}

/// `srcput`, as SEU saves a member: the client's lease on it breaks.
#[tokio::test(flavor = "multi_thread")]
async fn a_database_write_breaks_a_member_lease() {
    let (fs, access, src) = fs_with_store();
    let a = job_as(&fs, "ALICE", true);
    let mut c = alice(a.addr, "QSYS").await;
    let r = leased_open(&mut c, HELLO, [4; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH);
    let w = db(&access, move |t| {
        db::source::srcput(
            t,
            DBJOB,
            src,
            MemberSpec::Name(name("HELLO")),
            "PGM\nRETURN\nENDPGM\n",
            None,
            NOW,
        )
        .unwrap();
    });
    let n = c
        .next_break(BREAK_WAIT)
        .await
        .expect("a break for the database write");
    assert_eq!((n.current_lease_state, n.new_lease_state), (RWH, 0));
    assert_eq!(c.ack([4; 16], 0).await, STATUS_SUCCESS);
    w.join().unwrap();
    c.close(r.file_id).await;
    // A new open reads the database's records.
    let id = c.open(HELLO, FILE_GENERIC_READ, SHARE_ALL).await.unwrap();
    let data = c.read(id, 0, 400).await.unwrap();
    assert!(
        ccsid::from_ebcdic(&data).contains("RETURN"),
        "{:?}",
        ccsid::from_ebcdic(&data)
    );
    c.close(id).await;
}

/// While a database job has the member open for update, a lease caches
/// no writes.
#[tokio::test(flavor = "multi_thread")]
async fn a_member_lease_caches_no_writes_while_a_job_has_it_open_for_update() {
    let (fs, access, src) = fs_with_store();
    let a = job_as(&fs, "ALICE", true);
    let odp = db_here(&access, |t| {
        let mut opts = OpenOpts::input(DBJOB);
        opts.usage = Usage::UPDATE;
        Odp::open(t, new_shared(), src, opts).unwrap()
    });
    let mut c = alice(a.addr, "QSYS").await;
    let r = leased_open(&mut c, HELLO, [5; 16], RWH, false).await;
    assert_eq!(granted(&r), RWH & !smbc::lease::LEASE_WRITE_CACHING);
    c.close(r.file_id).await;
    db_here(&access, |t| odp.close(t));
}

/// `ADDPFM` and `RMVM` are heard in the file's directory.
#[tokio::test(flavor = "multi_thread")]
async fn a_watch_on_a_file_hears_members_added_and_removed() {
    const REMOVED: u32 = 2;
    let (fs, access, src) = fs_with_store();
    let a = job_as(&fs, "ALICE", true);
    let mut c = alice(a.addr, "QSYS").await;
    let dir = c.open_dir(r"QGPL.LIB\QCLSRC.FILE").await;
    let mid = arm(&mut c, dir, false).await;
    db(&access, move |t| {
        db::file::add_member(t, src, name("NEW"), "", None, NOW).unwrap();
    })
    .join()
    .unwrap();
    assert_eq!(notified(&mut c, mid).await, [(ADDED, "NEW.MBR".to_owned())]);
    let mid = arm(&mut c, dir, false).await;
    db(&access, move |t| {
        db::file::remove_member(t, DBJOB, src, name("NEW")).unwrap();
    })
    .join()
    .unwrap();
    assert_eq!(
        notified(&mut c, mid).await,
        [(REMOVED, "NEW.MBR".to_owned())]
    );
}
