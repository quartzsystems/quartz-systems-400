// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The processing programs of the system commands that run in the
//! storage service. A command's parameter block arrives decoded as
//! [`Parms`]; the processor either has a direct implementation here
//! (`direct`) or, through the bridge, hands the keyword form to the
//! library function of Phases 2 to 6 behind the console word the
//! inventory names. Output text goes back to the job; a first line that
//! starts with a message identifier is the escape that ended the
//! command, and a command that succeeded sends the completion message
//! the inventory gives it.

pub(crate) mod certcmd;
pub(crate) mod clpgm;
mod cmncmd;
mod cmntrc;
mod dbcmd;
mod direct;
mod ifscmd;
pub(crate) mod lpapi;
pub(crate) mod lpcmd;
pub(crate) mod iplcmd;
mod itgcmd;
mod medcmd;
mod msgfcmd;
mod objcmd;
mod prbcmd;
pub(crate) mod ptfcmd;
pub(crate) mod ptflic;
mod ptfsvc;
mod rpg;
mod savcmd;
mod savout;
pub(crate) mod svccmd;
pub(crate) mod stmf;
mod stmfcmd;
mod sysinfo;
mod tcpcmd;
mod uicmd;
pub(crate) mod updcmd;

use cmd::block::Parms;
use stg::proto::{CmdMsg, CmdReply};
use sys_cpps::{Entry, Site};

use crate::ipl::system::System;

pub use prbcmd::{detail_data as problem_detail, rows as problem_rows};
pub use savcmd::savf_listing;

/// Run command `name` with `parms`; the text, messages and escape.
/// `typed` is the keyword form as the caller typed it, for the bridge;
/// empty when the block is all there is.
pub fn run(system: &mut System, name: &str, parms: &Parms, typed: &str) -> CmdReply {
    let Some(e) = sys_cpps::entry(name) else {
        return escape(
            "CPF0006",
            name,
            &format!("{name} has no processing program here"),
        );
    };
    match e.site {
        Site::Stub => {
            let id = e.stub.clone().unwrap_or_else(|| "CPF3700".into());
            escape(&id, name, "")
        }
        Site::Job if !sys_cpps::inventory::in_service_too(name) => escape(
            "CPF0006",
            name,
            &format!("{name} runs in the job, not the service"),
        ),
        Site::Job | Site::Service => tcpcmd::run(system, name, parms)
            .or_else(|| direct::run(system, name, parms))
            .or_else(|| savcmd::run(system, name, parms))
            .or_else(|| medcmd::run(system, name, parms))
            .or_else(|| ptfcmd::run(system, name, parms))
            .or_else(|| updcmd::run(system, name, parms))
            .or_else(|| ptfsvc::run(system, name, parms))
            .or_else(|| iplcmd::run(system, name, parms))
            .or_else(|| itgcmd::run(system, name, parms))
            .or_else(|| prbcmd::run(system, name, parms))
            .or_else(|| svccmd::run(system, name, parms))
            .or_else(|| lpcmd::run(system, name, parms))
            .or_else(|| ifscmd::run(system, name, parms))
            .or_else(|| stmfcmd::run(system, name, parms))
            .or_else(|| msgfcmd::run(system, name, parms))
            .or_else(|| dbcmd::run(system, name, parms))
            .or_else(|| objcmd::run(system, name, parms))
            .or_else(|| cmncmd::run(system, name, parms))
            .or_else(|| cmntrc::run(system, name, parms))
            .or_else(|| certcmd::run(system, name, parms))
            .or_else(|| sysinfo::run(system, name, parms))
            .or_else(|| crate::stgsvc::regfac::command(system, name, parms))
            .unwrap_or_else(|| bridge(system, e, parms, typed)),
    }
}

/// The bridge: the keyword form into the console table, as typed when
/// the caller typed it.
fn bridge(system: &mut System, e: &Entry, parms: &Parms, typed: &str) -> CmdReply {
    let line = if typed.is_empty() {
        sys_cpps::bridge::specified_line(&e.word, parms)
    } else {
        typed.to_string()
    };
    let text = crate::stgsvc::exec::run(system, &line);
    classify(text, e, parms)
}

/// Output into a reply: a leading message identifier is the escape;
/// otherwise the inventory's completion message is sent.
pub fn classify(text: String, e: &Entry, parms: &Parms) -> CmdReply {
    let first = text.lines().next().unwrap_or("").trim();
    if let Some(id) = leading_msgid(first) {
        if id.starts_with("CPC") || id.starts_with("CPI") {
            return CmdReply {
                text,
                ..CmdReply::default()
            };
        }
        return CmdReply {
            escape: Some(CmdMsg {
                msgid: id.to_string(),
                kind: 7,
                data: Vec::new(),
            }),
            text,
            ..CmdReply::default()
        };
    }
    let mut reply = CmdReply {
        text,
        ..CmdReply::default()
    };
    if let Some((id, kwds)) = &e.comp {
        let fields: Vec<(String, usize)> = kwds.iter().map(|k| (parms.text(k), 10)).collect();
        let refs: Vec<(&str, usize)> = fields.iter().map(|(t, n)| (t.as_str(), *n)).collect();
        reply.messages.push(CmdMsg {
            msgid: id.clone(),
            kind: 4,
            data: cmd::error::data(&refs),
        });
    }
    reply
}

/// A reply that is one escape.
pub fn escape(msgid: &str, name: &str, detail: &str) -> CmdReply {
    CmdReply {
        text: if detail.is_empty() {
            String::new()
        } else {
            format!("{msgid} {detail}\n")
        },
        escape: Some(CmdMsg {
            msgid: msgid.into(),
            kind: 7,
            data: cmd::error::data(&[(name, 10)]),
        }),
        ..CmdReply::default()
    }
}

/// A reply that is text and a completion message.
pub fn done(text: String, msgid: &str, fields: &[(&str, usize)]) -> CmdReply {
    CmdReply {
        text,
        messages: vec![CmdMsg {
            msgid: msgid.into(),
            kind: 4,
            data: cmd::error::data(fields),
        }],
        ..CmdReply::default()
    }
}

/// `AAAnnnn` at the start of a line, followed by a blank, a colon or
/// the end.
#[must_use]
pub fn leading_msgid(line: &str) -> Option<&str> {
    let b = line.as_bytes();
    if b.len() < 7 {
        return None;
    }
    let ok = b[..3].iter().all(u8::is_ascii_uppercase)
        && b[3..7].iter().all(u8::is_ascii_hexdigit)
        && b.get(7).is_none_or(|c| *c == b' ' || *c == b':');
    ok.then(|| &line[..7])
}
