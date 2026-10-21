// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The work `slic` starts at the end of the IPL, as `SCPF` does: the
//! system jobs, `QSYSWRK` and the controlling subsystem, whose
//! autostart job `QSTRUPJD` runs the startup program; the IPL completes
//! when that job has ended. At power down the subsystems end first,
//! then these jobs.

use std::time::{Duration, Instant};

use ipc::chan::Chan;
use stg::Client;
use wm::ctl::{FromJob, Start, ToJob};

use super::spawn::{QSJOB, spawn, wait_or_kill};
use crate::ipl::system::System;

/// The system jobs, in the order they start.
pub const SYSTEM_JOBS: &[&str] = &[
    "QSYSARB",
    "QSYSARB2",
    "QSYSARB3",
    "QSYSARB4",
    "QSYSARB5",
    "QLUS",
    "QDBSRV01",
    "QDBSRV02",
    "QDBSRV03",
    "QDBSRV04",
    "QDBSRV05",
    "QSPLMAINT",
    "QALERT",
    "QSYSCOMM1",
];

/// The startup job the controlling subsystem starts.
pub const STARTUP_JOB: &str = "QSTRUPJD";

/// A job `slic` started.
pub struct Child {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
}

impl Child {
    /// Ask it to end.
    pub fn end(&mut self, immediate: bool) {
        let _ = self.chan.send(&ToJob::End { immediate });
    }
}

/// Start a job from `start`, numbered by the service; it is ready when
/// this returns.
fn start_job(system: &mut System, mut start: Start) -> Result<Child, String> {
    let stg = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?;
    let number = stg
        .job_number()
        .map_err(|e| format!("{}: {e}", start.name))??;
    start.number = number;
    start.handle = u64::from(number);
    let name = start.name.clone();
    let (pid, mut chan) = spawn(QSJOB, &[])?;
    chan.send(&ToJob::Start {
        start: Box::new(start),
    })
    .map_err(|e| format!("{name}: {e}"))?;
    match chan.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => Ok(Child { pid, chan }),
        Ok(FromJob::Ending { reason }) => Err(format!("{name}: {reason}")),
        Ok(other) => Err(format!("{name}: unexpected {other:?}")),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// Start the system jobs; how many started.
pub fn start_system(system: &mut System) -> usize {
    let mut n = 0;
    for name in SYSTEM_JOBS {
        let start = Start {
            name: (*name).into(),
            user: "QSYS".into(),
            kind: 'S',
            role: "SYSTEM".into(),
            jobd: "QSYS/QSYSJOBD".into(),
            class: "QSYS/QSYSCLS20".into(),
            pool: "*MACHINE".into(),
            ..Start::default()
        };
        match start_job(system, start) {
            Ok(c) => {
                system.procs.jobs.push(c);
                n += 1;
            }
            Err(e) => system.out.serial(&format!("slic: system job {e}\n")),
        }
    }
    n
}

/// Start subsystem `QSYS/name`'s monitor and wait until it registers.
pub fn start_subsystem(system: &mut System, name: &str) -> Result<(), String> {
    let start = Start {
        name: name.into(),
        user: "QSYS".into(),
        kind: 'M',
        subsystem: name.into(),
        role: "MONITOR".into(),
        jobd: "QSYS/QSYSJOBD".into(),
        class: "QSYS/QSYSCLS20".into(),
        rqsdta: format!("QSYS/{name}"),
        pool: "*BASE".into(),
        ..Start::default()
    };
    let c = start_job(system, start)?;
    system.procs.jobs.push(c);
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        let active = system
            .procs
            .stg
            .as_mut()
            .map(|s| s.sbs_list().unwrap_or_default())
            .unwrap_or_default();
        if active.iter().any(|a| a.name == name) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(format!("CPF1012 subsystem {name} did not start"))
}

fn state(system: &mut System, name: &str) -> u8 {
    system
        .procs
        .stg
        .as_mut()
        .and_then(|s| s.job_state(name.into()).ok())
        .unwrap_or(0)
}

/// Wait for job `name` to start within `appear` and end within
/// `finish`; whether it ran and ended.
pub fn wait_job(system: &mut System, name: &str, appear: Duration, finish: Duration) -> bool {
    let until = Instant::now() + appear;
    while state(system, name) == 0 {
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let until = Instant::now() + finish;
    while state(system, name) != 2 {
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

/// `ENDSBS *ALL`: every subsystem ends, controlled within `delay`
/// seconds or at once; wait for them, a little past the delay.
pub fn end_subsystems(system: &mut System, immediate: bool, delay: u32) {
    let Some(stg) = system.procs.stg.as_mut() else {
        return;
    };
    let _ = stg.sbs_end("*ALL".into(), immediate, delay);
    let until = Instant::now() + Duration::from_secs(u64::from(delay) + 15);
    while Instant::now() < until {
        let left = stg.sbs_list().unwrap_or_default();
        if left.is_empty() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// End the jobs `slic` started.
pub fn end_children(system: &mut System) {
    let mut jobs = std::mem::take(&mut system.procs.jobs);
    for c in &mut jobs {
        c.end(false);
    }
    for c in jobs {
        wait_or_kill(c.pid, Duration::from_secs(3));
    }
}
