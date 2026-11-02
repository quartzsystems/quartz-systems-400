// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RunCommand`, `CheckCommand` and `DescribeCommand` in the connection
//! job: the analyzer Command Entry uses, as the signed-on profile with
//! its library list, and the definition `F4` prompts from.
//! The messages a command sent its caller are the job log's entries it
//! added. A batch job's analyzer refuses commands allowed only
//! interactively (`CPD0031`), so they end with `Error`.

use cmd::analyze::CmdSource;
use cmd::prompt::flat;
use qap_proto::ops::command::{CommandDefinition, CommandReply, ParmKind, ParmNode, Range, Role, Show};
use qap_proto::types::{JobId, ObjectName};
use qap_proto::{ServerMessage, Status};
use qapd::Ran;

use crate::command::RunMode;
use crate::console::Console;
use crate::sysdsp::keys;

/// The program the job log names as the requester.
const PGM: (&str, &str) = ("QAPCNN", "QSYS");

/// Escapes and diagnostics that mean the command or what it names is
/// not there.
const NOT_FOUND: &[&str] = &["CPD0030", "CPF9801", "CPF2105", "CPF2110"];
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

/// Run `command`; `cancelled` says whether the client cancelled this
/// request.
pub fn run(console: &mut Console, command: &str, cancelled: &dyn Fn() -> bool) -> Ran {
    // An end asked for an earlier request is not for this one; one asked
    // for this request before it started is.
    let _ = keys::take_outside_end();
    if cancelled() {
        keys::end_from_outside();
    }
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

fn node_of(n: flat::Node) -> ParmNode {
    let u16_of = |i: usize| u16::try_from(i).unwrap_or(u16::MAX);
    ParmNode {
        index: u16_of(n.index),
        parent: n.parent.map(u16_of),
        role: match n.role {
            flat::Role::Parameter => Role::Parameter,
            flat::Role::Element => Role::Element,
            flat::Role::Qualifier => Role::Qualifier,
        },
        keyword: n.keyword,
        prompt: n.prompt,
        // No help panel groups ship for commands; F4 shows none either.
        help: String::new(),
        kind: match n.kind {
            flat::Kind::Name => ParmKind::Name,
            flat::Kind::Character => ParmKind::Character,
            flat::Kind::Decimal => ParmKind::Decimal,
            flat::Kind::Integer => ParmKind::Integer,
            flat::Kind::Logical => ParmKind::Logical,
            flat::Kind::Date => ParmKind::Date,
            flat::Kind::Time => ParmKind::Time,
            flat::Kind::Generic => ParmKind::Generic,
            flat::Kind::Path => ParmKind::Path,
            flat::Kind::Command => ParmKind::Command,
        },
        length: n.length,
        decimals: u16::from(n.decimals),
        required: n.required,
        min_values: n.min,
        max_values: n.max,
        default: n.default,
        special_values: n.special,
        values: n.values,
        range: n.range.map(|(low, high)| Range { low, high }),
        show: match n.show {
            flat::Show::Always => Show::Always,
            flat::Show::Additional => Show::Additional,
            flat::Show::Never => Show::Never,
        },
    }
}

/// `command`'s definition from its `*CMD` object, `CHGCMDDFT`'s
/// defaults applied; why not as a status, message and its data.
pub fn describe(console: &mut Console, command: &ObjectName) -> Result<CommandDefinition, (Status, String, Vec<String>)> {
    let name = command.name.trim().to_ascii_uppercase();
    let lib = command.library.trim().to_ascii_uppercase();
    let lib_arg = (!lib.is_empty() && lib != "*LIBL").then_some(lib.as_str());
    let shown_lib = if lib.is_empty() { "*LIBL".to_string() } else { lib.clone() };
    let found = console.host.cmds.borrow_mut().lookup(&name, lib_arg);
    match found {
        Ok(Some((obj, found_lib))) => {
            let def = obj.effective();
            Ok(CommandDefinition {
                name: ObjectName {
                    library: found_lib,
                    name: def.name.clone(),
                },
                text: def.prompt.clone(),
                help: String::new(),
                nodes: flat::nodes(&def).into_iter().map(node_of).collect(),
            })
        }
        Ok(None) => Err((Status::NotFound, "CPD0030".into(), vec![name, shown_lib])),
        Err(e) => {
            let id = e.msgid().to_string();
            let status = if NOT_AUTHORIZED.contains(&id.as_str()) {
                Status::NotAuthorized
            } else {
                Status::NotFound
            };
            Err((status, id, vec![name, shown_lib]))
        }
    }
}
