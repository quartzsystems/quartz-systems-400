// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Messages` service (docs/QAP.md, version 6) over the functions
//! the message commands use (`stgsvc/msgs.rs`), as the job's signed-on
//! user with their authority checks: listed as `DSPMSG` lists, answered
//! as `SNDRPYMSG` answers (valid replies checked, the sender's copy
//! answered), removed as `RMVMSG` removes, sent as `SNDMSG` sends. The
//! job turns the messages into the client's form, in its language.

use qap_proto::ops::messages::{Queue, Recipient};
use stg::QapRefused;
use wm::obj::msgq::{MsgKind, QMsg};

use super::super::msgs::{self, Refused};
use super::super::served::JobCtx;
use super::{ERROR, INVALID_REQUEST, NOT_AUTHORIZED, connection_job, refused};
use crate::ipl::system::System;

const NOT_FOUND: u16 = 7;

/// The program messages from QAP name as their sender.
const FROM_PGM: &str = "QAPCNN";

/// A refusal as the client is told it: the status by the message, the
/// values for the job to format in its language.
fn failed(r: &Refused, lib: &str, name: &str) -> QapRefused {
    let (status, data): (u16, Vec<String>) = match r.msgid.as_str() {
        "CPF2403" => (NOT_FOUND, vec![name.into(), lib.into()]),
        "CPF2204" => (NOT_FOUND, Vec::new()),
        "CPF2410" | "CPF2422" => (
            if r.msgid == "CPF2410" { NOT_FOUND } else { INVALID_REQUEST },
            if r.data.is_empty() { vec![r.text.clone()] } else { r.data.clone() },
        ),
        "CPF9820" => (NOT_AUTHORIZED, vec![lib.into()]),
        "CPF2189" | "CPF2182" | "CPF9802" | "CPF22E9" => {
            return QapRefused {
                status: NOT_AUTHORIZED,
                msgid: "CPF9802".into(),
                text: String::new(),
                data: vec![format!("{lib}/{name}")],
            };
        }
        _ if !r.data.is_empty() => (ERROR, r.data.clone()),
        _ => return refused(ERROR, &r.msgid, &r.text, &[]),
    };
    QapRefused {
        status,
        msgid: r.msgid.clone(),
        text: String::new(),
        data,
    }
}

/// The signed-on user the job runs as.
fn me(ctx: &JobCtx) -> String {
    ctx.session.borrow().principal.user.clone()
}

/// `user`'s message queue, as its profile names it (library, name);
/// `CPF2204` when there is no such profile.
fn queue_of(system: &mut System, user: &str) -> Result<(String, String), QapRefused> {
    let user = user.trim().to_ascii_uppercase();
    let p = super::super::hostsvr::read(system, &user).ok_or_else(|| refused(NOT_FOUND, "CPF2204", "", &[]))?;
    let (name, lib) = (p.msgq.0.trim().to_ascii_uppercase(), p.msgq.1.trim().to_ascii_uppercase());
    Ok(match name.as_str() {
        "" | "*USRPRF" => ("QUSRSYS".into(), user),
        _ if lib.is_empty() => ("*LIBL".into(), name),
        _ => (lib, name),
    })
}

/// The sender a message from this job names: the job, with the
/// signed-on profile as its user (an entry keeps one user, and the
/// client shows it as who sent the message).
fn sender(system: &System, ctx: &JobCtx) -> (String, String, u32) {
    let (name, _, number) = msgs::sender(system, ctx.job.number);
    (name, me(ctx), number)
}

/// A queue as library and name.
fn resolve(system: &mut System, ctx: &JobCtx, queue: &Queue) -> Result<(String, String), QapRefused> {
    match queue {
        Queue::Own => queue_of(system, &me(ctx)),
        Queue::SystemOperator => Ok(("QSYS".into(), "QSYSOPR".into())),
        Queue::Named { queue } => {
            let lib = match queue.library.trim().to_ascii_uppercase() {
                l if l.is_empty() => "*LIBL".to_string(),
                l => l,
            };
            Ok(msgs::queue_name(&format!("{lib}/{}", queue.name.trim())))
        }
    }
}

