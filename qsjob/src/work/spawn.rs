// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting a job process: `/qsjob`, its end of a socket pair as its
//! standard input, its output on the serial line, the start message.

use std::process::{Child, Command, Stdio};

use ipc::chan::Chan;
use wm::ctl::{FromJob, Start, ToJob};

/// The job process binary.
pub const QSJOB: &str = "/qsjob";
/// Where job processes write what they print.
const SERIAL: &str = "/dev/ttyS0";

/// A job process this one started.
pub struct Spawned {
    /// The process.
    pub child: Child,
    /// The control channel to it.
    pub chan: Chan,
    /// Who it is.
    pub start: Start,
}

/// Start a job process and send it `start`; it is placed in its pool's
/// cgroup before it runs anything of its own.
pub fn job(start: Start) -> Result<Spawned, String> {
    let (mut parent, child) = Chan::pair().map_err(|e| e.to_string())?;
    let out = || {
        std::fs::OpenOptions::new()
            .write(true)
            .open(SERIAL)
            .map_or_else(|_| Stdio::null(), Stdio::from)
    };
    let c = Command::new(QSJOB)
        .env_clear()
        .stdin(Stdio::from(child.into_fd()))
        .stdout(out())
        .stderr(out())
        .spawn()
        .map_err(|e| format!("{QSJOB}: {e}"))?;
    if !start.pool.is_empty() {
        super::cgroup::place(&start.pool, c.id());
    }
    parent
        .send(&ToJob::Start {
            start: Box::new(start.clone()),
        })
        .map_err(|e| format!("starting {}: {e}", start.name))?;
    Ok(Spawned {
        child: c,
        chan: parent,
        start,
    })
}

/// Wait for a started job to say it is ready.
pub fn ready(s: &mut Spawned) -> Result<(), String> {
    match s.chan.recv::<FromJob>() {
        Ok(FromJob::Ready { .. }) => Ok(()),
        Ok(FromJob::Ending { reason }) => Err(format!("{}: {reason}", s.start.name)),
        Ok(other) => Err(format!("{}: unexpected {other:?}", s.start.name)),
        Err(e) => Err(format!("{}: {e}", s.start.name)),
    }
}
