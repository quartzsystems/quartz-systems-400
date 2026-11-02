// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RunCommand` and `CheckCommand` in the connection job: the analyzer
//! Command Entry uses, as the signed-on profile with its library list.
//! The messages a command sent its caller are the job log's entries it
//! added. A batch job's analyzer refuses commands allowed only
//! interactively (`CPD0031`), so they end with `Error`.

use qap_proto::ops::command::CommandReply;
use qap_proto::types::JobId;
use qap_proto::{ServerMessage, Status};
use qapd::Ran;

use crate::command::RunMode;
use crate::console::Console;
use crate::sysdsp::keys;

/// The program the job log names as the requester.
const PGM: (&str, &str) = ("QAPCNN", "QSYS");

/// Escapes and diagnostics that mean the command or what it names is
/// not there.
const NOT_FOUND: &[&str] = &["CPD0030", "CPF9801", "CPF2105", "CPF2110", "CPF2125"];
/// ... and that the profile may not use it.
const NOT_AUTHORIZED: &[&str] = &["CPF9802", "CPF2189", "CPF2207", "CPD0033", "CPF22E9", "CPF2217"];

/// A job log entry as the client sees it.
fn message_of(e: &wm::joblog::Entry) -> ServerMessage {
    ServerMessage {
        id: e.msgid.clone(),
        severity: u8::try_from(e.severity).unwrap_or(99),
        data: Vec::new(),
        text: e.text.clone(),
        help: e.help.clone(),
    }
}

/// The messages the job log gained since entry `before`; requests left
/// out.
fn messages_since(console: &mut Console, before: usize) -> Vec<ServerMessage> {
    console
        .job()
        .joblog
        .entries()
        .skip(before)
        .filter(|e| e.kind != "Request")
        .map(message_of)
        .collect()
}

/// The status a command's messages give.
fn status_of(ids: &[&str]) -> Status {
    if ids.contains(&keys::ENDED) {
        Status::Cancelled
    } else if ids.iter().any(|id| NOT_FOUND.contains(id)) {
        Status::NotFound
    } else if ids.iter().any(|id| NOT_AUTHORIZED.contains(id)) {
        Status::NotAuthorized
    } else {
        Status::Error
    }
}

/// The job, as `JobId`.
fn job_id(console: &mut Console) -> JobId {
    let j = console.job();
    JobId {
        name: j.ident.name.clone(),
        user: j.ident.user.clone(),
        number: j.ident.number.clone(),
    }
}

/// Run `command`.
pub fn run(console: &mut Console, command: &str) -> Ran {
    // A cancel asked before this command started is not for it.
    let _ = keys::take_outside_end();
    let _request = keys::Request::enter();
    let before = console.job().joblog.len();
    console.job().log_request(command, PGM);
    let o = console.run(command, RunMode::Run);
    let messages = messages_since(console, before);
    let job = job_id(console);
    let Some(f) = o.escape else {
        return Ran {
            status: Status::Ok,
            escape: None,
            reply: CommandReply { job, messages },
        };
    };
    let mut ids = vec![f.msgid.as_str()];
    if let Some(d) = &o.diag {
        ids.push(d.msgid.as_str());
    }
    let status = status_of(&ids);
    let escape = messages
        .iter()
        .rev()
        .find(|m| m.id == f.msgid)
        .cloned()
        .unwrap_or_else(|| ServerMessage {
            id: f.msgid.clone(),
            severity: 40,
            data: Vec::new(),
            text: f.text.clone(),
            help: String::new(),
        });
    Ran {
        status,
        escape: Some(escape),
        reply: CommandReply { job, messages },
    }
}

/// Check `command` without running it.
pub fn check(console: &mut Console, command: &str) -> (Status, Vec<ServerMessage>) {
    let before = console.job().joblog.len();
    let o = console.run(command, RunMode::Check);
    let messages = messages_since(console, before);
    match o.escape {
        None => (Status::Ok, Vec::new()),
        Some(f) => {
            let mut ids = vec![f.msgid.as_str()];
            if let Some(d) = &o.diag {
                ids.push(d.msgid.as_str());
            }
            let status = match status_of(&ids) {
                Status::NotFound => Status::NotFound,
                _ => Status::Error,
            };
            (status, messages)
        }
    }
}
