// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing on for the host servers: a password substitute or a profile
//! token checked as a sign-on is (`QMAXSIGN`, audit), the server job
//! made to run as the user (`QSYGETPH`/`QWTSETP`: the session's
//! principal changes, the job keeps its name and user), profile tokens,
//! and the change password flow, whose new password comes protected
//! with the old one's token.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use hostsvr::common::rc;
use hostsvr::crypt::{Level, Seeds};
use sec::audit::{Event, Kind};
use sec::profile::PwdForm;
use sec::signon::Substitute;
use stg::{HostRefused, HostSignedOn};

use super::security::{principal, sign_on, sysval_text};
use super::served::JobCtx;
use crate::ipl::system::System;

/// A profile token: whose, until when, of what kind.
#[derive(Clone, Debug)]
struct Token {
    user: String,
    until: u64,
    kind: u8,
}

fn tokens() -> &'static Mutex<HashMap<Vec<u8>, Token>> {
    static T: OnceLock<Mutex<HashMap<Vec<u8>, Token>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now() -> u64 {
    crate::ipl::system::now_unix()
}

fn refused(rc: u32, msgid: &str, text: &str) -> HostRefused {
    HostRefused {
        rc,
        msgid: msgid.into(),
        text: text.into(),
    }
}

/// Whether `user` has a profile.
fn exists(system: &mut System, user: &str) -> bool {
    read(system, user).is_some()
}

fn read(system: &mut System, user: &str) -> Option<sec::profile::UsrPrf> {
    sec::guard::as_system(|| {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        sec::profile::objects::read(&mut txn, qsys, user).ok().map(|(_, p)| p)
    })
}

/// The user a token stands for, used up when it is single use.
fn redeem(token: &[u8]) -> Result<String, HostRefused> {
    let mut t = tokens().lock().unwrap_or_else(PoisonError::into_inner);
    let Some(k) = t.get(token).cloned() else {
        return Err(refused(rc::TOKEN_REQUEST, "CPF4AB8", "The profile token is not valid."));
    };
    if k.until < now() {
        t.remove(token);
        return Err(refused(rc::TOKEN_REQUEST, "CPF4AB8", "The profile token has expired."));
    }
    if k.kind == hostsvr::signon::token_type::SINGLE_USE {
        t.remove(token);
    }
    Ok(k.user)
}

/// Check a sign-on; the profile it proved.
fn check(system: &mut System, user: &str, proof: &Substitute, token: &[u8]) -> Result<(String, sec::signon::SignedOn), HostRefused> {
    if !token.is_empty() {
        let who = redeem(token)?;
        return Ok((who.clone(), sec::signon::SignedOn { user: who, ..Default::default() }));
    }
    let upper = user.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return Err(refused(rc::USER_ID_LENGTH, "CPF1107", "No user was given."));
    }
    match sign_on(system, &upper, "", "", false, Some(proof.clone())) {
        Ok(s) if s.next == sec::signon::Next::ChangePassword => {
            Err(refused(rc::PASSWORD_EXPIRED, "CPF22E4", "The password has expired."))
        }
        Ok(s) => Ok((s.user.clone(), s)),
        Err(e) => {
            let code = match e.msgid.as_str() {
                "CPF1107" if !exists(system, &upper) => rc::USER_UNKNOWN,
                "CPF1394" | "CPF1110" | "CPF1220" => rc::USER_DISABLED,
                "CPF1118" => rc::PASSWORD_NONE,
                "CPF1393" => rc::PASSWORD_INCORRECT_DISABLED,
                _ => rc::PASSWORD_INCORRECT,
            };
            Err(HostRefused {
                rc: code,
                msgid: e.msgid,
                text: e.text,
            })
        }
    }
}

/// `host_signon`.
pub fn signon(system: &mut System, ctx: &mut JobCtx, user: &str, proof: &Substitute, token: &[u8], swap: bool) -> Result<HostSignedOn, HostRefused> {
    let (who, s) = check(system, user, proof, token)?;
    if swap {
        become_user(system, ctx, &who);
    }
    let warn = sysval_text(system, "QPWDEXPWRN").trim().parse().unwrap_or(7);
    let t = now();
    Ok(HostSignedOn {
        user: who,
        now: t,
        previous: s.previous,
        expires: s.expires_in.map_or(0, |d| t.saturating_add(u64::try_from(d.max(0)).unwrap_or(0) * 86_400)),
        warning_days: warn,
    })
}

