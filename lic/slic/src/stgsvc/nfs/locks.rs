// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RLSIFSLCK` (docs/NFS.md section 8): the byte-range locks local jobs
//! hold on an object (`LCLBRNGLCK(*YES)`) or on every object of its file
//! system (`OBJMFS(*YES)`), through the IFS's lock table; `CPCA1C9`.
//! The locks NFS clients hold go through the `*NLM` daemon.

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

/// `CPFA1B6`: reason 1 the `*NLM` daemon does not run, 2 a file system
/// error.
fn not_released(reason: i64, obj: &str) -> Reply {
    Reply::fail(Msg::new("CPFA1B6").bin4(reason).bin4(0).vary(obj))
}

/// Run `RLSIFSLCK`.
pub fn run(system: &mut System, a: &Args) -> Reply {
    let nlm = super::state().registry.ready(Daemon::Nlm) > 0;
    if !a.remote.trim().is_empty() {
        // A remote host's locks are the lock manager's to release.
        if !nlm {
            return not_released(1, "*N");
        }
        // BLOCKED(rel): the release request to the *NLM daemon, which answers CPCA1C7 (NFS wave 2).
        return not_released(2, "*N");
    }
    let obj = a.obj.trim();
    if a.nfs && nlm {
        // BLOCKED(rel): the locks NFS clients hold on the object, released by the *NLM daemon (NFS wave 2).
        return not_released(2, obj);
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
        None => not_released(2, obj),
    }
}