fn page(v: Vec<QMsg>, start: u32, max: u32) -> Vec<QMsg> {
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let max = if max == 0 { usize::MAX } else { usize::try_from(max).unwrap_or(usize::MAX) };
    v.into_iter().skip(start).take(max).collect()
}

/// `qap_msg_list`: the queue's messages as `DSPMSG` reads them, in
/// arrival order, paged.
pub fn list(system: &mut System, ctx: &JobCtx, queue: &Queue, newest_first: bool, start: u32, max: u32) -> Result<(u32, Vec<QMsg>), QapRefused> {
    connection_job(ctx)?;
    let (lib, name) = resolve(system, ctx, queue)?;
    let mut all = msgs::with_queue(system, "msgq.receive", &lib, &name, |q, _| {
        Ok((q.by_arrival().into_iter().cloned().collect::<Vec<_>>(), false))
    })
    .map_err(|e| failed(&Refused::of(&e), &lib, &name))?;
    if newest_first {
        all.reverse();
    }
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    Ok((total, page(all, start, max)))
}

/// `qap_msg_reply`: as `SNDRPYMSG`.
pub fn reply(system: &mut System, ctx: &JobCtx, queue: &Queue, key: u32, reply: &str) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let (lib, name) = resolve(system, ctx, queue)?;
    let from = sender(system, ctx);
    msgs::reply(system, &lib, &name, key, reply, from)
        .map(|_| ())
        .map_err(|r| {
            let r = match r.msgid.as_str() {
                "CPF2410" | "CPF2422" => Refused {
                    data: vec![format!("{key:08X}")],
                    ..r
                },
                _ => r,
            };
            failed(&r, &lib, &name)
        })
}

/// `qap_msg_remove`: as `RMVMSG` for each key; a key not on the queue
/// (`CPF2410`) removes none.
pub fn remove(system: &mut System, ctx: &JobCtx, queue: &Queue, keys: &[u32]) -> Result<(), QapRefused> {
    connection_job(ctx)?;
    let (lib, name) = resolve(system, ctx, queue)?;
    msgs::with_queue(system, "msgq.remove", &lib, &name, |q, _| {
        if let Some(k) = keys.iter().find(|k| !q.msgs.iter().any(|m| m.key == **k)) {
            return Err(format!("CPF2410 {k:08X}"));
        }
        for k in keys {
            q.remove(*k).map_err(|e| e.to_string())?;
        }
        Ok(((), !keys.is_empty()))
    })
    .map_err(|e| {
        let r = Refused::of(&e);
        let r = if r.msgid == "CPF2410" {
            Refused {
                data: vec![r.text.clone()],
                ..r
            }
        } else {
            r
        };
        failed(&r, &lib, &name)
    })
}

/// `qap_msg_send`: as `SNDMSG`; an inquiry's reply comes to the user's
/// own queue, the key it will answer returned.
pub fn send(system: &mut System, ctx: &JobCtx, to: &Recipient, text: &str, inquiry: bool) -> Result<Option<u32>, QapRefused> {
    connection_job(ctx)?;
    if text.trim().is_empty() {
        return Err(refused(INVALID_REQUEST, "CPD0071", "", &["MSG"]));
    }
    let (lib, name) = match to {
        Recipient::User { profile } => queue_of(system, profile)?,
        Recipient::Queue { queue } => resolve(system, ctx, queue)?,
    };
    let kind = if inquiry { MsgKind::Inq } else { MsgKind::Info };
    let mut m = msgs::immediate(system, ctx.job.number, kind, text);
    m.job = sender(system, ctx);
    m.from_pgm = FROM_PGM.into();
    if !inquiry {
        return msgs::send(system, &lib, &name, m)
            .map(|_| None)
            .map_err(|e| failed(&Refused::of(&e), &lib, &name));
    }
    let own = queue_of(system, &me(ctx))?;
    msgs::send_inquiry(system, &lib, &name, m, (&own.0, &own.1))
        .map(Some)
        .map_err(|e| failed(&Refused::of(&e), &lib, &name))
}
