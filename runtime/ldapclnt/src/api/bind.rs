// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Binds: simple, and SASL `EXTERNAL` (the TLS client certificate). A
//! null DN or password takes `ldap_default_dn_set`'s and
//! `ldap_password_set`'s. Other SASL mechanisms (`DIGEST-MD5`,
//! `CRAM-MD5`, `GSSAPI`) are `LDAP_NOT_SUPPORTED` (docs/DIRSRV.md §14).

use ldap::msg::{BindAuth, ProtocolOp};
use mi::Job;

use super::result::wait_all;
use super::{give_berval, give_str, h, on, ret, text};
use crate::consts::*;
use crate::mem;
use crate::state::State;

/// The SASL mechanisms the client binds with.
pub const MECHANISMS: &[&str] = &["EXTERNAL"];

fn request(
    s: &mut State,
    j: &Job,
    k: u64,
    dn: u128,
    auth: Option<BindAuth>,
    pw: u128,
) -> ProtocolOp {
    let d = text(s, j, Some(k), dn)
        .or_else(|| h(s, k).default_dn.clone())
        .unwrap_or_default();
    let auth = auth.unwrap_or_else(|| {
        let p = text(s, j, Some(k), pw)
            .or_else(|| h(s, k).default_pw.clone())
            .unwrap_or_default();
        BindAuth::Simple(p.into_bytes())
    });
    ProtocolOp::BindRequest {
        version: h(s, k).opts.version,
        name: d,
        auth,
    }
}

fn send(s: &mut State, k: u64, op: ProtocolOp, ctrls: Option<Vec<ldap::control::Control>>) -> i64 {
    match h(s, k).send(op, ctrls) {
        Ok(id) => i64::from(id),
        Err(_) => -1,
    }
}

fn finish(s: &mut State, k: u64, id: i32) -> Result<(i32, Option<Vec<u8>>), i32> {
    let hd = h(s, k);
    let m = wait_all(hd, id, None)?;
    let code = super::result::record(hd, &m);
    let last = m.last().expect("one");
    hd.bind_controls.clone_from(&last.controls);
    let creds = match &last.op {
        ProtocolOp::BindResponse {
            server_sasl_creds, ..
        } => server_sasl_creds.clone(),
        _ => None,
    };
    Ok((code, creds))
}

/// `int ldap_simple_bind(LDAP *ld, const char *dn, const char *passwd)`:
/// the message ID, -1 on an error.
pub fn simple_bind(j: &mut Job, ld: u128, dn: u128, pw: u128) -> i64 {
    on(j, ld, -1, |s, j, k| {
        let op = request(s, j, k, dn, None, pw);
        send(s, k, op, None)
    })
}

/// `int ldap_simple_bind_s(LDAP *ld, const char *dn, const char *passwd)`.
pub fn simple_bind_s(j: &mut Job, ld: u128, dn: u128, pw: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let op = request(s, j, k, dn, None, pw);
        let id = match h(s, k).send(op, None) {
            Ok(id) => id,
            Err(code) => return ret(code),
        };
        ret(finish(s, k, id).map_or_else(|c| c, |(c, _)| c))
    })
}

/// `int ldap_bind(LDAP *ld, const char *dn, const char *cred, int
/// method)`: `LDAP_AUTH_SIMPLE` only; SASL goes through
/// `ldap_sasl_bind`.
pub fn bind(j: &mut Job, ld: u128, dn: u128, cred: u128, method: i64) -> i64 {
    if mem::int(method) != LDAP_AUTH_SIMPLE {
        return on(j, ld, -1, |s, _, k| {
            h(s, k).set_error(LDAP_AUTH_UNKNOWN, None, None);
            -1
        });
    }
    simple_bind(j, ld, dn, cred)
}

/// `int ldap_bind_s(LDAP *ld, const char *dn, const char *cred, int
/// method)`.
pub fn bind_s(j: &mut Job, ld: u128, dn: u128, cred: u128, method: i64) -> i64 {
    if mem::int(method) != LDAP_AUTH_SIMPLE {
        return on(j, ld, ret(LDAP_PARAM_ERROR), |s, _, k| {
            ret(h(s, k).set_error(LDAP_AUTH_UNKNOWN, None, None))
        });
    }
    simple_bind_s(j, ld, dn, cred)
}

