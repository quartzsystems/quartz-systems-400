// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Add, modify, delete, rename (`ldap_modrdn`, `ldap_rename`), compare
//! and abandon, in their asynchronous (message ID), `_s`, `_ext` and
//! `_ext_s` forms. `LDAPMod` values are strings in the handle's codec
//! unless `LDAP_MOD_BVALUES` is set.

use ldap::control::Control;
use ldap::msg::{Attribute, ModOp, Modification, ProtocolOp};
use mi::Job;

use super::result::{record, wait_all};
use super::{h, on, ret, text};
use crate::consts::*;
use crate::mem::{self, Codec};
use crate::state::State;

/// One `LDAPMod`: the operation, the type, the values.
pub type Change = (i32, String, Vec<Vec<u8>>);

/// A program's `LDAPMod **`: each operation, type and values.
pub fn read_mods(j: &Job, codec: Codec, p: u128) -> Result<Vec<Change>, i32> {
    let mut out = Vec::new();
    for m in mem::ptr_array(j, p) {
        let b = mem::bytes(j, m, mem::MOD).ok_or(LDAP_PARAM_ERROR)?;
        let op = i32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        let ty = u128::from_le_bytes(b[16..32].try_into().expect("16"));
        let vals = u128::from_le_bytes(b[32..48].try_into().expect("16"));
        let ty = codec.arg(j, ty).ok_or(LDAP_PARAM_ERROR)?;
        let values: Vec<Vec<u8>> = if op & LDAP_MOD_BVALUES != 0 {
            mem::ptr_array(j, vals)
                .into_iter()
                .filter_map(|v| mem::berval(j, v))
                .collect()
        } else {
            codec
                .strings(j, vals)
                .into_iter()
                .map(String::into_bytes)
                .collect()
        };
        out.push((op & !LDAP_MOD_BVALUES, ty, values));
    }
    Ok(out)
}

fn mod_op(n: i32) -> Option<ModOp> {
    match n {
        LDAP_MOD_ADD => Some(ModOp::Add),
        LDAP_MOD_DELETE => Some(ModOp::Delete),
        LDAP_MOD_REPLACE => Some(ModOp::Replace),
        LDAP_MOD_INCREMENT => Some(ModOp::Increment),
        _ => None,
    }
}

/// Sends `op`; its message ID.
fn send(s: &mut State, k: u64, op: ProtocolOp, ctrls: Option<Vec<Control>>) -> Result<i32, i32> {
    h(s, k).send(op, ctrls)
}

/// Runs `op` to its result (following a referral when the handle
/// says); the result code.
fn run(s: &mut State, k: u64, op: ProtocolOp, ctrls: Option<Vec<Control>>) -> i32 {
    let hd = h(s, k);
    if hd.opts.referrals {
        let ctrls = ctrls.unwrap_or_else(|| hd.opts.sctrls.clone());
        let c = match hd.connect() {
            Ok(c) => c,
            Err(e) => return e,
        };
        return match c.request(&op, &ctrls) {
            Ok(r) => {
                let code = i32::try_from(r.result.code.number()).unwrap_or(LDAP_OTHER);
                hd.set_error(code, Some(r.result.matched_dn), Some(r.result.message))
            }
            Err(e) => hd.fail(&e),
        };
    }
    let id = match hd.send(op, ctrls) {
        Ok(id) => id,
        Err(e) => return e,
    };
    match wait_all(hd, id, None) {
        Ok(m) => record(hd, &m),
        Err(e) => e,
    }
}

/// How a call answers: a message ID, a code with `*msgidp`, or the
/// result code.
#[derive(Clone, Copy)]
enum Form {
    Id,
    Ext(u128),
    Sync,
}

fn go(
    j: &mut Job,
    s: &mut State,
    k: u64,
    op: ProtocolOp,
    ctrls: Option<Vec<Control>>,
    form: Form,
) -> i64 {
    match form {
        Form::Id => send(s, k, op, ctrls).map_or(-1, i64::from),
        Form::Ext(p) => match send(s, k, op, ctrls) {
            Ok(id) => {
                mem::put_i32(j, p, id);
                ret(LDAP_SUCCESS)
            }
            Err(e) => ret(e),
        },
        Form::Sync => ret(run(s, k, op, ctrls)),
    }
}

fn bad(form: Form) -> i64 {
    match form {
        Form::Id => -1,
        _ => ret(LDAP_PARAM_ERROR),
    }
}

