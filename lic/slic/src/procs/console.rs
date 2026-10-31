// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console job `QCONSOLE`: it holds the console display
//! `QSCONSOLE` as a telnet session holds a virtual display (sign-on,
//! routing through the controlling subsystem's work station entries,
//! the jobs, device recovery), with or without TCP/IP. `slic` starts it
//! when the IPL completes, starts it again when it ends, and links the
//! framebuffer console's terminal to it (`ipl::link`).

use std::sync::mpsc::Sender;
use std::thread;

use ipc::chan::Chan;
use stg::Client;
use wm::ctl::{FromJob, Start, ToJob};
use wm::dev::CONSOLE_JOB;

use super::spawn::{QSJOB, spawn};
use crate::ipl::Event;
use crate::ipl::system::System;

/// The running console job.
pub struct ConsoleJob {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
}

impl ConsoleJob {
    /// Ask it to end.
    pub fn end(&mut self) {
        let _ = self.chan.send(&ToJob::End { immediate: false });
    }
}

/// The controlling subsystem.
pub fn controlling(system: &System) -> String {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QCTLSBSD").ok())
        .and_then(|v| v.text().split_whitespace().next().map(str::to_string))
        .unwrap_or_else(|| "QBASE".into())
}

/// Start `QCONSOLE`; its messages arrive on `tx`.
pub fn start(system: &mut System, tx: Sender<Event>) -> Result<(), String> {
    let number = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?
        .job_number()
        .map_err(|e| format!("{CONSOLE_JOB}: {e}"))??;
    let (pid, mut chan) = spawn(QSJOB, &[])?;
    chan.send(&ToJob::Start {
        start: Box::new(Start {
            name: CONSOLE_JOB.into(),
            user: "QSYS".into(),
            number,
            handle: u64::from(number),
            kind: 'S',
            // Like QSVCJOB: listed under QSYSWRK, owned by no monitor,
            // so the console outlives ENDSBS and ENDTCP.
            subsystem: "QSYSWRK".into(),
            role: "CONSOLE".into(),
            jobd: "QSYS/QSYSJOBD".into(),
            ..Start::default()
        }),
    })
    .map_err(|e| format!("{CONSOLE_JOB}: {e}"))?;
    match chan.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => {}
        Ok(FromJob::Ending { reason }) => return Err(format!("{CONSOLE_JOB}: {reason}")),
        Ok(other) => return Err(format!("{CONSOLE_JOB}: unexpected {other:?}")),
        Err(e) => return Err(format!("{CONSOLE_JOB}: {e}")),
    }
    let mut reader = chan.try_clone().map_err(|e| e.to_string())?;
    thread::spawn(move || {
        while let Ok(m) = reader.recv::<FromJob>() {
            if tx.send(Event::ConsoleJob(m)).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::ConsoleGone(pid));
    });
    system.procs.console = Some(ConsoleJob { pid, chan });
    Ok(())
}

/// What the console job sent.
pub fn from_job(system: &mut System, m: FromJob) {
    match m {
        FromJob::Output { text } => system.out.serial(&text),
        FromJob::Ending { reason } => system
            .out
            .serial(&format!("slic: {CONSOLE_JOB} ending: {reason}\n")),
        FromJob::PowerDown {
            immediate,
            delay,
            restart,
            iplsrc,
        } => crate::ipl::power::down_ending(system, restart, immediate, delay, iplsrc),
        _ => {}
    }
}

/// The console job is gone: started again (power down takes it out of
/// `procs` before it ends, so this does not see it).
pub fn gone(system: &mut System, pid: u32, tx: &Sender<Event>) {
    if system.procs.console.as_ref().is_none_or(|j| j.pid != pid) {
        return;
    }
    system.procs.console = None;
    system
        .out
        .serial(&format!("slic: {CONSOLE_JOB} ({pid}) ended; starting it again\n"));
    if let Err(e) = start(system, tx.clone()) {
        system.out.serial(&format!("slic: {e}\n"));
    }
}