/// The SASL request of a mechanism and credentials; a null mechanism
/// is a simple bind with the credentials as the password.
fn sasl_op(
    s: &mut State,
    j: &Job,
    k: u64,
    dn: u128,
    mech: u128,
    cred: u128,
) -> Result<ProtocolOp, i32> {
    let Some(m) = text(s, j, Some(k), mech) else {
        let pw = mem::berval(j, cred).unwrap_or_default();
        return Ok(request(s, j, k, dn, Some(BindAuth::Simple(pw)), 0));
    };
    let m = m.trim().to_ascii_uppercase();
    if !MECHANISMS.contains(&m.as_str()) {
        return Err(h(s, k).set_error(
            LDAP_NOT_SUPPORTED,
            None,
            Some(format!("SASL mechanism {m} is not supported; EXTERNAL is")),
        ));
    }
    let auth = BindAuth::Sasl {
        mechanism: m,
        credentials: mem::berval(j, cred),
    };
    Ok(request(s, j, k, dn, Some(auth), 0))
}

/// `int ldap_sasl_bind(LDAP *ld, const char *dn, const char
/// *mechanism, const struct berval *cred, LDAPControl **serverctrls,
/// LDAPControl **clientctrls, int *msgidp)`.
#[allow(clippy::too_many_arguments)]
pub fn sasl_bind(
    j: &mut Job,
    ld: u128,
    dn: u128,
    mech: u128,
    cred: u128,
    sctrls: u128,
    cctrls: u128,
    msgidp: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sctrls, cctrls) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let op = match sasl_op(s, j, k, dn, mech, cred) {
            Ok(op) => op,
            Err(e) => return ret(e),
        };
        match h(s, k).send(op, ctrls) {
            Ok(id) => {
                mem::put_i32(j, msgidp, id);
                ret(LDAP_SUCCESS)
            }
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_sasl_bind_s(LDAP *ld, const char *dn, const char
/// *mechanism, const struct berval *cred, LDAPControl **serverctrls,
/// LDAPControl **clientctrls, struct berval **servercredp)`.
#[allow(clippy::too_many_arguments)]
pub fn sasl_bind_s(
    j: &mut Job,
    ld: u128,
    dn: u128,
    mech: u128,
    cred: u128,
    sctrls: u128,
    cctrls: u128,
    credp: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sctrls, cctrls) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let op = match sasl_op(s, j, k, dn, mech, cred) {
            Ok(op) => op,
            Err(e) => return ret(e),
        };
        let id = match h(s, k).send(op, ctrls) {
            Ok(id) => id,
            Err(e) => return ret(e),
        };
        match finish(s, k, id) {
            Ok((code, creds)) => {
                if !mem::is_null(credp) {
                    let p = creds.map_or(mi::FatPtr::null(), |c| give_berval(s, j, &c));
                    mem::put_ptr(j, credp, p);
                }
                ret(code)
            }
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_default_dn_set(LDAP *ld, char *dn)`.
pub fn default_dn_set(j: &mut Job, ld: u128, dn: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let d = text(s, j, Some(k), dn);
        h(s, k).default_dn = d;
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_default_dn_get(LDAP *ld, char **dn)`: a copy, freed with
/// `ldap_memfree`; null when none is set.
pub fn default_dn_get(j: &mut Job, ld: u128, out: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let c = s.codec_of(Some(k));
        let p = match h(s, k).default_dn.clone() {
            Some(d) => give_str(s, j, c, &d),
            None => mi::FatPtr::null(),
        };
        if mem::put_ptr(j, out, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}

/// `int ldap_password_set(LDAP *ld, char *password)`.
pub fn password_set(j: &mut Job, ld: u128, pw: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let p = text(s, j, Some(k), pw);
        h(s, k).default_pw = p;
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_password_get(LDAP *ld, char **password)`.
pub fn password_get(j: &mut Job, ld: u128, out: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let c = s.codec_of(Some(k));
        let p = match h(s, k).default_pw.clone() {
            Some(d) => give_str(s, j, c, &d),
            None => mi::FatPtr::null(),
        };
        if mem::put_ptr(j, out, p) {
            ret(LDAP_SUCCESS)
        } else {
            ret(LDAP_PARAM_ERROR)
        }
    })
}
