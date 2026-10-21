// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting: who the job is, the link to the storage service, then the
//! job's role.

use std::path::Path;

use ipc::chan::Chan;
use stg::{Client, JobInfo, Link};
use wm::ctl::{FromJob, ToJob};

/// Run the job. Never returns.
pub fn run() -> ! {
    let mut ctl = match Chan::from_stdin() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qsjob: no control channel: {e}");
            std::process::exit(1);
        }
    };
    let start = match ctl.recv::<ToJob>() {
        Ok(ToJob::Start { start }) => start,
        Ok(other) => end(&mut ctl, &format!("expected the start, got {other:?}")),
        Err(e) => end(&mut ctl, &format!("control channel: {e}")),
    };
    let link = match Chan::connect(Path::new(stg::SOCKET)) {
        Ok(c) => Link::over(c),
        Err(e) => end(&mut ctl, &format!("storage service: {e}")),
    };
    let link = link.shared();
    let hello = link.borrow_mut().hello(JobInfo {
        name: start.name.clone(),
        user: start.user.clone(),
        number: start.number,
        kind: start.kind,
        subtype: ' ',
        subsystem: start.subsystem.clone(),
        pid: std::process::id(),
        handle: start.handle,
        jobd: start.jobd.clone(),
        class: start.class.clone(),
        pool: start.pool.clone(),
        ..JobInfo::default()
    });
    match hello {
        Ok(Ok(())) => {}
        Ok(Err(e)) => end(&mut ctl, &format!("storage service: {e}")),
        Err(e) => end(&mut ctl, &format!("storage service: {e}")),
    }
    let _ = ctl.send(&FromJob::Ready {
        pid: std::process::id(),
    });
    match start.role.as_str() {
        "QSVCJOB" => crate::console::serve(ctl, &link, &start),
        "MONITOR" => crate::work::monitor::run(ctl, &link, &start),
        "BATCH" => crate::work::batch::run(ctl, &link, &start),
        "INTER" => crate::work::inter::run(ctl, &link, &start),
        "SYSTEM" => crate::work::sysjob::run(ctl, &link, &start),
        other => end(&mut ctl, &format!("no role {other}")),
    }
}

/// Tell the parent why, and exit.
pub fn end(ctl: &mut Chan, reason: &str) -> ! {
    let _ = ctl.send(&FromJob::Ending {
        reason: reason.to_string(),
    });
    std::process::exit(1)
}
