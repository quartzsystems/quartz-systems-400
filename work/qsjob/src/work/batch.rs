// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A routed job: the program its routing entry names runs first.
//! `QSYS/QCMD` runs the request data as a command through the analyzer;
//! any other program is called. When it returns the job ends.

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
    let code = if pgm == "QEJSCHED" {
        super::scheduler::run(&mut c, link)
    } else if pgm == "QWCSTRUP" {
        startup(&mut c, link)
    } else if pgm == super::netsvr::SERVER_PROGRAM {
        super::netsvr::server::run(&mut c, link, start.number, &start.name)
    } else if let Some(d) = stg::nfs::Daemon::of_program(pgm) {
        super::nfs::run(&mut c, link, d, start.number)
    } else if matches!(pgm, "QGLDPUBE" | "QGLDPUBA") {
        publisher(&mut c, pgm)
    } else if pgm == "QSPWTR" {
        super::writer::run(&mut c, link, start.rqsdta.trim())
    } else if pgm == "QCMD" {
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
            ended(&c.run(&request, crate::command::RunMode::Run))
        }
    } else {
        status(link, Status::Run, &format!("PGM-{pgm}"));
        ended(&c.run(&format!("call {lib}/{pgm}"), crate::command::RunMode::Run))
    };
    c.ended(code);
    let mut ending = ending;
    let _ = ending.send(&FromJob::Ending {
        reason: format!("ended, code {code}"),
    });
    std::process::exit(0)
}

/// `QGLDPUBE` and `QGLDPUBA`, the directory server's publishing agents
/// (docs/DIRSRV.md section 10.1): `GLD010B` in the job log, and the job
/// ends. BLOCKED(rel): publishing users and printers is not built yet.
fn publisher(c: &mut Console, pgm: &str) -> u16 {
    let data = wm::joblog::data(&[(pgm, 10)]);
    c.job()
        .log_external("GLD010B", "QGLDMSG", &data, qir::MsgType::Info);
    0
}

/// `QWCSTRUP`: call the program `QSTRUPPGM` names, unless `*NONE`.
fn startup(c: &mut Console, link: &Shared) -> u16 {
    let sv = link
        .borrow_mut()
        .with(|l| stg::Client::sysval(l, "QSTRUPPGM".into()));
    let text = sv.map(|v| ccsid::from_ebcdic(&v.data)).unwrap_or_default();
    let mut words = text.split_whitespace();
    let (Some(pgm), lib) = (words.next(), words.next().unwrap_or("*LIBL")) else {
        return 0;
    };
    // A value written as LIB/PGM (an install's answers before V1R1M0).
    let (pgm, lib) = pgm.split_once('/').map_or((pgm, lib), |(l, p)| (p, l));
    if pgm == "*NONE" {
        return 0;
    }
    status(link, Status::Run, &format!("PGM-{pgm}"));
    c.job()
        .log_request(&format!("CALL {lib}/{pgm}"), ("QWCSTRUP", "QSYS"));
    // A value the analyzer refuses (no such name, a bad library) fails
    // as surely as a program that cannot be called or ends in an
    // escape: the job ends abnormally either way.
    let code = ended(&c.run(&format!("call {lib}/{pgm}"), crate::command::RunMode::Run));
    if code != 0 {
        // As IBM i: CPI0915 to QSYSOPR, the details in this job's log;
        // the IPL goes on to sign-on.
        let m = wm::obj::msgq::QMsg {
            kind: wm::obj::msgq::MsgKind::Info,
            severity: 20,
            msgid: STARTUP_FAILED.into(),
            msgf: ("QCPFMSG".into(), "QSYS".into()),
            text: format!(
                "Startup program {lib}/{pgm} did not run to its end; see the job log of this job."
            ),
            ..wm::obj::msgq::QMsg::default()
        };
        if let Err(e) = link
            .borrow_mut()
            .with(|l| stg::Client::msgq_send(l, "*SYSOPR".into(), m))
        {
            eprintln!("qsjob: QSYSOPR: {e}");
        }
    }
    code
}

/// What QSYSOPR is sent when the startup program fails (IBM i's
/// message for `QSTRUPPGM`).
const STARTUP_FAILED: &str = "CPI0915";

/// The end code of a run: 20 when it ended in an escape (the analyzer
/// refused it, the program could not be called or left one).
fn ended(o: &crate::command::Outcome) -> u16 {
    if o.escape.is_some() {
        20
    } else {
        end_code(&o.text)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Fail, Outcome};

    #[test]
    fn a_refused_or_failed_call_ends_the_job_abnormally() {
        // The analyzer's refusal of an unreadable QSTRUPPGM value.
        let refused = Outcome {
            text: "CPD0071 ...\nCPF0006 Errors occurred in command.\n".into(),
            escape: Some(Fail::new("CPF0006", "")),
            ..Outcome::default()
        };
        assert_eq!(ended(&refused), 20);
        let ok = Outcome {
            text: "Program ended.\n".into(),
            ..Outcome::default()
        };
        assert_eq!(ended(&ok), 0);
        assert_eq!(STARTUP_FAILED, "CPI0915");
    }
}
