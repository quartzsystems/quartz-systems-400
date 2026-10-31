// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The processes `slic` starts: first the hardware resource service,
//! then, once the IPL has the store ready, the storage service, which
//! takes the store over, the system jobs and the IPL's subsystems, and
//! the service console job `QSVCJOB`, which runs the console's commands.

pub mod console;
pub mod hwr;
pub mod spawn;
pub mod stg;
pub mod svcjob;
pub mod sysjobs;

/// The children `slic` talks to.
#[derive(Default)]
pub struct Procs {
    /// The hardware resource service.
    pub hwr: Option<hwr::HwrProc>,
    /// The storage service.
    pub stg: Option<stg::StgProc>,
    /// The service console job.
    pub svcjob: Option<svcjob::SvcJob>,
    /// The job signed on at the console display.
    pub console: Option<console::ConsoleJob>,
    /// The console job `DSCJOB` or inactivity left running.
    pub disconnected: Option<console::Disconnected>,
    /// The system jobs and the subsystem monitors `slic` started.
    pub jobs: Vec<sysjobs::Child>,
}

/// End the subsystems (controlled within `delay` seconds, or at once),
/// the console and service console jobs, the system jobs, then close
/// the storage service.
pub fn end_all(system: &mut crate::ipl::system::System, immediate: bool, delay: u32) {
    if system.procs.stg.is_some() {
        sysjobs::end_subsystems(system, immediate, delay);
        system.emit(&crate::src::SBS_ENDED);
    }
    if let Some(mut j) = system.procs.console.take() {
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    if let Some(d) = system.procs.disconnected.take() {
        let mut j = d.job;
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    if let Some(mut j) = system.procs.svcjob.take() {
        j.end();
        spawn::wait_or_kill(j.pid, std::time::Duration::from_secs(5));
    }
    if !system.procs.jobs.is_empty() {
        sysjobs::end_children(system);
        system.emit(&crate::src::SYSJOBS_ENDED);
    }
    stg::close(system);
}

/// Put a process in a storage pool's cgroup, making the cgroup when it
/// is missing (`docs/WORKMGMT.md`, pools).
/// The directory of the display device sockets (`wm::dev`).
pub fn device_dir() {
    let _ = std::fs::create_dir_all(wm::dev::DIR);
}

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
