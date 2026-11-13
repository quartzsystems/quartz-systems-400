// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QNFSRGYD`: the `NFSv4` identity mapping. On QS/400 the server jobs
//! ask the storage service themselves (`NfsOp::NameOfUid` ...: one
//! request, kept in the job), so this daemon's running is what turns
//! the mapping on (`NfsOp::Mapping`): while it runs, v4 owners are
//! `profile@domain`; while it does not, they are numeric strings
//! (docs/NFS.md section 4, "The registry daemon").

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::host::Host;
use super::running::Running;
use super::{DaemonEnv, Serve, Started};

/// What the daemon serves beside turning the mapping on.
pub fn registry_service<H: Host>(_host: &Arc<H>, stop: &AtomicBool) {
    // BLOCKED(rel): NFSv4 in the server job (NFS-6): the registry's lookups (nfs::registry) for the server jobs and the client, if they are to go through this job.
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Serve: ready at once; the mapping is on while the job runs.
pub fn serve<H: Host>(_env: &mut dyn DaemonEnv, _started: Started, host: &Arc<H>) -> Serve {
    let h = Arc::clone(host);
    let r = Running::spawn("nfs-rgyd", move |stop, ready| {
        ready();
        registry_service(&h, stop);
        Ok(())
    });
    match r {
        Ok(r) => Serve::Ready(r),
        Err(f) => Serve::Unavailable(f),
    }
}
