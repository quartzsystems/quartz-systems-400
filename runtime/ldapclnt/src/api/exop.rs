// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Extended operations: `ldap_extended_operation` (message ID) and
//! `ldap_extended_operation_s` (the response's name and value).

use ldap::msg::ProtocolOp;
use mi::Job;

use super::result::{record, wait_all};
use super::{give_berval, give_str, h, on, ret, text};
use crate::consts::*;
use crate::mem;

/// `int ldap_extended_operation(LDAP *ld, const char *reqoid, const
/// struct berval *reqdata, LDAPControl **serverctrls, LDAPControl
/// **clientctrls, int *msgidp)`.
#[allow(clippy::too_many_arguments)]
pub fn extended_operation(
    j: &mut Job,
    ld: u128,
    oid: u128,
    data: u128,
    sc: u128,
    cc: u128,
    idp: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sc, cc) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let Some(name) = text(s, j, Some(k), oid) else {
            return ret(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
        };
        let op = ProtocolOp::ExtendedRequest {
            name,
            value: mem::berval(j, data),
        };
        match h(s, k).send(op, ctrls) {
            Ok(id) => {
                mem::put_i32(j, idp, id);
                ret(LDAP_SUCCESS)
            }
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_extended_operation_s(LDAP *ld, const char *reqoid, const
/// struct berval *reqdata, LDAPControl **serverctrls, LDAPControl
/// **clientctrls, char **retoidp, struct berval **retdatap)`.
#[allow(clippy::too_many_arguments)]
pub fn extended_operation_s(
    j: &mut Job,
    ld: u128,
    oid: u128,
    data: u128,
    sc: u128,
    cc: u128,
    retoid: u128,
    retdata: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sc, cc) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let Some(name) = text(s, j, Some(k), oid) else {
            return ret(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
        };
        let op = ProtocolOp::ExtendedRequest {
            name,
            value: mem::berval(j, data),
        };
        let hd = h(s, k);
        let id = match hd.send(op, ctrls) {
            Ok(id) => id,
            Err(e) => return ret(e),
        };
        let m = match wait_all(hd, id, None) {
            Ok(m) => m,
            Err(e) => return ret(e),
        };
        let code = record(hd, &m);
        let (rname, rvalue) = match &m.last().expect("one").op {
            ProtocolOp::ExtendedResponse { name, value, .. } => (name.clone(), value.clone()),
            _ => (None, None),
        };
        let c = s.codec_of(Some(k));
        if !mem::is_null(retoid) {
            let p = rname.map_or(mi::FatPtr::null(), |n| give_str(s, j, c, &n));
            mem::put_ptr(j, retoid, p);
        }
        if !mem::is_null(retdata) {
            let p = rvalue.map_or(mi::FatPtr::null(), |v| give_berval(s, j, &v));
            mem::put_ptr(j, retdata, p);
        }
        ret(code)
    })
}
