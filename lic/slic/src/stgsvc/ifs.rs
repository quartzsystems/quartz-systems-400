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

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;

use ifs::Errno;
use ifs::backend::linux::LinuxBackend;
use ifs::client::{Direct, IfsClient, Unavailable};
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
    line
}

fn build(system: &mut System) -> Result<Ifs<LinuxBackend>, String> {
    if !mounted(ROOT) {
        return Err(format!("{ROOT} is not mounted"));
    }
    let b = LinuxBackend::open(ROOT).map_err(|e| format!("{ROOT}: {}", e.text()))?;
    let ifs = Ifs::new(b, Box::new(StoreQsys::new(Lent)), Config::default());
    with_lent(system, JobHandle(0), || {
        ifs.ensure_layout(&StoreEnv::new(&Lent))
    })
    .map_err(|e| format!("making the standard directories: {}", e.text()))?;
    Ok(ifs)
}

/// The file system, when it is up.
pub fn get() -> Option<&'static Ifs<LinuxBackend>> {
    IFS.get().and_then(|r| r.as_ref().ok())
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

/// Answer one `ifs` request for a job.
pub fn serve(
    system: &mut System,
    handle: u64,
    number: u32,
    principal: &Principal,
    op: IfsOp,
) -> IfsReply {
    let ccsid = job_ccsid(system, number);
    let owner = owner_of(handle);
    with_client(system, handle, owner, principal, ccsid, |c| {
        stg::ifs::serve(c, op)
    })
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

/// Size and used bytes of the IFS root's filesystem, for the status
/// displays.
pub fn capacity() -> Option<(u64, u64)> {
    use ifs::backend::Backend as _;
    let s = get()?.backend().statfs().ok()?;
    let size = s.blocks.saturating_mul(s.block_size);
    let free = s.bfree.saturating_mul(s.block_size);
    Some((size, size.saturating_sub(free)))
}
