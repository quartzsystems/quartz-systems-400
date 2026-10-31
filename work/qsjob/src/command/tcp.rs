// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `STRTCP`, `ENDTCP` and `ENDTCPABN` in the job: the storage service
//! applies the configuration (`stg::tcp`); the job starts and ends the
//! servers through the TCP servers framework (`tcpsvr.rs`), since a
//! server is a job this job spawns. The TCP/IP displays in a job
//! without a display answer here too, as text.

use std::fmt::Write as _;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use stg::tcp::{TcpOp, TcpReply};
use tcpcfg::msg::Note;
use tcpcfg::server::{end_all, start_all};

use super::{Fail, parms_of};
use crate::console::host::Host;

/// Ask the service.
#[allow(clippy::unnecessary_wraps)] // callers chain it with `?`
pub fn ask(host: &Host, op: TcpOp) -> Result<TcpReply, Fail> {
    Ok(host.link.borrow_mut().with(|l| l.tcp(op)))
}

fn lines(notes: &[Note], out: &mut String) {
    for n in notes {
        let _ = writeln!(out, "{}", n.line());
    }
}

fn param(r: &Resolved, kwd: &str) -> String {
    parms_of(r).name(kwd)
}

/// `STRTCP STRSVR STRIFC STRIP6`.
pub fn strtcp(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let strsvr = param(r, "STRSVR") != "*NO";
    let op = TcpOp::Start {
        strsvr,
        strifc: param(r, "STRIFC") != "*NO",
        strip6: param(r, "STRIP6") != "*NO",
    };
    let (notes, servers) = match ask(host, op)? {
        TcpReply::Done { notes, servers } => (notes, servers),
        TcpReply::Failed { note } => return Err(Fail::new(&note.msgid, note.line())),
        other => return Err(Fail::check("", format!("unexpected answer {other:?}"))),
    };
    let mut out = String::new();
    lines(&notes, &mut out);
    let fw = super::tcpsvr::framework();
    let mut env = super::tcpsvr::JobEnv::new(job, host);
    let mut starter = servers::framework::Starter::new(&fw, &mut env);
    for (server, r) in start_all(&mut starter, &servers) {
        match r {
            Ok(t) if !t.is_empty() => {
                let _ = writeln!(out, "{t}");
            }
            Ok(_) => {}
            Err(e) => {
                let _ = writeln!(out, "{}", e.trim());
                let _ = server;
            }
        }
    }
    Ok(out)
}

/// `ENDTCP OPTION DELAY ENDSVR`, and `ENDTCPABN` (`OPTION(*IMMED)`):
/// the servers end first, then the stack.
pub fn endtcp(job: &mut Job, host: &Host, r: &Resolved, abnormal: bool) -> Result<String, Fail> {
    let immediate = abnormal || param(r, "OPTION") != "*CNTRLD";
    let endsvr = abnormal || param(r, "ENDSVR") != "*NO";
    let mut out = String::new();
    match ask(host, TcpOp::End { immediate, endsvr, ending: true })? {
        TcpReply::Done { notes, servers } => {
            lines(&notes, &mut out);
            let fw = super::tcpsvr::framework();
    let mut env = super::tcpsvr::JobEnv::new(job, host);
    let mut starter = servers::framework::Starter::new(&fw, &mut env);
            for (_, r) in end_all(&mut starter, &servers, immediate) {
                match r {
                    Ok(t) | Err(t) if !t.trim().is_empty() => {
                        let _ = writeln!(out, "{}", t.trim());
                    }
                    _ => {}
                }
            }
        }
        TcpReply::Failed { note } => return Err(Fail::new(&note.msgid, note.line())),
        other => return Err(Fail::check("", format!("unexpected answer {other:?}"))),
    }
    match ask(host, TcpOp::End { immediate, endsvr, ending: false })? {
        TcpReply::Done { notes, .. } => lines(&notes, &mut out),
        TcpReply::Failed { note } => return Err(Fail::new(&note.msgid, note.line())),
        other => return Err(Fail::check("", format!("unexpected answer {other:?}"))),
    }
    Ok(out)
}

/// A TCP/IP display's data as text, for a job without a display.
pub fn listing(host: &Host, what: &str) -> Result<String, Fail> {
    let data = match ask(host, TcpOp::Panel { what: what.into(), args: Vec::new() })? {
        TcpReply::Panel { data } => data,
        TcpReply::Failed { note } => return Err(Fail::new(&note.msgid, note.line())),
        other => return Err(Fail::check("", format!("unexpected answer {other:?}"))),
    };
    let mut out = String::new();
    for (_, rows) in &data.1 {
        for row in rows {
            let cells: Vec<&str> = row.iter().map(|(_, v)| v.as_str()).collect();
            let _ = writeln!(out, "{}", cells.join("  ").trim_end());
        }
    }
    Ok(out)
}

/// The job-site TCP/IP commands; `None` for another command.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "STRTCP" => strtcp(job, host, r),
        "ENDTCP" => endtcp(job, host, r, false),
        "ENDTCPABN" => endtcp(job, host, r, true),
        "WRKSRVTBLE" => listing(host, "SRV"),
        "WRKPCLTBLE" => listing(host, "PCL"),
        "WRKNETTBLE" => listing(host, "NET"),
        "CFGTCP" | "CFGTCPAPP" | "CFGTCPBP" | "CFGTCPRTD" | "CFGTCPSNMP" | "CFGTCPTELN" | "CFGTCPSMTP"
        | "CFGTCPPTP" => Err(Fail::no_display(&r.name)),
        _ => return None,
    })
}
