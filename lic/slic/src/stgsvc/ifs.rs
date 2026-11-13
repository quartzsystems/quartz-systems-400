// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The integrated file system in the storage service: one
//! `Ifs<LinuxBackend>` over the xfs `slic` mounted at [`ROOT`], built
//! when the service starts. `/QSYS.LIB` is the store, the profile
//! environment the store's profiles, and audit events become
//! `QAUDJRN` entries the request's flush writes. Each IFS call runs
//! with the store lent ([`Lent`] reaches it) and outside the store's
//! guard: the IFS decides authority itself, once. When the xfs is not
//! mounted every request answers `ENOTSUP`. A job's descriptors, streams
//! and locks are its connection's and go when it ends ([`release`]).
//! `/QOPT` shows the optical volumes of the loaded image catalogs
//! (`super::media`). Leases `QZLSFILE` and `QNFSNFSD` jobs hold are the
//! `Ifs`'s (`ifs::lease`): a request that has to wait for another job's
//! break, and a server job's poll for news, are parked by the loop
//! (`super::turns`) and answered again once ready or out of time.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;

use httpadmin::seed::{REGISTRY, WWW, WWW_OWNER, WWW_PUBLIC};
use ifs::Errno;
use ifs::backend::linux::LinuxBackend;
use ifs::client::{Direct, IfsClient, Unavailable};
use ifs::lease::defer::Deferral;
use ifs::ops::{Config, Ctx, Ifs, Owner};
use qsyslib::access::store_errno;
use qsyslib::{JournalAudit, StoreAccess, StoreEnv, StoreQsys};
use sec::auth::principal::Principal;
use stg::ifs::{IfsOp, IfsReply};
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};
use crate::svc::ile::{self, Shared};

/// Where `slic` mounts the IFS root's xfs.
pub const ROOT: &str = "/run/qs400/ifs";

/// The file system, or why there is none.
static IFS: OnceLock<Result<Ifs<LinuxBackend>, String>> = OnceLock::new();

thread_local! {
    /// The store lent for the call in progress, and the job it is for.
    /// The service answers one request at a time.
    static LENT: RefCell<Option<(Shared, JobHandle)>> = const { RefCell::new(None) };
    /// The request being answered may be parked ([`parking`]).
    static PARKING: Cell<bool> = const { Cell::new(false) };
    /// What the last job request deferred on.
    static DEFERRED: Cell<Option<Deferral>> = const { Cell::new(None) };
}

/// The store as the IFS reaches it: whatever is lent for the call in
/// progress.
#[derive(Debug, Clone, Copy, Default)]
pub struct Lent;

impl Lent {
    fn current() -> Option<(Shared, JobHandle)> {
        LENT.with(|l| l.borrow().clone())
    }
}

impl StoreAccess for Lent {
    fn with_main<T>(
        &self,
        f: &mut dyn FnMut(&mut Txn<'_>, ObjectId) -> Result<T, Errno>,
    ) -> Result<T, Errno> {
        let (shared, _) = Self::current().ok_or(Errno::EIO)?;
        let mut b = shared.borrow_mut();
        let lent = b.as_mut().ok_or(Errno::EIO)?;
        let qsys = lent.storage.qsys.ok_or(Errno::EIO)?;
        let mut txn = lent.storage.store.begin().map_err(|e| store_errno(&e))?;
        match f(&mut txn, qsys) {
            Ok(v) => {
                txn.commit().map_err(|e| store_errno(&e))?;
                Ok(v)
            }
            Err(e) => {
                txn.abort();
                Err(e)
            }
        }
    }

    fn job(&self) -> JobHandle {
        Self::current().map_or(JobHandle(0), |(_, j)| j)
    }

    fn sysval(&self, name: &str) -> Option<String> {
        let (shared, _) = Self::current()?;
        let b = shared.borrow();
        let v = b.as_ref()?.sysvals.as_ref()?.get(name).ok()?;
        Some(v.text())
    }

    fn now(&self) -> u64 {
        now_unix()
    }

    fn env_cache(&self) -> Option<&qsyslib::env::EnvCache> {
        Some(&super::ifscaller::CACHE)
    }
}

/// Run `f` with the store lent for `job` and the guard off.
fn with_lent<T>(system: &mut System, job: JobHandle, f: impl FnOnce() -> T) -> T {
    let lent = ile::lend(system).is_ok();
    LENT.with(|l| *l.borrow_mut() = Some((Rc::clone(&system.lent), job)));
    let r = sec::guard::as_system(f);
    LENT.with(|l| *l.borrow_mut() = None);
    if lent {
        ile::restore(system);
    }
    r
}

/// Whether `path` is a mount point (`/proc/self/mountinfo`).
fn mounted(path: &str) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo")
        .is_ok_and(|t| t.lines().any(|l| l.split_whitespace().nth(4) == Some(path)))
}

