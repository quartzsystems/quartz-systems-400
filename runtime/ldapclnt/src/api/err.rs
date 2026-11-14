// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Errors: `ldap_err2string` (Quartz's texts), `ldap_perror` (to the
//! job's output), the handle's error number, matched DN and message.

use mi::{FatPtr, Job};

use super::{give_str, h, key, on, ptr, ret, with};
use crate::consts::*;
use crate::mem;
use crate::state::State;

fn kept(s: &mut State, j: &mut Job, table: u8, code: i32, t: &str) -> FatPtr {
    if let Some(p) = s.texts.get(&(table, code)) {
        return *p;
    }
    let c = s.codec();
    let p = give_str(s, j, c, t);
    s.texts.insert((table, code), p);
    p
}

/// `char *ldap_err2string(int err)`: a text kept for good (not to be
/// freed).
pub fn err2string(j: &mut Job, code: i64) -> u128 {
    let code = mem::int(code);
    with(j, |s, j| ptr(kept(s, j, 0, code, err_text(code))))
}

/// The text of a password policy error number (`LDAP_PP_*`).
#[must_use]
pub fn pwdpolicy_text(code: i32) -> &'static str {
    match code {
        LDAP_PP_NO_ERROR => "No password policy error",
        LDAP_PP_PASSWORD_EXPIRED => "The password has expired",
        LDAP_PP_ACCOUNT_LOCKED => "The account is locked",
        LDAP_PP_CHANGE_AFTER_RESET => "The password must be changed after a reset",
        LDAP_PP_MOD_NOT_ALLOWED => "The password may not be changed",
        LDAP_PP_MUST_SUPPLY_OLD => "The old password must be given",
        LDAP_PP_INSUFFICIENT_QUALITY => "The password is not of the quality required",
        LDAP_PP_TOO_SHORT => "The password is too short",
        LDAP_PP_TOO_YOUNG => "The password was changed too recently",
        LDAP_PP_IN_HISTORY => "The password was used before",
        _ => "Unknown password policy error",
    }
}

/// `char *ldap_pwdpolicy_err2string(int error)`.
pub fn pwdpolicy_err2string(j: &mut Job, code: i64) -> u128 {
    let code = mem::int(code);
    with(j, |s, j| ptr(kept(s, j, 1, code, pwdpolicy_text(code))))
}

/// `void ldap_perror(LDAP *ld, const char *s)`: `s: text` and the
/// server's message, as job output.
pub fn perror(j: &mut Job, ld: u128, sp: u128) -> i64 {
    let line = with(j, |s, j| {
        let k = key(s, j, ld);
        let pre = s.codec_of(k).arg(j, sp).filter(|p| !p.is_empty());
        let (code, msg) = match k {
            Some(k) => {
                let hd = h(s, k);
                (hd.errno, hd.errmsg.clone())
            }
            None => (s.errno, None),
        };
        let mut line = match pre {
            Some(p) => format!("{p}: {}", err_text(code)),
            None => err_text(code).to_string(),
        };
        if let Some(m) = msg {
            line.push_str(" (");
            line.push_str(&m);
            line.push(')');
        }
        line
    });
    j.console.push(line);
    0
}

fn hold(s: &mut State, j: &mut Job, k: u64, slot: u8, t: Option<String>, out: u128) {
    if mem::is_null(out) {
        return;
    }
    if let Some(old) = s.held.remove(&(k, slot)) {
        s.heap.free(j, old);
    }
    let p = match t {
        Some(t) => {
            let c = s.codec_of(Some(k));
            let p = give_str(s, j, c, &t);
            if let Some(n) = mem::native(j, mem::raw(p)) {
                s.held.insert((k, slot), n);
            }
            p
        }
        None => FatPtr::null(),
    };
    mem::put_ptr(j, out, p);
}

/// `int ldap_get_lderrno(LDAP *ld, char **m, char **s)`: the error
/// number; `*m` the matched DN and `*s` the message, owned by the
/// handle until the next call.
pub fn get_lderrno(j: &mut Job, ld: u128, m: u128, sp: u128) -> i64 {
    with(j, |s, j| match key(s, j, ld) {
        Some(k) => {
            let (code, md, msg) = {
                let hd = h(s, k);
                (hd.errno, hd.matched.clone(), hd.errmsg.clone())
            };
            hold(s, j, k, 0, md, m);
            hold(s, j, k, 1, msg, sp);
            ret(code)
        }
        None => ret(s.errno),
    })
}

/// `int ldap_set_lderrno(LDAP *ld, int e, char *m, char *s)`.
pub fn set_lderrno(j: &mut Job, ld: u128, e: i64, m: u128, sp: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let c = s.codec_of(Some(k));
        let md = c.arg(j, m);
        let msg = c.arg(j, sp);
        let hd = h(s, k);
        hd.errno = mem::int(e);
        hd.matched = md;
        hd.errmsg = msg;
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_get_errno(LDAP *ld)`.
pub fn get_errno(j: &mut Job, ld: u128) -> i64 {
    with(j, |s, j| match key(s, j, ld) {
        Some(k) => ret(h(s, k).errno),
        None => ret(s.errno),
    })
}

/// `int ldap_get_exterror(LDAP *ld)`: a secure connection's reason
/// code (0 for none).
pub fn get_exterror(j: &mut Job, ld: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, _, k| {
        ret(h(s, k).ext_error)
    })
}
