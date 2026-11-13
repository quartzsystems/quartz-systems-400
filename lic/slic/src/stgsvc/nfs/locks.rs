// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RLSIFSLCK` (docs/NFS.md section 8.1): the byte-range locks local
//! jobs hold on an object (`LCLBRNGLCK(*YES)`) or on every object of its
//! file system (`OBJMFS(*YES)`), through the IFS's lock table; those
//! NFS clients hold there (`NFSSVRLCK(*YES)`: the `*NLM` and `*SVR`
//! daemons' NLM and `NFSv4` sub-owners); a remote host's (`RMTLOCNAME`),
//! ordered from the `*NLM` daemon and each `*SVR` daemon (its `NFSv4`
//! clients' state), which carry the order out and say so in their job
//! logs.

use ifs::client::IfsClient;
use ifs::lock::range::RangeKind;
use ifs::lock::{Held, ObjKey};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{F_SETLK, O_RDONLY, SEEK_SET};
use ifscmd::{Msg, Reply};
use stg::nfs::Daemon;

use crate::ipl::system::System;

/// What the command asked.
pub struct Args {
    /// `RMTLOCNAME`, empty when not given.
    pub remote: String,
    /// `OBJ`, empty when not given.
    pub obj: String,
    /// `NFSSVRLCK(*YES)`.
    pub nfs: bool,
    /// `LCLBRNGLCK(*YES)`.
    pub local: bool,
    /// `OBJMFS(*YES)`.
    pub whole_fs: bool,
}

/// Whether a sub-owner is an NFS client's: NLM's from
/// `nfs::nlm::state::SUB_BASE` (`0x4E4C...`), `NFSv4`'s `0x4E34...`.
fn client_sub(sub: u64) -> bool {
    matches!(sub >> 48, 0x4E4C | 0x4E34)
}

/// `CPFA1B6`: reason 1 the `*NLM` daemon does not run, 2 a file system
/// error.
fn not_released(reason: i64, errno: i64, obj: &str) -> Reply {
    Reply::fail(Msg::new("CPFA1B6").bin4(reason).bin4(errno).vary(obj))
}

/// Run `RLSIFSLCK`.
pub fn run(system: &mut System, a: &Args) -> Reply {
    // The daemons whose jobs hold NFS clients' locks.
    let holders: Vec<u32> = {
        let s = super::state();
        let mut v = s.registry.ready_numbers(Daemon::Nlm);
        v.extend(s.registry.ready_numbers(Daemon::Svr));
        v
    };
    let remote = a.remote.trim();
    if !remote.is_empty() {
        // A remote host's locks are the lock manager's to release: the
        // order is queued for it and the command ends, as IBM's does.
        let by = system
            .jobs
            .get(system.current_job)
            .map_or_else(String::new, wm::table::JobInfo::qualified);
        return match super::state().registry.order(remote, &by) {
            Some((_, job)) => Reply::done(Msg::new("CPCA1C7").vary(&job).vary(remote)),
            None => not_released(1, 0, "*N"),
        };
    }
    let obj = a.obj.trim();
    if a.nfs
        && !holders.is_empty()
        && let Err(e) = release_nfs(system, &holders, obj, a.whole_fs)
    {
        return Reply::fail(
            Msg::new("CPFA1B6")
                .bin4(2)
                .bin4(i64::from(e.code()))
                .vary(obj),
        )
        .diag(Msg::errno(e, obj));
    }
    // Without the lock manager no NFS client holds a lock: what is left
    // to release is the local jobs'.
    if !(a.local || a.whole_fs) {
        return Reply::done(Msg::path("CPCA1C9", obj));
    }
    let done =
        crate::stgsvc::ifs::with_job_ifs(system, |ifs, cx| ifs.release_ranges(cx, obj, a.whole_fs));
    match done {
        Some(Ok(_)) => Reply::done(Msg::path("CPCA1C9", obj)),
        Some(Err(e)) => Reply::fail(
            Msg::new("CPFA1B6")
                .bin4(2)
                .bin4(i64::from(e.code()))
                .vary(obj),
        )
        .diag(Msg::errno(e, obj)),
        None => not_released(2, 0, obj),
    }
}

/// The locks the `*NLM` and `*SVR` daemons (jobs `numbers`) hold for
/// their clients on `obj`, or on every object of its file system,
/// unlocked: how many.
fn release_nfs(
    system: &mut System,
    numbers: &[u32],
    obj: &str,
    whole_fs: bool,
) -> Result<usize, ifs::Errno> {
    // The object as the asking job finds it.
    let key = crate::stgsvc::ifs::with_job_ifs(system, |ifs, cx| {
        ifs.stat(cx, obj).map(|s| ObjKey {
            fs: s.vfs,
            ino: s.ino,
        })
    })
    .unwrap_or(Err(ifs::Errno::EIO))?;
    let p = crate::stgsvc::certs::system_principal();
    let mut n = 0;
    for number in numbers {
        let Some(handle) = system.jobs.get(*number).map(|j| j.handle) else {
            continue;
        };
        let owner = crate::stgsvc::ifs::owner_of(handle);
        n += crate::stgsvc::ifs::with_client(system, handle, owner, &p, 37, |c| {
            unlock_subs(c, key, whole_fs)
        })?;
    }
    Ok(n)
}

/// Unlock, as a daemon's owner (`c`), each range its NLM or `NFSv4` lock
/// owners hold on `key` (or its file system).
fn unlock_subs(c: &dyn IfsClient, key: ObjKey, whole_fs: bool) -> Result<usize, ifs::Errno> {
    let mut n = 0;
    for (k, info) in c.owner_locks()? {
        let Held::Range(l) = info.what else { continue };
        let here = if whole_fs { k.fs == key.fs } else { k == key };
        if !here || !client_sub(l.sub) {
            continue;
        }
        let fd = c.open(&info.path, OpenArgs::new(O_RDONLY))?;
        let start = i64::try_from(l.start).unwrap_or(i64::MAX);
        let len = i64::try_from(l.len()).unwrap_or(0);
        let r = c.lock_range_as(fd, l.sub, F_SETLK, RangeKind::Unlock, SEEK_SET, start, len);
        let _ = c.close(fd);
        r?;
        n += 1;
    }
    Ok(n)
}
