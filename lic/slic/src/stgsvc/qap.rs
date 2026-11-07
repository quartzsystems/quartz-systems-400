// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP sessions (docs/QAP.md): sign-on as a 5250 display signs on (the
//! same checks, messages and audit entries, no device), the profile
//! swap, the tokens and their table (`qapd::sessions`), resume, attach,
//! sign off and password changes. Only a QAP connection job (`QAPCNN`) asks,
//! and only the listener (`QAPD`) ends every session. Passwords and
//! tokens are never logged.

pub mod manage;
pub mod messages;
pub mod spool;

use std::io::Read;
use std::sync::{Mutex, OnceLock, PoisonError};

use qap_proto::Token;
use qapd::sessions::{EndedBy, Gone, Sessions};
use qapd::tickets::{TICKET_SECS, Tickets};
use servers::attrs::qap::QapAttrs;
use stg::{QapRefused, QapSession};

use super::hostsvr::{become_user, read, reset};
use super::jobqs::qual;
use super::security::{chg_pwd, principal, sign_on, sign_on_ticket, sysval_text};
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
const BUSY: u16 = 9;
const SESSION_EXPIRED: u16 = 11;

/// `QAP session ended or expired.`
const SESSION_GONE: &str = "TCP1A53";
/// `QAP session of &1 has as many connections as it may have.`
const SESSION_FULL: &str = "TCP1A59";
/// What a 5250 display shows when the password expired.
const EXPIRED: &str = "CPD9BAE";

fn sessions() -> &'static Mutex<Sessions> {
    static S: OnceLock<Mutex<Sessions>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Sessions::new()))
}

fn tickets() -> &'static Mutex<Tickets> {
    static T: OnceLock<Mutex<Tickets>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(Tickets::new()))
}

/// The Telnet server's jobs.
const TELNET: &str = "QTVTELNET";
/// The Access for Web instance's job.
const ACCESSWEB: &str = "ACCESSWEB";
/// `Automatic sign-on is not allowed for user profile &1.`
const NO_AUTO_SIGNON: &str = "TCP1A55";

/// Whether `QRMTSIGN` lets a client sign on automatically.
pub fn remote_signon_allowed(system: &System) -> bool {
    let v = sysval_text(system, "QRMTSIGN").trim().to_ascii_uppercase();
    v.starts_with("*VERIFY") || v.starts_with("*SAMEPRF")
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
        .map_err(|_| {
            refused(
                ERROR,
                "CPF9898",
                "No random numbers for a session token.",
                &[],
            )
        })?;
    Ok(Token(b))
}