/// Runs one operation of a form: `build` makes the request.
fn call(
    j: &mut Job,
    ld: u128,
    sctrls: u128,
    cctrls: u128,
    form: Form,
    build: impl FnOnce(&mut State, &Job, u64) -> Result<ProtocolOp, i32>,
) -> i64 {
    on(j, ld, bad(form), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sctrls, cctrls) {
            Ok(c) => c,
            Err(e) => return if matches!(form, Form::Id) { -1 } else { ret(e) },
        };
        match build(s, j, k) {
            Ok(op) => go(j, s, k, op, ctrls, form),
            Err(e) => {
                h(s, k).set_error(e, None, None);
                if matches!(form, Form::Id) { -1 } else { ret(e) }
            }
        }
    })
}

fn dn_of(s: &State, j: &Job, k: u64, p: u128) -> Result<String, i32> {
    text(s, j, Some(k), p).ok_or(LDAP_PARAM_ERROR)
}

fn add_op(s: &mut State, j: &Job, k: u64, dn: u128, mods: u128) -> Result<ProtocolOp, i32> {
    let codec = s.codec_of(Some(k));
    let dn = dn_of(s, j, k, dn)?;
    let mut attrs: Vec<Attribute> = Vec::new();
    for (_, ty, vals) in read_mods(j, codec, mods)? {
        match attrs.iter_mut().find(|a| a.ty.eq_ignore_ascii_case(&ty)) {
            Some(a) => a.vals.extend(vals),
            None => attrs.push(Attribute { ty, vals }),
        }
    }
    Ok(ProtocolOp::AddRequest { dn, attrs })
}

fn modify_op(s: &mut State, j: &Job, k: u64, dn: u128, mods: u128) -> Result<ProtocolOp, i32> {
    let codec = s.codec_of(Some(k));
    let dn = dn_of(s, j, k, dn)?;
    let mut changes = Vec::new();
    for (op, ty, vals) in read_mods(j, codec, mods)? {
        let op = mod_op(op).ok_or(LDAP_PARAM_ERROR)?;
        changes.push(Modification {
            op,
            attr: Attribute { ty, vals },
        });
    }
    Ok(ProtocolOp::ModifyRequest { dn, changes })
}

/// `int ldap_add(LDAP *ld, const char *dn, LDAPMod **attrs)`.
pub fn add(j: &mut Job, ld: u128, dn: u128, mods: u128) -> i64 {
    call(j, ld, 0, 0, Form::Id, |s, j, k| add_op(s, j, k, dn, mods))
}

/// `int ldap_add_s(LDAP *ld, const char *dn, LDAPMod **attrs)`.
pub fn add_s(j: &mut Job, ld: u128, dn: u128, mods: u128) -> i64 {
    call(j, ld, 0, 0, Form::Sync, |s, j, k| add_op(s, j, k, dn, mods))
}

/// `int ldap_add_ext(LDAP *ld, const char *dn, LDAPMod **attrs,
/// LDAPControl **serverctrls, LDAPControl **clientctrls, int *msgidp)`.
pub fn add_ext(j: &mut Job, ld: u128, dn: u128, mods: u128, sc: u128, cc: u128, idp: u128) -> i64 {
    call(j, ld, sc, cc, Form::Ext(idp), |s, j, k| {
        add_op(s, j, k, dn, mods)
    })
}

/// `int ldap_add_ext_s(LDAP *ld, const char *dn, LDAPMod **attrs,
/// LDAPControl **serverctrls, LDAPControl **clientctrls)`.
pub fn add_ext_s(j: &mut Job, ld: u128, dn: u128, mods: u128, sc: u128, cc: u128) -> i64 {
    call(j, ld, sc, cc, Form::Sync, |s, j, k| {
        add_op(s, j, k, dn, mods)
    })
}

/// `int ldap_modify(LDAP *ld, const char *dn, LDAPMod **mods)`.
pub fn modify(j: &mut Job, ld: u128, dn: u128, mods: u128) -> i64 {
    call(j, ld, 0, 0, Form::Id, |s, j, k| {
        modify_op(s, j, k, dn, mods)
    })
}

/// `int ldap_modify_s(LDAP *ld, const char *dn, LDAPMod **mods)`.
pub fn modify_s(j: &mut Job, ld: u128, dn: u128, mods: u128) -> i64 {
    call(j, ld, 0, 0, Form::Sync, |s, j, k| {
        modify_op(s, j, k, dn, mods)
    })
}

/// `int ldap_modify_ext(LDAP *ld, const char *dn, LDAPMod **mods,
/// LDAPControl **serverctrls, LDAPControl **clientctrls, int *msgidp)`.
pub fn modify_ext(
    j: &mut Job,
    ld: u128,
    dn: u128,
    mods: u128,
    sc: u128,
    cc: u128,
    idp: u128,
) -> i64 {
    call(j, ld, sc, cc, Form::Ext(idp), |s, j, k| {
        modify_op(s, j, k, dn, mods)
    })
}

