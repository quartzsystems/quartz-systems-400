// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The processes `slic` starts once the IPL has the store ready: the
//! storage service, which takes the store over, and the service console
//! job `QSVCJOB`, which runs the console's commands.

pub mod spawn;
pub mod stg;
pub mod svcjob;

/// The children `slic` talks to.
#[derive(Default)]
pub struct Procs {
    /// The storage service.
    pub stg: Option<stg::StgProc>,
    /// The service console job.
    pub svcjob: Option<svcjob::SvcJob>,
}

/// End the service console job, then close the storage service.
pub fn end_all(system: &mut crate::ipl::system::System) {
    if let Some(mut j) = system.procs.svcjob.take() {
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    stg::close(system);
}
