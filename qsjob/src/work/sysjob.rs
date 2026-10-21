// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system jobs `slic` starts at IPL (type `SYS`): `QSYSARB` to
//! `QSYSARB5`, `QLUS`, `QDBSRV01` to `QDBSRV05`, `QSPLMAINT`, `QALERT`
//! and `QSYSCOMM1`. `QSPLMAINT` cleans up spooling every minute; the
//! others hold their place until their work exists (`MIGRATE(phase6)`
//! device arbitration, `MIGRATE(phase10)` communications and alerts;
//! the storage service rebuilds access paths itself at IPL).

use std::time::{Duration, Instant};

use ipc::chan::Chan;
use stg::remote::status;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use crate::console::Console;

/// How often `QSPLMAINT` cleans up.
const SPLMAINT: Duration = Duration::from_secs(60);

/// Run system job `start.name` until it is ended. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = ctl.try_clone();
    let mut c = Console::new(ctl, link, start);
    let mut next = Instant::now();
    loop {
        if start.name == "QSPLMAINT" && Instant::now() >= next {
            status(link, Status::Run, "SPLMAINT");
            let text = link.borrow_mut().with(Client::spool_maint);
            if !text.is_empty() {
                eprintln!("qsjob: QSPLMAINT: {text}");
            }
            next = Instant::now() + SPLMAINT;
        }
        status(link, Status::Evtw, "");
        if !c.idle(Duration::from_secs(1)) {
            break;
        }
    }
    if let Ok(mut e) = ending {
        let _ = e.send(&FromJob::Ending {
            reason: format!("{} ended", start.name),
        });
    }
    std::process::exit(0)
}
