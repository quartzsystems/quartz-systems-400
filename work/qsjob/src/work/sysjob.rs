// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system jobs `slic` starts at IPL (type `SYS`): `QSYSARB` to
//! `QSYSARB5`, `QLUS`, `QDBSRV01` to `QDBSRV05`, `QSPLMAINT`, `QALERT`,
//! `QSYSCOMM1`, `QTCPCTL` and `QTCPWRK`. `QSYSARB5` has the service
//! watch the system ASP's threshold and lower limit, the power supply
//! and the batch restricted state every ten seconds, and runs the
//! commands it decides on (`stgsvc::arb`). `QSPLMAINT` cleans up spooling
//! every minute; `QTCPWRK` has the storage service look at the lines
//! and carriers under TCP/IP's interfaces every few seconds (the state
//! itself is the service's); `QSYSCOMM1` has the service check the
//! certificate stores for expiry ten minutes after the IPL and then daily
//! (warnings go to `QSYSOPR`) and hourly whether an update check is due
//! (`QSCHGUPDA`); `QDBFSTCCOL` has the service collect the column
//! statistics the planner asked for and those gone stale every minute
//! (`QDBFSTCCOL` system value `*NONE`: none); the others hold their place: the storage service
//! keeps the devices' states itself (varied on, signed on, the job
//! at each), rebuilds access paths at IPL, and
//! communications are `QTCPWRK`'s and the servers'. No alerts are
//! sent: QS/400 has no SNA focal point to send them to.

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

/// When `QSYSCOMM1` first checks the certificates, after it starts.
const CERTCHK_FIRST: Duration = Duration::from_mins(10);

/// How often `QSYSCOMM1` checks them after that.
const CERTCHK: Duration = Duration::from_hours(24);

/// How often `QSYSARB5` looks at storage, power and the batch
/// restricted state.
const ARB: Duration = Duration::from_secs(10);

/// How often `QDBFSTCCOL` collects the database statistics due.
const STCCOL: Duration = Duration::from_mins(1);

/// How often `QSYSCOMM1` asks whether an update check is due.
const UPDCHK: Duration = Duration::from_hours(1);

/// Run system job `start.name` until it is ended. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = ctl.try_clone();
    let mut c = Console::new(ctl, link, start);
    let mut next = Instant::now();
    let mut certchk = Instant::now() + CERTCHK_FIRST;
    let mut updchk = Instant::now() + CERTCHK_FIRST;
    loop {
        if start.name == "QSYSCOMM1" && Instant::now() >= certchk {
            status(link, Status::Run, "CERTCHK");
            for line in link.borrow_mut().with(Client::cert_expiry) {
                eprintln!("qsjob: QSYSCOMM1: {line}");
            }
            certchk = Instant::now() + CERTCHK;
        }
        if start.name == "QSYSCOMM1" && Instant::now() >= updchk {
            status(link, Status::Run, "UPDCHK");
            for line in link.borrow_mut().with(Client::upd_tick) {
                eprintln!("qsjob: QSYSCOMM1: {line}");
            }
            updchk = Instant::now() + UPDCHK;
        }
        if start.name == "QSYSARB5" && Instant::now() >= next {
            status(link, Status::Run, "ARB");
            // Taken first: the commands use the link themselves.
            let cmds = link.borrow_mut().with(Client::arb_tick);
            for cmd in cmds {
                let out = c.line(&cmd);
                eprintln!("qsjob: QSYSARB5: {cmd}: {}", out.trim_end());
            }
            next = Instant::now() + ARB;
        }
        if start.name == "QDBFSTCCOL" && Instant::now() >= next {
            status(link, Status::Run, "STCCOL");
            for line in link.borrow_mut().with(Client::stats_tick) {
                eprintln!("qsjob: QDBFSTCCOL: {line}");
            }
            next = Instant::now() + STCCOL;
        }
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