/// `IDLETIMO` from the server table.
fn idle_secs(system: &mut System) -> u32 {
    super::tcp::with(system, |_, rt| {
        rt.cfg
            .servers
            .get("*QAP")
            .map(|a| QapAttrs::from_bag(&a.bag).idletimo)
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
        sysval_text(system, "QCCSID")
            .trim()
            .parse()
            .unwrap_or(65535)
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
    let vals: std::collections::HashMap<String, String> = [
        "QSYSLIBL", "QUSRLIBL", "QCCSID", "QLANGID", "QCNTRYID", "QDATFMT", "QDATSEP", "QTIMSEP",
        "QDECFMT",
    ]
    .iter()
    .map(|n| ((*n).to_string(), sysval_text(system, n)))
    .collect();
    let get = |n: &str| {
        vals.get(n)
            .cloned()
            .unwrap_or_else(|| sysval_text(system, n))
    };
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
pub fn signon(
    system: &mut System,
    ctx: &mut JobCtx,
    user: &str,
    password: &str,
) -> Result<QapSession, QapRefused> {
    connection_job(ctx)?;
    if signed_on(ctx).is_some() {
        return Err(refused(
            INVALID_REQUEST,
            "CPF9898",
            "The connection is signed on already.",
            &[],
        ));
    }
    let user = user.trim().to_ascii_uppercase();
    match sign_on(system, &user, password, "", false, None) {
        Err(e) => Err(signon_refused(&e, &user)),
        Ok(s) if s.next == sec::signon::Next::ChangePassword => {
            Err(refused(PASSWORD_EXPIRED, EXPIRED, "", &[]))
        }
        Ok(s) => open(system, ctx, &s.user),
    }
}

/// `qap_chgpwd`.
pub fn chgpwd(
    system: &mut System,
    ctx: &mut JobCtx,
    user: &str,
    old: &str,
    new: &str,
) -> Result<QapSession, QapRefused> {
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
pub fn resume(
    system: &mut System,
    ctx: &mut JobCtx,
    token: &[u8],
) -> Result<QapSession, QapRefused> {
    join(system, ctx, token, false)
}

/// `qap_attach`: as `qap_resume`, but beside the session's other jobs.
pub fn attach(
    system: &mut System,
    ctx: &mut JobCtx,
    token: &[u8],
) -> Result<QapSession, QapRefused> {
    join(system, ctx, token, true)
}

/// Resume or attach: the job runs as the session's profile with the
/// library list of its job description, its other state its own.
fn join(
    system: &mut System,
    ctx: &mut JobCtx,
    token: &[u8],
    attach: bool,
) -> Result<QapSession, QapRefused> {
    connection_job(ctx)?;
    if signed_on(ctx).is_some() {
        return Err(refused(
            INVALID_REQUEST,
            "CPF9898",
            "The connection is signed on already.",
            &[],
        ));
    }
    let gone = || refused(SESSION_EXPIRED, SESSION_GONE, "", &[]);
    let token = token_of(token).ok_or_else(gone)?;
    let alive = |n: u32| system.jobs.get(n).is_some();
    let (job, now) = (ctx.job.number, now_unix());
    let mut table = sessions().lock().unwrap_or_else(PoisonError::into_inner);
    let r = if attach {
        table.attach(&token, job, now, &alive)
    } else {
        table.resume(&token, job, now, &alive)
    };
    let s = match r {
        Ok(s) => s,
        Err(Gone::Full) => {
            let user = table
                .get(&token)
                .map(|s| s.user.clone())
                .unwrap_or_default();
            return Err(refused(BUSY, SESSION_FULL, "", &[&user]));
        }
        Err(_) => return Err(gone()),
    };
    drop(table);
    become_user(system, ctx, &s.user);
    libl_of(system, ctx, &s.user);
    Ok(describe(system, &s.user, &token, s.idle_secs))
}

/// `qap_signoff`; `operator` when an operator ended this job. The
/// session's other jobs hear of it at their next `qap_ended`.
pub fn signoff(system: &mut System, ctx: &mut JobCtx, token: &[u8], operator: bool) {
    if ctx.job.name != CONN_JOB {
        return;
    }
    if let Some(t) = token_of(token) {
        let why = if operator {
            EndedBy::Operator
        } else {
            EndedBy::SignOff
        };
        sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .end(&t, ctx.job.number, why);
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

/// `qap_ended`: whether another job ended this job's session, and
/// whether an operator did; told once.
pub fn ended(ctx: &JobCtx) -> Option<bool> {
    if ctx.job.name != CONN_JOB {
        return None;
    }
    sessions()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take_ended(ctx.job.number)
        .map(|by| by == EndedBy::Operator)
}

/// `qap_end_all`.
pub fn end_all(ctx: &JobCtx) {
    if ctx.job.name == LISTENER {
        sessions()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// `qap_pwd_status`: the profile's password expiry (its interval, else
/// `QPWDEXPITV`, from the last change; now when it is set expired) and
/// `QPWDEXPWRN`.
pub fn pwd_status(system: &mut System, ctx: &JobCtx) -> (Option<i64>, u16) {
    let user = ctx.session.borrow().principal.user.clone();
    let policy = sec::password::policy::Policy::from_sysvals(&|n: &str| sysval_text(system, n));
    let warn = u16::try_from(policy.expwrn).unwrap_or(u16::MAX);
    let Some(p) = read(system, &user) else {
        return (None, warn);
    };
    let ms = |secs: u64| i64::try_from(secs).unwrap_or(i64::MAX).saturating_mul(1000);
    let expires = if p.pwdexp {
        Some(ms(now_unix()))
    } else {
        policy
            .interval(&p)
            .map(|days| ms(p.pwd_changed.saturating_add(u64::from(days) * 86_400)))
    };
    (expires, warn)
}

/// `qap_ticket`: 256 random bits for the session's own profile, kept
/// here a minute. `NotAuthorized` when `QRMTSIGN` allows no automatic
/// sign-on or the profile could not sign on (disabled, `*NONE`
/// password, password expired).
pub fn ticket(system: &mut System, ctx: &JobCtx) -> Result<(Vec<u8>, u32), QapRefused> {
    // A QAP connection job, or Access for Web's job running as the user
    // who signed on to it with a password (its 5250 auto sign-on).
    if ctx.job.name != ACCESSWEB {
        connection_job(ctx)?;
    }
    let user = signed_on(ctx).ok_or_else(|| refused(3, "CPF9898", "Not signed on.", &[]))?;
    let no = || refused(NOT_AUTHORIZED, NO_AUTO_SIGNON, "", &[&user]);
    if !remote_signon_allowed(system) {
        return Err(no());
    }
    if !may_sign_on(system, &user) {
        return Err(no());
    }
    let token = new_token()?;
    tickets()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .issue(&token, &user, now_unix());
    sec::audit::record(
        sec::audit::Event::new(sec::audit::Kind::Ps)
            .set("ETYP", "P")
            .set("USRP", user.as_str())
            .set("NUSR", user.as_str()),
    );
    Ok((token.0.to_vec(), TICKET_SECS))
}

/// Whether `user` may sign on automatically now: enabled, a password,
/// not expired.
fn may_sign_on(system: &mut System, user: &str) -> bool {
    let Some(p) = read(system, user) else {
        return false;
    };
    let policy = sec::password::policy::Policy::from_sysvals(&|n: &str| sysval_text(system, n));
    sec::signon::automatic_allowed(&p, &policy, now_unix())
}

/// `telnet_ticket`: used up here whatever the answer; `QRMTSIGN` and
/// the profile as at issue (`qapd::tickets::check`), so an expired
/// password signs nothing on; then the sign-on checks for `user`, all
/// but the password. Refused as a wrong password reads, so a ticket
/// says nothing about a profile.
pub fn telnet_ticket(
    system: &mut System,
    ctx: &JobCtx,
    user: &str,
    ticket: &[u8],
    device: &str,
    tls: bool,
) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
    let not = || sec::signon::Refused {
        msgid: "CPF1107".into(),
        text: "Password or user profile is not correct.".into(),
        vary_off: false,
    };
    if ctx.job.name != TELNET {
        return Err(not());
    }
    let user = user.trim().to_ascii_uppercase();
    let token = token_of(ticket).ok_or_else(not)?;
    let allowed = remote_signon_allowed(system);
    let r = qapd::tickets::check(
        &mut tickets().lock().unwrap_or_else(PoisonError::into_inner),
        &token,
        &user,
        now_unix(),
        tls,
        allowed,
        |u| may_sign_on(system, u),
    );
    sec::audit::record(
        sec::audit::Event::new(sec::audit::Kind::Ps)
            .set("ETYP", "V")
            .set("USRP", user.as_str())
            .set("NUSR", user.as_str())
            .set("RSLT", if r.is_ok() { "A" } else { "R" }),
    );
    if r.is_err() {
        return Err(not());
    }
    sign_on_ticket(system, &user, device)
}

/// The Telnet server's own jobs: `QTVTELNET` and its sessions (`QTVDEVICE`).
fn telnet_job(ctx: &JobCtx) -> bool {
    let ok = ctx.job.name == TELNET || ctx.job.name == "QTVDEVICE";
    if !ok {
        eprintln!("qsstg: telnet sign-on asked by job {}", ctx.job.name);
    }
    ok
}

/// `telnet_verify`: whether `proof` is `user`'s password, before the
/// device exists (the `INIT0100` "client password validated"). Nothing
/// is changed: the sign-on that follows counts a wrong one.
pub fn telnet_verify(
    system: &mut System,
    ctx: &JobCtx,
    user: &str,
    proof: &sec::signon::Substitute,
) -> bool {
    if !telnet_job(ctx) {
        return false;
    }
    let user = user.trim().to_ascii_uppercase();
    let Some(p) = read(system, &user) else {
        return false;
    };
    if p.disabled || p.pwd_none {
        return false;
    }
    let values = sec::signon::Values::from_sysvals(&|n: &str| sysval_text(system, n));
    sec::password::verify_substitute(&p, &user, proof, values.policy.pwdlvl)
}

/// `telnet_exit_signon`: an `INIT0100` exit program allowed automatic
/// sign-on as `user`; every sign-on check but the password, as IBM's
/// "anonymous" Telnet does. Only `QTVTELNET` may ask.
pub fn telnet_exit_signon(
    system: &mut System,
    ctx: &JobCtx,
    user: &str,
    device: &str,
) -> Result<sec::signon::SignedOn, sec::signon::Refused> {
    if !telnet_job(ctx) {
        return Err(sec::signon::Refused {
            msgid: "CPF2207".into(),
            text: format!(
                "Job {} may not sign a user on for an exit program.",
                ctx.job.name
            ),
            vary_off: false,
        });
    }
    let user = user.trim().to_ascii_uppercase();
    sec::audit::record(
        sec::audit::Event::new(sec::audit::Kind::Ps)
            .set("ETYP", "V")
            .set("USRP", user.as_str())
            .set("NUSR", user.as_str())
            .set("RSLT", "A"),
    );
    sign_on_ticket(system, &user, device)
}
