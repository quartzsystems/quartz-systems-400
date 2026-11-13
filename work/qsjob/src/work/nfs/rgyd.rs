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

/// The daemon's loop: it serves nothing itself; while it runs the
/// server jobs' `ids::JobIdMap` maps names (`NfsOp::Mapping`), each
/// asking the service through its own registry cache.
pub fn registry_service<H: Host>(_host: &Arc<H>, stop: &AtomicBool) {
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
