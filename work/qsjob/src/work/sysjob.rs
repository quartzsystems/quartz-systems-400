// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system jobs `slic` starts at IPL (type `SYS`): `QSYSARB` to
//! `QSYSARB5`, `QLUS`, `QDBSRV01` to `QDBSRV05`, `QSPLMAINT`, `QALERT`,
//! `QSYSCOMM1`, `QTCPCTL` and `QTCPWRK`. `QSPLMAINT` cleans up spooling
//! every minute; `QTCPWRK` has the storage service look at the lines
//! and carriers under TCP/IP's interfaces every few seconds (the state
//! itself is the service's); the others hold their place until their
//! work exists (`MIGRATE(phase6)` device arbitration, `MIGRATE(phase10)`
//! communications and alerts; the storage service rebuilds access
//! paths itself at IPL).

use std::time::{Duration, Instant};

use ipc::chan::Chan;
use stg::remote::status;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use crate::console::Console;

/// How often `QSPLMAINT` cleans up.
const SPLMAINT: Duration = Duration::from_secs(60);

/// How often `QTCPWRK` looks at the lines.
const TCPWRK: Duration = Duration::from_secs(5);

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
        if start.name == "QTCPWRK" && Instant::now() >= next {
            let reply = link.borrow_mut().with(|l| l.tcp(stg::tcp::TcpOp::Tick));
            if let stg::tcp::TcpReply::Done { notes, .. } = reply {
                for n in notes {
                    eprintln!("qsjob: QTCPWRK: {}", n.line());
                }
            }
            next = Instant::now() + TCPWRK;
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