/// Build the IFS at service start: the standard directories with
/// IBM's owners and authorities where they are missing. A line for
/// the start message.
pub fn start(system: &mut System) -> String {
    let r = build(system);
    let line = match &r {
        Ok(_) => format!("IFS root at {ROOT}"),
        Err(e) => format!("IFS not available: {e}"),
    };
    if let Err(e) = &r {
        eprintln!("qsstg: {e}");
    }
    let _ = IFS.set(r);
    if let Some(ifs) = get() {
        keep_registry(system);
        keep_notices(system);
        keep_downloads(system);
        super::nfs::ship_etc(system);
        // Members, files and libraries changed through the database break
        // leases and are heard by watches (`qsyslib::notice`).
        if let Some(s) = system.storage.as_mut() {
            let notice = qsyslib::Notice::new(ifs.leases_shared());
            s.store.set_watcher(Some(std::sync::Arc::new(notice)));
        }
    }
    line
}

/// Access for Web's client pieces (`osobjects::afwdl`), rewritten when
/// they differ.
fn keep_downloads(system: &mut System) {
    let p = Principal {
        user: "QSYS".into(),
        spcaut: sec::profile::SpecialAuth::ALL.0,
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    with_client(system, 0, console_owner(0), &p, 37, |c| {
        for (name, data) in osobjects::afwdl::FILES {
            let path = format!("{}/{name}", osobjects::afwdl::DIR);
            let same = c
                .open(&path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY))
                .and_then(|fd| {
                    let r = ifs::client::read_all(c, fd);
                    let _ = c.close(fd);
                    r
                })
                .is_ok_and(|old| old == *data);
            if !same && let Err(e) = ifs::client::replace(c, &path, data, 0o644) {
                eprintln!("qsstg: {path}: {}", e.text());
            }
        }
    });
}

