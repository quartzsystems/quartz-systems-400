// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP sessions (docs/QAP.md): sign-on as a 5250 display signs on (the
//! same checks, messages and audit entries, no device), the profile
//! swap, the tokens and their table (`qapd::sessions`), resume, sign
//! off and password changes. Only a QAP connection job (`QAPCNN`) asks,
//! and only the listener (`QAPD`) ends every session. Passwords and
//! tokens are never logged.

use std::io::Read;
use std::sync::{Mutex, OnceLock, PoisonError};

use qap_proto::Token;
use qapd::sessions::Sessions;
use servers::attrs::qap::QapAttrs;
use stg::{QapRefused, QapSession};

use super::hostsvr::{become_user, read, reset};
use super::jobqs::qual;
use super::security::{chg_pwd, principal, sign_on, sysval_text};
use super::served::JobCtx;
use crate::ipl::system::{System, now_unix};

/// The connection jobs' name.
const CONN_JOB: &str = "QAPCNN";
/// The listener's name.
const LISTENER: &str = "QAPD";

// docs/QAP.md's status values.
const ERROR: u16 = 1;
const SIGNON_FAILED: u16 = 4;
const PASSWORD_EXPIRED: u16 = 5;
const NOT_AUTHORIZED: u16 = 6;
const INVALID_REQUEST: u16 = 8;
const SESSION_EXPIRED: u16 = 11;

/// `QAP session ended or expired.`
const SESSION_GONE: &str = "TCP1A53";
/// What a 5250 display shows when the password expired.
const EXPIRED: &str = "CPD9BAE";

fn sessions() -> &'static Mutex<Sessions> {
    static S: OnceLock<Mutex<Sessions>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Sessions::new()))
}

fn refused(status: u16, msgid: &str, text: &str, data: &[&str]) -> QapRefused {
    QapRefused {
        status,
        msgid: msgid.to_string(),
        text: text.to_string(),
        data: data.iter().map(ToString::to_string).collect(),
    }
}

/// Only a connection job may ask.
fn connection_job(ctx: &JobCtx) -> Result<(), QapRefused> {
    if ctx.job.name == CONN_JOB {
        Ok(())
    } else {
        Err(refused(NOT_AUTHORIZED, "CPF2207", "", &[]))
    }
}

/// The job's user when it has signed on; `None` while it runs as its own.
fn signed_on(ctx: &JobCtx) -> Option<String> {
    let now = ctx.session.borrow().principal.user.clone();
    (now != ctx.job.user).then_some(now)
}

/// 256 random bits; no fallback, a token is never predictable.
fn new_token() -> Result<Token, QapRefused> {
    let mut b = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .map_err(|_| refused(ERROR, "CPF9898", "No random numbers for a session token.", &[]))?;
    Ok(Token(b))
}

/// `IDLETIMO` from the server table.
fn idle_secs(system: &mut System) -> u32 {
    super::tcp::with(system, |_, rt| {
        rt.cfg.servers.get("*QAP").map(|a| QapAttrs::from_bag(&a.bag).idletimo)
    })
    .ok()
    .flatten()
    .unwrap_or(servers::attrs::qap::IDLETIMO_DFT)
}

/// A refused sign-on as the display shows it: its message and text,
/// the profile its message names.
fn signon_refused(e: &sec::signon::Refused, user: &str) -> QapRefused {
    let data: &[&str] = match e.msgid.as_str() {
        "CPF1394" | "CPF1118" | "CPF1393" => &[user],
        _ => &[],
    };
    refused(SIGNON_FAILED, &e.msgid, &e.text, data)
}

/// The session's description for `user`.
fn describe(system: &mut System, user: &str, token: &Token, idle: u32) -> QapSession {
    let Some(p) = read(system, user) else {
        return QapSession {
            token: token.0.to_vec(),
            idle_secs: idle,
            profile: user.to_string(),
            ..QapSession::default()
        };
    };
    let ccsid = if p.ccsid == 0 {
        sysval_text(system, "QCCSID").trim().parse().unwrap_or(65535)
    } else {
        u16::try_from(p.ccsid).unwrap_or(65535)
    };
    let home = p.homedir.trim();
    QapSession {
        token: token.0.to_vec(),
        idle_secs: idle,
        profile: user.to_string(),
        text: p.text.trim().to_string(),
        ccsid,
        date_format: sysval_text(system, "QDATFMT").trim().to_string(),
        date_separator: sysval_text(system, "QDATSEP").trim().to_string(),
        time_separator: sysval_text(system, "QTIMSEP").trim().to_string(),
        home_directory: if home.is_empty() || home.starts_with('*') {
            format!("/home/{user}")
        } else {
            home.to_string()
        },
        special_authorities: p.spcaut.names().into_iter().map(str::to_string).collect(),
    }
}

