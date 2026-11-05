// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL CLI server mode: a job asks for a ticket ([`ticket`]) and hands
//! it to the `QSQSRVR` prestart job that takes its connection; that job
//! proves the ticket ([`assume`]) and runs as the job's user (or as the
//! user `SQLConnect` named, its password checked as a sign-on), its
//! `QSYS2.SERVER_MODE_JOB_NAME` the client's name, until the
//! connection ends ([`end`]).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use stg::HostRefused;

use super::served::JobCtx;
use crate::ipl::system::System;

/// How long a ticket is good for.
const TICKET_SECS: u64 = 120;

/// The job a server job serves.
pub const SERVER_JOB: &str = "QSQSRVR";

/// A ticket's client: its qualified name and current user.
struct Ticket {
    job: String,
    user: String,
    until: u64,
}

fn tickets() -> &'static Mutex<HashMap<Vec<u8>, Ticket>> {
    static T: OnceLock<Mutex<HashMap<Vec<u8>, Ticket>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A job's qualified name, `number/user/name`.
#[must_use]
pub fn qualified(job: &stg::JobInfo) -> String {
    format!("{:06}/{}/{}", job.number, job.user.trim(), job.name.trim())
}

/// `sqlsrv_ticket`: a ticket for the calling job and its current user.
pub fn ticket(ctx: &JobCtx) -> Vec<u8> {
    let mut t = sec::password::fresh_salt().to_vec();
    t.extend(sec::password::fresh_salt());
    let mut all = tickets().lock().unwrap_or_else(PoisonError::into_inner);
    let at = now();
    all.retain(|_, x| x.until > at);
    all.insert(t.clone(), Ticket { job: qualified(&ctx.job), user: ctx.session.borrow().principal.user.clone(), until: at + TICKET_SECS });
    t
}

/// A refusal with its message's text.
fn refused(system: &mut System, msgid: &str, values: &[&str]) -> HostRefused {
    HostRefused { rc: 1, msgid: msgid.into(), text: super::msgs::text(system, msgid, values) }
}

/// `sqlsrv_assume`: the calling `QSQSRVR` prestart job takes the
/// ticket's connection; it runs as the client's user or as `user`. The
/// client's qualified name back.
pub fn assume(system: &mut System, ctx: &mut JobCtx, ticket: &[u8], user: &str, password: &str) -> Result<String, HostRefused> {
    if ctx.job.name.trim() != SERVER_JOB || ctx.job.subtype != 'J' {
        // Only a QSQSRVR prestart job serves SQL connections.
        let user = ctx.job.user.clone();
        return Err(refused(system, "CPF2217", &[&user]));
    }
    let t = tickets().lock().unwrap_or_else(PoisonError::into_inner).remove(ticket);
    let Some(t) = t.filter(|t| t.until > now()) else {
        return Err(refused(system, "CPF4AB8", &[]));
    };
    if user.trim().is_empty() {
        super::hostsvr::become_user(system, ctx, &t.user);
    } else {
        super::ddm::signon(system, ctx, user, password, true)?;
    }
    ctx.sql = None;
    ctx.sql_client = Some(t.job.clone());
    Ok(t.job)
}

/// `sqlsrv_end`: the connection ended; the job's SQL goes and it runs
/// as its own user again.
pub fn end(system: &mut System, ctx: &mut JobCtx) {
    ctx.sql = None;
    ctx.sql_client = None;
    super::hostsvr::reset(system, ctx);
}
