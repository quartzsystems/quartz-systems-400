// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service console job `QSVCJOB`: lines typed on the serial port go
//! to it; its output and prompt come back through a reader thread as
//! events. `slic` keeps the commands that are its own (the SRC log,
//! history, the display, the keylock, power down).

use std::sync::mpsc::Sender;
use std::thread;

use ipc::chan::Chan;
use stg::Client;
use wm::ctl::{FromJob, Start, ToJob};

use super::spawn::{QSJOB, spawn};
use crate::ipl::Event;
use crate::ipl::system::System;

/// The commands `slic` answers itself while the console is idle.
pub const LOCAL: &[&str] = &[
    "help", "release", "srcs", "hist", "hw", "lic", "mode", "screen", "key", "type", "pwrdwn",
    "dbgkill", "tn5250",
];

/// The running job.
pub struct SvcJob {
    /// Its process ID.
    pub pid: u32,
    chan: Chan,
    /// The prompt it last asked for.
    pub prompt: String,
    /// A line is being worked on.
    pub busy: bool,
}

impl SvcJob {
    /// Pass a line on.
    pub fn line(&mut self, text: &str) -> bool {
        self.busy = true;
        self.chan
            .send(&ToJob::Line {
                text: text.to_string(),
            })
            .is_ok()
    }

    /// Ask it to end.
    pub fn end(&mut self) {
        let _ = self.chan.send(&ToJob::End { immediate: false });
    }

    /// Whether `slic` answers this line itself: a command of its own,
    /// typed while the job waits at the normal prompt.
    pub fn is_local(&self, line: &str) -> bool {
        let word = line.split_whitespace().next().unwrap_or("");
        !self.busy && self.prompt == crate::svc::PROMPT && LOCAL.contains(&word)
    }
}

/// Start `QSVCJOB`; its messages arrive on `tx`.
pub fn start(system: &mut System, tx: Sender<Event>) -> Result<(), String> {
    let number = system
        .procs
        .stg
        .as_mut()
        .ok_or("the storage service is not running")?
        .job_number()
        .map_err(|e| format!("QSVCJOB: {e}"))??;
    let (pid, mut chan) = spawn(QSJOB, &[])?;
    chan.send(&ToJob::Start {
        start: Box::new(Start {
            name: "QSVCJOB".into(),
            user: "QSYS".into(),
            number,
            handle: u64::from(number),
            kind: 'S',
            // Listed under QSYSWRK; no monitor owns it, so ending the
            // subsystem leaves the console working.
            subsystem: "QSYSWRK".into(),
            role: "QSVCJOB".into(),
            jobd: "QSYS/QSYSJOBD".into(),
            ..Start::default()
        }),
    })
    .map_err(|e| format!("QSVCJOB: {e}"))?;
    match chan.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => {}
        Ok(FromJob::Ending { reason }) => return Err(format!("QSVCJOB: {reason}")),
        Ok(other) => return Err(format!("QSVCJOB: unexpected {other:?}")),
        Err(e) => return Err(format!("QSVCJOB: {e}")),
    }
    let mut reader = chan.try_clone().map_err(|e| e.to_string())?;
    thread::spawn(move || {
        while let Ok(m) = reader.recv::<FromJob>() {
            if tx.send(Event::Job(m)).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::JobGone);
    });
    system.procs.svcjob = Some(SvcJob {
        pid,
        chan,
        prompt: crate::svc::PROMPT.to_string(),
        busy: false,
    });
    Ok(())
}

/// Whether a line is one of `slic`'s own commands.
pub fn is_own(line: &str) -> bool {
    LOCAL.contains(&line.split_whitespace().next().unwrap_or(""))
}