/// `int ldap_modify_ext_s(LDAP *ld, const char *dn, LDAPMod **mods,
/// LDAPControl **serverctrls, LDAPControl **clientctrls)`.
pub fn modify_ext_s(j: &mut Job, ld: u128, dn: u128, mods: u128, sc: u128, cc: u128) -> i64 {
    call(j, ld, sc, cc, Form::Sync, |s, j, k| {
        modify_op(s, j, k, dn, mods)
    })
}

fn delete_op(s: &mut State, j: &Job, k: u64, dn: u128) -> Result<ProtocolOp, i32> {
    Ok(ProtocolOp::DelRequest(dn_of(s, j, k, dn)?))
}

/// `int ldap_delete(LDAP *ld, const char *dn)`.
pub fn delete(j: &mut Job, ld: u128, dn: u128) -> i64 {
    call(j, ld, 0, 0, Form::Id, |s, j, k| delete_op(s, j, k, dn))
}

/// `int ldap_delete_s(LDAP *ld, const char *dn)`.
pub fn delete_s(j: &mut Job, ld: u128, dn: u128) -> i64 {
    call(j, ld, 0, 0, Form::Sync, |s, j, k| delete_op(s, j, k, dn))
}

/// `int ldap_delete_ext(LDAP *ld, const char *dn, LDAPControl
/// **serverctrls, LDAPControl **clientctrls, int *msgidp)`.
pub fn delete_ext(j: &mut Job, ld: u128, dn: u128, sc: u128, cc: u128, idp: u128) -> i64 {
    call(j, ld, sc, cc, Form::Ext(idp), |s, j, k| {
        delete_op(s, j, k, dn)
    })
}

/// `int ldap_delete_ext_s(LDAP *ld, const char *dn, LDAPControl
/// **serverctrls, LDAPControl **clientctrls)`.
pub fn delete_ext_s(j: &mut Job, ld: u128, dn: u128, sc: u128, cc: u128) -> i64 {
    call(j, ld, sc, cc, Form::Sync, |s, j, k| delete_op(s, j, k, dn))
}

fn rename_op(
    s: &mut State,
    j: &Job,
    k: u64,
    dn: u128,
    rdn: u128,
    parent: u128,
    del: i64,
) -> Result<ProtocolOp, i32> {
    Ok(ProtocolOp::ModDnRequest {
        dn: dn_of(s, j, k, dn)?,
        new_rdn: dn_of(s, j, k, rdn)?,
        delete_old_rdn: mem::int(del) != 0,
        new_superior: text(s, j, Some(k), parent),
    })
}

/// `int ldap_modrdn(LDAP *ld, const char *dn, const char *newrdn, int
/// deleteoldrdn)`.
pub fn modrdn(j: &mut Job, ld: u128, dn: u128, rdn: u128, del: i64) -> i64 {
    call(j, ld, 0, 0, Form::Id, |s, j, k| {
        rename_op(s, j, k, dn, rdn, 0, del)
    })
}

/// `int ldap_modrdn_s(LDAP *ld, const char *dn, const char *newrdn, int
/// deleteoldrdn)`.
pub fn modrdn_s(j: &mut Job, ld: u128, dn: u128, rdn: u128, del: i64) -> i64 {
    call(j, ld, 0, 0, Form::Sync, |s, j, k| {
        rename_op(s, j, k, dn, rdn, 0, del)
    })
}

/// `int ldap_rename(LDAP *ld, const char *dn, const char *newrdn, const
/// char *newparent, int deleteoldrdn, LDAPControl **serverctrls,
/// LDAPControl **clientctrls, int *msgidp)`.
#[allow(clippy::too_many_arguments)]
pub fn rename(
    j: &mut Job,
    ld: u128,
    dn: u128,
    rdn: u128,
    parent: u128,
    del: i64,
    sc: u128,
    cc: u128,
    idp: u128,
) -> i64 {
    call(j, ld, sc, cc, Form::Ext(idp), |s, j, k| {
        rename_op(s, j, k, dn, rdn, parent, del)
    })
}

/// `int ldap_rename_s(LDAP *ld, const char *dn, const char *newrdn,
/// const char *newparent, int deleteoldrdn, LDAPControl **serverctrls,
/// LDAPControl **clientctrls)`.
#[allow(clippy::too_many_arguments)]
pub fn rename_s(
    j: &mut Job,
    ld: u128,
    dn: u128,
    rdn: u128,
    parent: u128,
    del: i64,
    sc: u128,
    cc: u128,
) -> i64 {
    call(j, ld, sc, cc, Form::Sync, |s, j, k| {
        rename_op(s, j, k, dn, rdn, parent, del)
    })
}

