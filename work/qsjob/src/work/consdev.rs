// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QCONSOLE`: the console job, which holds the console display
//! `QSCONSOLE` the way a telnet session holds a virtual display. `slic`
//! starts it once the IPL is complete and connects the framebuffer
//! console's terminal to its socket; the session is the telnet
//! server's (`telnet::console_session`), so the console shows the
//! shipped sign-on, starts and recovers its jobs and answers System
//! Request and Attention like any display, with or without TCP/IP.
//! One connection at a time; a new one is taken when `slic` connects
//! again.

use std::os::unix::net::UnixListener;
use std::time::Duration;

use ipc::chan::Chan;
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start, ToJob};
use wm::dev;

use super::telnet;

/// Run the console job. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, _start: &Start) -> ! {
    let host = link
        .borrow_mut()
        .with(|l| l.sysval("QSYSNAME".into()))
        .map_or_else(
            || "QS400".to_string(),
            |v| ccsid::from_ebcdic(&v.data).trim().to_string(),
        );
    let _ = std::fs::create_dir_all(dev::DIR);
    let _ = std::fs::remove_file(dev::CONSOLE_TERMINAL);
    let listener = match UnixListener::bind(dev::CONSOLE_TERMINAL) {
        Ok(l) => l,
        Err(e) => crate::start::end(&mut ctl, &format!("{}: {e}", dev::CONSOLE_TERMINAL)),
    };
    // `slic` ends the job at power down; the jobs kept for a later
    // sign-on end with it.
    let mut reader = ctl.try_clone().expect("ctl");
    std::thread::spawn(move || {
        while let Ok(m) = reader.recv::<ToJob>() {
            if matches!(m, ToJob::End { .. }) {
                break;
            }
        }
        telnet::end_disconnected();
        std::thread::sleep(Duration::from_millis(200));
        std::process::exit(0);
    });
    stg::remote::status(link, wm::job::Status::Selw, "");
    for conn in listener.incoming() {
        let Ok(stream) = conn else {
            continue;
        };
        // `QDSCJOBITV`: jobs kept since an earlier connection.
        let minutes = link
            .borrow_mut()
            .with(|l| l.sysval("QDSCJOBITV".into()))
            .map(|v| ccsid::from_ebcdic(&v.data).trim().to_string())
            .and_then(|s| s.parse::<u64>().ok());
        telnet::sweep(minutes);
        let r = telnet::console_session(stream, &mut link.borrow_mut(), &host);
        if let Err(e) = r {
            eprintln!("qsjob: {}: {e}", dev::CONSOLE_JOB);
        }
    }
    telnet::end_disconnected();
    let _ = ctl.send(&FromJob::Ending {
        reason: "ended".into(),
    });
    std::process::exit(0)
}
