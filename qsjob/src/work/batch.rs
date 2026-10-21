// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A routed job: the program its routing entry names runs first.
//! `QSYS/QCMD` runs the request data as a command; any other program is
//! called. When it returns the job ends. `MIGRATE(phase7)`: `QCMD` is
//! the command processor; here the request data is a service console
//! command line or `CALL LIB/PGM`.

use ipc::chan::Chan;
use stg::Shared;
use stg::remote::status;
use wm::ctl::{FromJob, Start};
use wm::job::Status;

use crate::console::Console;

/// Run the job. Never returns.
pub fn run(ctl: Chan, link: &Shared, start: &Start) -> ! {
    let ending = match ctl.try_clone() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("qsjob: {}: {e}", start.name);
            std::process::exit(1);
        }
    };
    let mut c = Console::new(ctl, link, start);
    c.started();
    let (lib, pgm) = start
        .program
        .split_once('/')
        .unwrap_or(("QSYS", start.program.as_str()));
    let code = if pgm == "QCMD" {
        let rqs = start.rqsdta.trim();
        if rqs.is_empty() {
            0
        } else {
            let word = rqs.split_whitespace().next().unwrap_or("");
            status(
                link,
                Status::Run,
                &format!("CMD-{}", word.to_ascii_uppercase()),
            );
            c.job().log_request(rqs, ("QCMD", "QSYS"));
            let request = if word.eq_ignore_ascii_case("CALL") {
                format!("call {}", rqs[4..].trim())
            } else {
                rqs.to_string()
            };
            let out = c.line(&request);
            end_code(&out)
        }
    } else {
        status(link, Status::Run, &format!("PGM-{pgm}"));
        let out = c.line(&format!("call {lib}/{pgm}"));
        end_code(&out)
    };
    c.ended(code);
    let mut ending = ending;
    let _ = ending.send(&FromJob::Ending {
        reason: format!("ended, code {code}"),
    });
    std::process::exit(0)
}

/// The end code a run's output implies: 20 when the program could not
/// be called or left an exception, 0 otherwise.
fn end_code(out: &str) -> u16 {
    if out.contains("could not be called") || out.contains("left for the caller") {
        20
    } else {
        0
    }
}