/// The session runs as `user`, keeping what the job adopts (`QWTSETP`).
fn become_user(system: &mut System, ctx: &mut JobCtx, user: &str) {
    let from = ctx.session.borrow().principal.user.clone();
    let mut p = principal(system, user);
    p.adopted = std::mem::take(&mut ctx.session.borrow_mut().principal.adopted);
    *ctx.session.borrow_mut() = sec::auth::cache::Session::new(p);
    sec::audit::record(
        Event::new(Kind::Ps)
            .set("ETYP", "S")
            .set("USRP", from.as_str())
            .set("NUSR", user),
    );
}

/// `host_reset`: the job's own user again.
pub fn reset(system: &mut System, ctx: &mut JobCtx) {
    let own = ctx.job.user.clone();
    if ctx.session.borrow().principal.user != own {
        become_user(system, ctx, &own);
    }
}

/// `host_token`.
pub fn token(system: &mut System, user: &str, proof: &Substitute, token: &[u8], kind: u8, timeout: u32) -> Result<Vec<u8>, HostRefused> {
    if !matches!(kind, 0xF1..=0xF3) {
        return Err(refused(rc::TOKEN_TYPE, "CPF4AB9", "The token type is not valid."));
    }
    let (who, _) = check(system, user, proof, token)?;
    let mut t = sec::password::fresh_salt().to_vec();
    t.extend(sec::password::fresh_salt());
    let timeout = if timeout == 0 { 3600 } else { timeout.min(3600) };
    tokens().lock().unwrap_or_else(PoisonError::into_inner).insert(
        t.clone(),
        Token {
            user: who.clone(),
            until: now() + u64::from(timeout),
            kind,
        },
    );
    sec::audit::record(Event::new(Kind::Ps).set("ETYP", "T").set("USRP", who.as_str()).set("NUSR", who.as_str()));
    Ok(t)
}

/// The password a profile's tokens keep, for the level its substitute
/// came at.
fn stored_password(p: &sec::profile::UsrPrf, level: Level) -> Option<String> {
    let unhex = |s: &str| -> Vec<u8> { (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect() };
    match level {
        Level::Des => p.hash(PwdForm::DesToken).map(|h| ccsid::from_ebcdic(&unhex(&h.phc)).trim_end().to_string()),
        _ => p.hash(PwdForm::ShaToken).and_then(|h| {
            let b = unhex(&h.phc);
            let units: Vec<u16> = b.chunks(2).map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)])).collect();
            String::from_utf16(&units).ok()
        }),
    }
}

/// `host_chgpwd`.
pub fn chgpwd(system: &mut System, user: &str, proof: &Substitute, new_protected: &[u8], new_len: u32) -> Result<(), HostRefused> {
    let (who, _) = check(system, user, proof, &[])?;
    let level = Level::from_qpwdlvl(sysval_text(system, "QPWDLVL").trim().parse().unwrap_or(0));
    let p = read(system, &who).ok_or_else(|| refused(rc::USER_UNKNOWN, "CPF2204", "The user profile was not found."))?;
    let old = stored_password(&p, level).ok_or_else(|| refused(rc::GENERAL, "CPF22E5", "The password cannot be changed here."))?;
    let seeds = Seeds {
        client: proof.client_seed,
        server: proof.server_seed,
    };
    let new = hostsvr::crypt::recover_new_password(level, &who, &old, &seeds, new_protected, new_len)
        .map_err(|_| refused(rc::NEW_PASSWORD_NOT_VALID, "CPF22C2", "The new password could not be read."))?;
    let me = principal(system, &who);
    super::security::chg_pwd(system, &me, &old, &new, true).map_err(|e| {
        let id = e.split_whitespace().next().unwrap_or("CPF22C2").to_string();
        refused(rc::NEW_PASSWORD_NOT_VALID, &id, &e)
    })
}