/// The third-party notices this release ships, at
/// `/QQS/ProdData/Notices/NOTICES.txt`, rewritten when they differ.
fn keep_notices(system: &mut System) {
    let p = Principal {
        user: "QSYS".into(),
        spcaut: sec::profile::SpecialAuth::ALL.0,
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    let path = osobjects::notices::PATH;
    let text = osobjects::notices::TEXT.as_bytes();
    with_client(system, 0, console_owner(0), &p, 37, |c| {
        let same = c
            .open(path, ifs::ops::OpenArgs::new(ifs::ops::flags::O_RDONLY))
            .and_then(|fd| {
                let r = ifs::client::read_all(c, fd);
                let _ = c.close(fd);
                r
            })
            .is_ok_and(|old| old == text);
        if !same && let Err(e) = ifs::client::replace(c, path, text, 0o644) {
            eprintln!("qsstg: {path}: {}", e.text());
        }
    });
}

/// The HTTP instance registry is the `ADMIN` job's: one another
/// profile took over by renaming over it (`ENDTCPSVR` did, before
/// replaced files kept their owners) is given back.
fn keep_registry(system: &mut System) {
    let p = Principal {
        user: "QSYS".into(),
        spcaut: sec::profile::SpecialAuth::ALL.0,
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    let path = REGISTRY;
    with_client(system, 0, console_owner(0), &p, 37, |c| {
        let Ok(a) = c.dspaut(path) else { return };
        if a.owner == WWW_OWNER {
            return;
        }
        match c.chgown(path, WWW_OWNER, true) {
            Ok(()) => eprintln!("qsstg: {path} given back to {WWW_OWNER} from {}", a.owner),
            Err(e) => eprintln!("qsstg: {path}: owner {}: {}", a.owner, e.text()),
        }
    });
}

fn build(system: &mut System) -> Result<Ifs<LinuxBackend>, String> {
    if !mounted(ROOT) {
        return Err(format!("{ROOT} is not mounted"));
    }
    let b = LinuxBackend::open(ROOT).map_err(|e| format!("{ROOT}: {}", e.text()))?;
    // `/QOPT` is the optical volumes the image catalogs load.
    let opt = ifs::vfs::optical::Volumes::new(super::media::volume_set());
    let ifs = Ifs::with_optical(
        b,
        Box::new(StoreQsys::new(Lent)),
        Box::new(opt),
        Config::default(),
    );
    with_lent(system, JobHandle(0), || {
        let env = StoreEnv::new(&Lent);
        ifs.ensure_layout(&env)?;
        // The HTTP server's instances: the `ADMIN` job writes there.
        // The third-party notices' directory (`keep_notices`).
        for dir in ["/QQS", "/QQS/ProdData", osobjects::notices::DIR] {
            ifs.ensure_dir(&env, dir, "QSYS", "*RX")?;
        }
        // Access for Web's downloads (`keep_downloads`).
        for dir in osobjects::afwdl::DIRS {
            ifs.ensure_dir(&env, dir, "QSYS", "*RX")?;
        }
        ifs.ensure_dir(&env, WWW, WWW_OWNER, WWW_PUBLIC)
    })
    .map_err(|e| format!("making the standard directories: {}", e.text()))?;
    Ok(ifs)
}

/// The file system, when it is up.
pub fn get() -> Option<&'static Ifs<LinuxBackend>> {
    IFS.get().and_then(|r| r.as_ref().ok())
}

/// Let go of the file system so it can be unmounted (power down):
/// every IFS request after it fails.
pub fn shut() {
    if let Some(i) = get() {
        i.backend().close();
    }
}

/// Why there is no file system, if there is none.
pub fn why_not() -> Option<String> {
    match IFS.get() {
        None => Some("the storage service has not started the IFS".into()),
        Some(Err(e)) => Some(e.clone()),
        Some(Ok(_)) => None,
    }
}

/// A job's owner number: its connection, named by its job handle.
#[must_use]
pub fn owner_of(handle: u64) -> Owner {
    Owner(handle)
}

/// The owner the service console's lines use for the job `handle`:
/// apart from the job's connection, released after each line.
#[must_use]
pub fn console_owner(handle: u64) -> Owner {
    Owner(handle | 1 << 63)
}

/// Run `f` against the IFS as `principal` for the job `handle`, its
/// calls owned by `owner`; the unavailable client when there is no IFS.
pub fn with_client<T>(
    system: &mut System,
    handle: u64,
    owner: Owner,
    principal: &Principal,
    job_ccsid: u16,
    f: impl FnOnce(&dyn IfsClient) -> T,
) -> T {
    let Some(ifs) = get() else {
        return f(&Unavailable(Errno::ENOTSUP));
    };
    with_lent(system, JobHandle(handle), || {
        let access = Lent;
        let env = StoreEnv::new(&access);
        let cx = Ctx {
            owner,
            principal,
            env: &env,
            audit: &JournalAudit,
            job_ccsid,
        };
        f(&Direct { ifs, cx: &cx })
    })
}

/// Run `f` against the file system itself as the job whose request
/// this is (`SAV`, `RST`); `None` when there is no IFS.
pub fn with_job_ifs<T>(
    system: &mut System,
    f: impl FnOnce(&Ifs<LinuxBackend>, &Ctx<'_>) -> T,
) -> Option<T> {
    let ifs = get()?;
    let number = system.current_job;
    let handle = u64::from(number);
    let principal = crate::svc::ifs::principal();
    let job_ccsid = job_ccsid(system, number);
    Some(with_lent(system, JobHandle(handle), || {
        let access = Lent;
        let env = StoreEnv::new(&access);
        let cx = Ctx {
            owner: owner_of(handle),
            principal: &principal,
            env: &env,
            audit: &JournalAudit,
            job_ccsid,
        };
        f(ifs, &cx)
    }))
}

/// The job's CCSID, as text conversion uses it (its default CCSID for
/// `*HEX`).
pub fn job_ccsid(system: &System, number: u32) -> u16 {
    system
        .attrs
        .get(&number)
        .map_or(37, |a| match u16::try_from(a.ccsid) {
            Ok(65535) | Err(_) => u16::try_from(a.dftccsid).unwrap_or(37),
            Ok(c) => c,
        })
}

/// Run `f`, a job's request, letting an IFS operation in it defer for a
/// lease break or a poll for break orders when `allow`
/// ([`ifs::lease::defer`]); the deferral, when it did (its reply is then
/// not sent: the loop parks the request, `super::turns`).
pub fn parking<T>(allow: bool, f: impl FnOnce() -> T) -> (T, Option<Deferral>) {
    let before = PARKING.with(|p| p.replace(allow));
    DEFERRED.with(|d| d.set(None));
    let r = f();
    PARKING.with(|p| p.set(before));
    (r, DEFERRED.with(Cell::take))
}

/// Whether a request may be parked: an IFS operation while some job
/// holds a lease, or a `QZLSFILE` job's poll for break orders.
#[must_use]
pub fn may_park(op: &IfsOp) -> bool {
    match op {
        IfsOp::LeaseOrders { .. } => true,
        IfsOp::As { op, .. } => may_park(op),
        _ => get().is_some_and(|i| !i.leases().is_empty()),
    }
}

/// Whether some job holds a lease: a database open for change may then
/// have to wait for it (`db::odp`, `qsyslib::notice`).
#[must_use]
pub fn leased() -> bool {
    get().is_some_and(|i| !i.leases().is_empty())
}

/// Run `f`, a job's database open, letting it defer for a lease break
/// as [`serve`] lets an IFS operation: the deferral is the request's
/// (`super::turns` parks it; its reply is not sent).
pub fn db_deferring<T>(f: impl FnOnce() -> T) -> T {
    let allow = PARKING.with(Cell::get);
    let (r, d) = ifs::lease::defer::deferring(allow, f);
    DEFERRED.with(|c| c.set(d));
    r
}

/// The leases: what the loop checks parked requests against.
#[must_use]
pub fn leases() -> Option<&'static ifs::lease::Leases> {
    get().map(Ifs::leases)
}

/// Answer one `ifs` request for a job. Lease, poll and on-behalf calls
/// are only for the jobs `super::ifscaller` names (`EPERM` otherwise, in
/// the reply the operation has); an on-behalf call (`IfsOp::As`) runs
/// with the principal of the profile it names, the job's owner kept.
pub fn serve(
    system: &mut System,
    handle: u64,
    number: u32,
    principal: &Principal,
    op: IfsOp,
) -> IfsReply {
    let job = system
        .jobs
        .get(number)
        .map_or_else(String::new, |j| j.name.clone());
    if let Err(e) = super::ifscaller::allowed(&job, &op) {
        return stg::ifs::refuse(op, e);
    }
    let allow = PARKING.with(Cell::get);
    let ccsid = job_ccsid(system, number);
    let owner = owner_of(handle);
    let (reply, d) = ifs::lease::defer::deferring(allow, || match op {
        IfsOp::As { who, op } => {
            let me = with_lent(system, JobHandle(handle), || {
                super::ifscaller::principal(&StoreEnv::new(&Lent), &who)
            });
            match me {
                Ok(p) => with_client(system, handle, owner, &p, ccsid, |c| {
                    stg::ifs::serve(c, *op)
                }),
                Err(e) => stg::ifs::refuse(*op, e),
            }
        }
        op => with_client(system, handle, owner, principal, ccsid, |c| {
            stg::ifs::serve(c, op)
        }),
    });
    DEFERRED.with(|c| c.set(d));
    reply
}

/// A job's connection ended: its descriptors (members written back),
/// directory streams and locks go.
pub fn release(system: &mut System, handle: u64) {
    release_owner(system, handle, owner_of(handle));
}

/// End `owner`'s use for the job `handle`.
pub fn release_owner(system: &mut System, handle: u64, owner: Owner) {
    if let Some(ifs) = get() {
        with_lent(system, JobHandle(handle), || ifs.release_owner(owner));
    }
}

/// Size and used bytes of an IFS partition outside the ASP, which the
/// status displays add to ASP 1's figures. A volume in the ASP is ASP
/// pages already counted, so `None` then.
pub fn partition_capacity(system: &System) -> Option<(u64, u64)> {
    let in_asp = system
        .storage
        .as_ref()
        .is_some_and(|s| !s.store.ifs_volume().is_empty());
    if in_asp {
        return None;
    }
    capacity()
}

/// Size and used bytes of the IFS root's filesystem.
pub fn capacity() -> Option<(u64, u64)> {
    use ifs::backend::Backend as _;
    let s = get()?.backend().statfs().ok()?;
    let size = s.blocks.saturating_mul(s.block_size);
    let free = s.bfree.saturating_mul(s.block_size);
    Some((size, size.saturating_sub(free)))
}

#[cfg(test)]
mod tests {
    use ifs::lease::state::{H, R, RWH, Trigger, W};
    use ifs::lease::{Leases, Others};
    use ifs::lock::ObjKey;

    use super::{Owner, db_deferring, parking};

    const KEY: ObjKey = ObjKey { fs: 5, ino: 9 };

    /// Job 1's client caches writes to the object.
    fn leased() -> Leases {
        let l = Leases::default();
        assert_eq!(
            l.grant(Owner(1), KEY, 5, [1; 32], RWH, Others::default()),
            RWH
        );
        l
    }

    #[test]
    fn a_database_open_defers_only_when_its_request_may_park() {
        let l = leased();
        let open = || l.gate(Owner(2), KEY, Trigger::Open { overwrite: false });
        // A job's own open request: the deferral is the request's.
        let (r, d) = parking(true, || db_deferring(open));
        assert_eq!(r, Err(ifs::Errno::EAGAIN));
        assert!(d.is_some_and(|d| !l.ready(&d)));
        l.report(Owner(1), 5, &[([1; 32], R | H)]);
        assert!(d.is_some_and(|d| l.ready(&d)));
        // Anything else (a statement, a command): breaks sent, no wait.
        let l = leased();
        let (r, d) = parking(false, || {
            db_deferring(|| l.gate(Owner(2), KEY, Trigger::Open { overwrite: false }))
        });
        assert_eq!((r, d), (Ok(()), None));
        assert_eq!(l.on(KEY)[0].2 & W, W, "still breaking, not waited for");
    }
}