fn compare_op(
    s: &mut State,
    j: &Job,
    k: u64,
    dn: u128,
    attr: u128,
    value: Vec<u8>,
) -> Result<ProtocolOp, i32> {
    Ok(ProtocolOp::CompareRequest {
        dn: dn_of(s, j, k, dn)?,
        attr: dn_of(s, j, k, attr)?,
        value,
    })
}

fn str_value(s: &State, j: &Job, k: u64, v: u128) -> Result<Vec<u8>, i32> {
    text(s, j, Some(k), v)
        .map(String::into_bytes)
        .ok_or(LDAP_PARAM_ERROR)
}

/// `int ldap_compare(LDAP *ld, const char *dn, const char *attr, const
/// char *value)`.
pub fn compare(j: &mut Job, ld: u128, dn: u128, attr: u128, v: u128) -> i64 {
    call(j, ld, 0, 0, Form::Id, |s, j, k| {
        let v = str_value(s, j, k, v)?;
        compare_op(s, j, k, dn, attr, v)
    })
}

/// `int ldap_compare_s(LDAP *ld, const char *dn, const char *attr,
/// const char *value)`: `LDAP_COMPARE_TRUE` or `LDAP_COMPARE_FALSE`.
pub fn compare_s(j: &mut Job, ld: u128, dn: u128, attr: u128, v: u128) -> i64 {
    call(j, ld, 0, 0, Form::Sync, |s, j, k| {
        let v = str_value(s, j, k, v)?;
        compare_op(s, j, k, dn, attr, v)
    })
}

/// `int ldap_compare_ext(LDAP *ld, const char *dn, const char *attr,
/// const struct berval *bvalue, LDAPControl **serverctrls, LDAPControl
/// **clientctrls, int *msgidp)`.
#[allow(clippy::too_many_arguments)]
pub fn compare_ext(
    j: &mut Job,
    ld: u128,
    dn: u128,
    attr: u128,
    bv: u128,
    sc: u128,
    cc: u128,
    idp: u128,
) -> i64 {
    call(j, ld, sc, cc, Form::Ext(idp), |s, j, k| {
        let v = mem::berval(j, bv).ok_or(LDAP_PARAM_ERROR)?;
        compare_op(s, j, k, dn, attr, v)
    })
}

/// `int ldap_compare_ext_s(LDAP *ld, const char *dn, const char *attr,
/// const struct berval *bvalue, LDAPControl **serverctrls, LDAPControl
/// **clientctrls)`.
#[allow(clippy::too_many_arguments)]
pub fn compare_ext_s(
    j: &mut Job,
    ld: u128,
    dn: u128,
    attr: u128,
    bv: u128,
    sc: u128,
    cc: u128,
) -> i64 {
    call(j, ld, sc, cc, Form::Sync, |s, j, k| {
        let v = mem::berval(j, bv).ok_or(LDAP_PARAM_ERROR)?;
        compare_op(s, j, k, dn, attr, v)
    })
}

fn do_abandon(s: &mut State, k: u64, id: i32) -> i64 {
    let hd = h(s, k);
    hd.outstanding.retain(|o| *o != id);
    hd.pending.remove(&id);
    let Some(c) = hd.client.as_mut() else {
        return ret(hd.set_error(LDAP_SERVER_DOWN, None, None));
    };
    match c.abandon(id) {
        Ok(()) => ret(hd.set_error(LDAP_SUCCESS, None, None)),
        Err(e) => ret(hd.fail(&e)),
    }
}

/// `int ldap_abandon(LDAP *ld, int msgid)`: 0, or -1 on an error.
pub fn abandon(j: &mut Job, ld: u128, id: i64) -> i64 {
    on(j, ld, -1, |s, _, k| {
        if do_abandon(s, k, mem::int(id)) == 0 {
            0
        } else {
            -1
        }
    })
}

/// `int ldap_abandon_ext(LDAP *ld, int msgid, LDAPControl
/// **serverctrls, LDAPControl **clientctrls)`: an abandon carrying
/// server controls is `LDAP_NOT_SUPPORTED`.
pub fn abandon_ext(j: &mut Job, ld: u128, id: i64, sc: u128, cc: u128) -> i64 {
    on(
        j,
        ld,
        ret(LDAP_PARAM_ERROR),
        |s, j, k| match super::control::op_controls(s, j, k, sc, cc) {
            Err(e) => ret(e),
            Ok(Some(c)) if !c.is_empty() => ret(h(s, k).set_error(
                LDAP_NOT_SUPPORTED,
                None,
                Some("an abandon request carries no controls here".into()),
            )),
            Ok(_) => do_abandon(s, k, mem::int(id)),
        },
    )
}
