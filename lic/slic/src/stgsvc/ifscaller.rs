// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IFS calls made for someone else (`IfsOp::As`, `ifs::client::caller`):
//! which jobs may make them, who the caller then is, and what the
//! service keeps across requests to answer that quickly (the profiles
//! by UID and the principals built, until a profile changes). Which
//! jobs may hold leases and poll for news is here too.

use ifs::Errno;
use ifs::client::caller::{self, Caller};
use qsyslib::env::EnvCache;
use sec::auth::principal::Principal;
use stg::ifs::IfsOp;

/// The jobs whose calls may name another user: the NFS server's.
pub const AS_JOBS: &[&str] = &["QNFSNFSD", "QNFSMNTD", "QNFSNLMD"];

/// The jobs that hold leases and watch directories: `QZLSFILE`
/// (`docs/NETSERVER.md` section 6) and `QNFSNFSD` (a read delegation is
/// a read lease, `docs/NFS.md` section 5).
pub const LEASE_JOBS: &[&str] = &["QZLSFILE", "QNFSNFSD"];

/// The jobs that poll for news (`lease_orders`) and so may queue locks
/// whose grant the poll tells: the lease holders and the lock manager.
pub const POLL_JOBS: &[&str] = &["QZLSFILE", "QNFSNFSD", "QNFSNLMD"];

/// The profile a call for [`Caller::Anonymous`] runs as.
pub const ANONYMOUS: &str = "QNFSANON";

/// What the service keeps across requests for the profile environment.
pub static CACHE: std::sync::LazyLock<EnvCache> = std::sync::LazyLock::new(EnvCache::default);

/// Whether the job named `job` may make `op` (its inner operation too
/// for `As`); `EPERM` otherwise.
pub fn allowed(job: &str, op: &IfsOp) -> Result<(), Errno> {
    let ok = match op {
        IfsOp::As { op, .. } => {
            return if AS_JOBS.contains(&job) {
                allowed(job, op)
            } else {
                Err(Errno::EPERM)
            };
        }
        IfsOp::LeaseGrant { .. }
        | IfsOp::LeaseReport { .. }
        | IfsOp::WatchAdd { .. }
        | IfsOp::WatchEnd { .. }
        | IfsOp::LeaseReset => LEASE_JOBS.contains(&job),
        IfsOp::LeaseOrders { .. } | IfsOp::LockWait { .. } | IfsOp::LockCancel { .. } => {
            POLL_JOBS.contains(&job)
        }
        _ => true,
    };
    if ok { Ok(()) } else { Err(Errno::EPERM) }
}

/// The principal `who` names, from the profiles in `env`.
///
/// # Errors
///
/// `ENOTENROLL` when no profile is `who` (the server then maps the
/// request to its anonymous user or refuses it).
pub fn principal(env: &dyn ifs::auth::AuthEnv, who: &Caller) -> Result<Principal, Errno> {
    let name = caller::resolve(who, env, Some(ANONYMOUS))?;
    env.principal_of(&name).ok_or(Errno::ENOTENROLL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_nfs_jobs_call_as_others() {
        let op = IfsOp::As {
            who: Caller::Uid { uid: 5 },
            op: Box::new(IfsOp::Getcwd),
        };
        assert_eq!(allowed("QNFSNFSD", &op), Ok(()));
        assert_eq!(allowed("QNFSMNTD", &op), Ok(()));
        assert_eq!(allowed("QZLSFILE", &op), Err(Errno::EPERM));
        assert_eq!(allowed("QPADEV0001", &op), Err(Errno::EPERM));
        let lease = IfsOp::LeaseReset;
        assert_eq!(allowed("QNFSNFSD", &lease), Ok(()));
        assert_eq!(allowed("QNFSNLMD", &lease), Err(Errno::EPERM));
        let poll = IfsOp::LeaseOrders { wait_ms: 1 };
        assert_eq!(allowed("QNFSNLMD", &poll), Ok(()));
        assert_eq!(allowed("QPADEV0001", &poll), Err(Errno::EPERM));
        // The inner operation is checked as well.
        let inner = IfsOp::As {
            who: Caller::Anonymous,
            op: Box::new(IfsOp::LeaseReset),
        };
        assert_eq!(allowed("QNFSMNTD", &inner), Err(Errno::EPERM));
        assert_eq!(allowed("QPADEV0001", &IfsOp::Getcwd), Ok(()));
    }
}