/// The job's library list and current library from `user`'s job
/// description, as a job of the user's starts with.
fn libl_of(system: &mut System, ctx: &JobCtx, user: &str) {
    let Some(p) = read(system, user) else {
        return;
    };
    let (name, lib) = &p.jobd;
    let jobd = qual(&format!("{lib}/{name}"));
    let Ok(b) = super::sbs::wm_object(system, &jobd, "*JOBD") else {
        return;
    };
    let Ok(d) = wm::obj::jobd::JobD::decode(&b) else {
        return;
    };
    let prf = super::jobs::user_dft(system, user);
    let vals: std::collections::HashMap<String, String> = ["QSYSLIBL", "QUSRLIBL", "QCCSID", "QLANGID", "QCNTRYID", "QDATFMT", "QDATSEP", "QTIMSEP", "QDECFMT"]
        .iter()
        .map(|n| ((*n).to_string(), sysval_text(system, n)))
        .collect();
    let get = |n: &str| vals.get(n).cloned().unwrap_or_else(|| sysval_text(system, n));
    let a = wm::job::JobAttrs::resolve_for(&jobd, &d, &wm::obj::cls::Cls::default(), &get, &prf);
    if let Some(mine) = system.attrs.get_mut(&ctx.job.number) {
        mine.libl.usr = a.libl.usr;
        mine.libl.cur = a.libl.cur;
    }
}

/// Run as `user` in a new session attached to this job.
fn open(system: &mut System, ctx: &mut JobCtx, user: &str) -> Result<QapSession, QapRefused> {
    let token = new_token()?;
    let idle = idle_secs(system);
    become_user(system, ctx, user);
    libl_of(system, ctx, user);
    sessions()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .create(&token, user, idle, ctx.job.number);
    Ok(describe(system, user, &token, idle))
}

/// `qap_signon`.
pub fn signon(system: &mut System, ctx: &mut JobCtx, user: &str, password: &str) -> Result<QapSession, QapRefused> {
    connection_job(ctx)?;
    if signed_on(ctx).is_some() {
        return Err(refused(INVALID_REQUEST, "CPF9898", "The connection is signed on already.", &[]));
    }
    let user = user.trim().to_ascii_uppercase();
    match sign_on(system, &user, password, "", false, None) {
        Err(e) => Err(signon_refused(&e, &user)),
        Ok(s) if s.next == sec::signon::Next::ChangePassword => Err(refused(PASSWORD_EXPIRED, EXPIRED, "", &[])),
        Ok(s) => open(system, ctx, &s.user),
    }
}

/// `qap_chgpwd`.
pub fn chgpwd(system: &mut System, ctx: &mut JobCtx, user: &str, old: &str, new: &str) -> Result<QapSession, QapRefused> {
    connection_job(ctx)?;
    let user = user.trim().to_ascii_uppercase();
    let already = signed_on(ctx);
    match &already {
        Some(me) if *me != user => return Err(refused(NOT_AUTHORIZED, "CPF22E9", "", &[&user])),
        Some(_) => {}
        // Not signed on: the old password is checked as a sign-on is.
        None => {
            if let Err(e) = sign_on(system, &user, old, "", false, None) {
                return Err(signon_refused(&e, &user));
            }
        }
    }
    let target = principal(system, &user);
    chg_pwd(system, &target, old, new, false).map_err(|e| {
        let id = e
            .split_whitespace()
            .next()
            .filter(|w| w.len() == 7 && w.starts_with("CP"))
            .unwrap_or("CPF22E2");
        let text = e.strip_prefix(id).unwrap_or(&e).trim();
        refused(ERROR, id, text, &[])
    })?;
    if already.is_some() {
        // The session goes on; its token stays with the connection.
        let idle = idle_secs(system);
        let mut s = describe(system, &user, &Token::default(), idle);
        s.token.clear();
        return Ok(s);
    }
    open(system, ctx, &user)
}

fn token_of(b: &[u8]) -> Option<Token> {
    b.try_into().ok().map(Token)
}

/// `qap_resume`.
pub fn resume(system: &mut System, ctx: &mut JobCtx, token: &[u8]) -> Result<QapSession, QapRefused> {
    connection_job(ctx)?;
    if signed_on(ctx).is_some() {
        return Err(refused(INVALID_REQUEST, "CPF9898", "The connection is signed on already.", &[]));
    }
    let gone = || refused(SESSION_EXPIRED, SESSION_GONE, "", &[]);
    let token = token_of(token).ok_or_else(gone)?;
    let alive = |n: u32| system.jobs.get(n).is_some();
    let s = sessions()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .resume(&token, ctx.job.number, now_unix(), &alive)
        .map_err(|_| gone())?;
    become_user(system, ctx, &s.user);
    libl_of(system, ctx, &s.user);
    Ok(describe(system, &s.user, &token, s.idle_secs))
}

/// `qap_signoff`.
pub fn signoff(system: &mut System, ctx: &mut JobCtx, token: &[u8]) {
    if ctx.job.name != CONN_JOB {
        return;
    }
    if let Some(t) = token_of(token) {
        sessions().lock().unwrap_or_else(PoisonError::into_inner).end(&t);
    }
    reset(system, ctx);
    let own = ctx.job.user.clone();
    libl_of(system, ctx, &own);
}

/// `qap_detach`.
pub fn detach(ctx: &JobCtx, token: &[u8]) {
    if ctx.job.name != CONN_JOB {
        return;
    }
    if let Some(t) = token_of(token) {
        sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .detach(&t, ctx.job.number, now_unix());
    }
}

/// `qap_end_all`.
pub fn end_all(ctx: &JobCtx) {
    if ctx.job.name == LISTENER {
        sessions().lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}
