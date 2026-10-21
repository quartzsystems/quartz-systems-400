// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The processes `slic` starts once the IPL has the store ready: the
//! storage service, which takes the store over, and the service console
//! job `QSVCJOB`, which runs the console's commands.

pub mod console;
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
    /// The job signed on at the console display.
    pub console: Option<console::ConsoleJob>,
}

/// End the service console job, then close the storage service.
pub fn end_all(system: &mut crate::ipl::system::System) {
    if let Some(mut j) = system.procs.console.take() {
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    if let Some(mut j) = system.procs.svcjob.take() {
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    stg::close(system);
}

/// Put a process in a storage pool's cgroup, making the cgroup when it
/// is missing (`docs/WORKMGMT.md`, pools).
pub fn cgroup_place(pool: &str, pid: u32) {
    let mut dir = std::path::PathBuf::from("/sys/fs/cgroup/qs400");
    for part in pool.trim_start_matches('*').split('/') {
        dir.push(part);
    }
    let _ = std::fs::write("/sys/fs/cgroup/cgroup.subtree_control", "+memory +pids");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(
        "/sys/fs/cgroup/qs400/cgroup.subtree_control",
        "+memory +pids",
    );
    if let Err(e) = std::fs::write(dir.join("cgroup.procs"), pid.to_string()) {
        eprintln!("slic: job {pid} into pool {pool}: {e}");
    }
}
