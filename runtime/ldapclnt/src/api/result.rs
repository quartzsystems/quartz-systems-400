// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Results: `ldap_result` collects an operation's messages into a chain
//! the program walks and frees with `ldap_msgfree`; the `ldap_parse_*`
//! functions read a chain's result; `ldap_msgtype`, `ldap_msgid`,
//! `ldap_result2error`.

use std::time::{Duration, Instant};

use ldap::msg::{LdapMessage, LdapResult, ProtocolOp};
use mi::Job;

use super::{give_berval, give_controls, give_str, give_strings, h, key, on, ret, timeval, with};
use crate::consts::*;
use crate::mem;
use crate::state::{Handle, State};

/// Whether a message ends its operation.
#[must_use]
pub fn is_final(m: &LdapMessage) -> bool {
    !matches!(
        m.op,
        ProtocolOp::SearchResultEntry { .. }
            | ProtocolOp::SearchResultReference(_)
            | ProtocolOp::IntermediateResponse { .. }
    )
}

/// The `LDAP_RES_*` type of a message.
#[must_use]
pub fn msg_type(m: &LdapMessage) -> i32 {
    i32::from(m.op.tag())
}

/// Takes the messages `all` asks for from what has arrived.
fn take(hd: &mut Handle, id: Option<i32>, all: i32) -> Option<Vec<LdapMessage>> {
    let ids: Vec<i32> = match id {
        Some(i) => vec![i],
        None => hd.pending.keys().copied().collect(),
    };
    for i in ids {
        let Some(q) = hd.pending.get_mut(&i) else {
            continue;
        };
        if q.is_empty() {
            continue;
        }
        let out: Vec<LdapMessage> = match all {
            LDAP_MSG_ALL => match q.iter().position(is_final) {
                Some(p) => q.drain(..=p).collect(),
                None => continue,
            },
            LDAP_MSG_RECEIVED => std::mem::take(q),
            _ => vec![q.remove(0)],
        };
        if q.is_empty() {
            hd.pending.remove(&i);
        }
        if out.iter().any(is_final) {
            hd.outstanding.retain(|o| *o != i);
        }
        return Some(out);
    }
    None
}

/// The messages of `id` (any with none) as `all` asks, waiting at most
/// `timeout`; empty when nothing came in time.
pub fn collect(
    hd: &mut Handle,
    id: Option<i32>,
    all: i32,
    timeout: Option<Duration>,
) -> Result<Vec<LdapMessage>, i32> {
    let deadline = timeout.map(|t| Instant::now() + t);
    loop {
        if let Some(m) = take(hd, id, all) {
            return Ok(m);
        }
        if all == LDAP_MSG_RECEIVED && id.is_some_and(|i| !hd.outstanding.contains(&i)) {
            return Ok(Vec::new());
        }
        let left = deadline.map(|d| d.saturating_duration_since(Instant::now()));
        let Some(c) = hd.client.as_mut() else {
            return Err(hd.set_error(LDAP_SERVER_DOWN, None, None));
        };
        match c.poll(None, left) {
            Ok(Some(m)) => hd.pending.entry(m.id).or_default().push(m),
            Ok(None) => return Ok(Vec::new()),
            Err(e) => return Err(hd.fail(&e)),
        }
        if left.is_some_and(|l| l.is_zero()) && take_ready(hd, id, all).is_none() {
            return Ok(Vec::new());
        }
    }
}

/// Whether something is ready to take (for a zero timeout's one look).
fn take_ready(hd: &Handle, id: Option<i32>, all: i32) -> Option<()> {
    let ready = |q: &Vec<LdapMessage>| {
        if all == LDAP_MSG_ALL {
            q.iter().any(is_final)
        } else {
            !q.is_empty()
        }
    };
    let ok = match id {
        Some(i) => hd.pending.get(&i).is_some_and(ready),
        None => hd.pending.values().any(ready),
    };
    ok.then_some(())
}

/// Waits for the whole of `id` (a synchronous call); a timeout abandons
/// it and is `LDAP_TIMEOUT`.
pub fn wait_all(
    hd: &mut Handle,
    id: i32,
    timeout: Option<Duration>,
) -> Result<Vec<LdapMessage>, i32> {
    let m = collect(hd, Some(id), LDAP_MSG_ALL, timeout)?;
    if m.is_empty() {
        if let Some(c) = hd.client.as_mut() {
            let _ = c.abandon(id);
        }
        hd.outstanding.retain(|o| *o != id);
        hd.pending.remove(&id);
        return Err(hd.set_error(LDAP_TIMEOUT, None, None));
    }
    Ok(m)
}

/// The result of a chain: its final message's.
#[must_use]
pub fn result_of(msgs: &[LdapMessage]) -> Option<&LdapResult> {
    msgs.iter()
        .rev()
        .find(|m| is_final(m))
        .and_then(|m| m.op.result())
}

/// Records a chain's result in the handle; its code.
pub fn record(hd: &mut Handle, msgs: &[LdapMessage]) -> i32 {
    match result_of(msgs) {
        Some(r) => {
            let code = i32::try_from(r.code.number()).unwrap_or(LDAP_OTHER);
            hd.set_error(code, Some(r.matched_dn.clone()), Some(r.message.clone()))
        }
        None => hd.set_error(LDAP_SUCCESS, None, None),
    }
}

/// `int ldap_result(LDAP *ld, int msgid, int all, struct timeval
/// *timeout, LDAPMessage **result)`: the type of the last message
/// returned, 0 when the time ran out, -1 on an error.
pub fn result(j: &mut Job, ld: u128, msgid: i64, all: i64, tv: u128, res: u128) -> i64 {
    on(j, ld, -1, |s, j, k| {
        let id = match mem::int(msgid) {
            LDAP_RES_ANY => None,
            n => Some(n),
        };
        let t = timeval(j, tv);
        match collect(h(s, k), id, mem::int(all), t) {
            Ok(m) if m.is_empty() => {
                mem::put_ptr(j, res, mi::FatPtr::null());
                h(s, k).set_error(LDAP_TIMEOUT, None, None);
                0
            }
            Ok(m) => {
                let ty = msg_type(m.last().expect("one"));
                record(h(s, k), &m);
                let p = s.new_chain(j, k, m);
                mem::put_ptr(j, res, p);
                i64::from(ty)
            }
            Err(_) => {
                mem::put_ptr(j, res, mi::FatPtr::null());
                -1
            }
        }
    })
}

/// Hands a chain to the program through `res`; the chain's result code.
pub fn give_chain(s: &mut State, j: &mut Job, k: u64, msgs: Vec<LdapMessage>, res: u128) -> i32 {
    let code = record(h(s, k), &msgs);
    let p = s.new_chain(j, k, msgs);
    mem::put_ptr(j, res, p);
    code
}

/// `int ldap_msgfree(LDAPMessage *lm)`: the type of the chain's first
/// message, 0 for null.
pub fn msgfree(j: &mut Job, lm: u128) -> i64 {
    with(j, |s, j| {
        let Some(n) = mem::native(j, lm) else {
            return 0;
        };
        match s.chains.remove(&n) {
            Some(c) => {
                s.heap.free(j, n);
                c.msgs.first().map_or(0, |m| i64::from(msg_type(m)))
            }
            None => ret(LDAP_PARAM_ERROR),
        }
    })
}

/// The message a pointer reaches, cloned.
pub fn message(s: &State, j: &Job, p: u128) -> Option<(u64, usize, LdapMessage)> {
    let (base, i) = s.message(j, p)?;
    Some((base, i, s.chains[&base].msgs[i].clone()))
}

/// `int ldap_msgtype(LDAPMessage *lm)`; -1 for no message.
pub fn msgtype(j: &mut Job, lm: u128) -> i64 {
    with(j, |s, j| {
        message(s, j, lm).map_or(-1, |(_, _, m)| i64::from(msg_type(&m)))
    })
}

/// `int ldap_msgid(LDAPMessage *lm)`; -1 for no message.
pub fn msgid(j: &mut Job, lm: u128) -> i64 {
    with(j, |s, j| {
        message(s, j, lm).map_or(-1, |(_, _, m)| i64::from(m.id))
    })
}

/// The result message of the chain from `res` on.
fn result_msg(s: &State, j: &Job, res: u128) -> Option<(u64, LdapMessage)> {
    let (base, i) = s.message(j, res)?;
    s.chains[&base].msgs[i..]
        .iter()
        .find(|m| is_final(m))
        .map(|m| (base, m.clone()))
}

fn maybe_free(s: &mut State, j: &mut Job, res: u128, freeit: i64) {
    if freeit != 0
        && let Some(n) = mem::native(j, res)
        && s.chains.remove(&n).is_some()
    {
        s.heap.free(j, n);
    }
}

/// `int ldap_result2error(LDAP *ld, LDAPMessage *res, int freeit)`.
pub fn result2error(j: &mut Job, ld: u128, res: u128, freeit: i64) -> i64 {
    with(j, |s, j| {
        let Some((_, m)) = result_msg(s, j, res) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let code = if let Some(k) = key(s, j, ld) {
            record(h(s, k), std::slice::from_ref(&m))
        } else {
            m.op.result().map_or(LDAP_SUCCESS, |r| {
                i32::try_from(r.code.number()).unwrap_or(LDAP_OTHER)
            })
        };
        maybe_free(s, j, res, freeit);
        ret(code)
    })
}

/// `int ldap_parse_result(LDAP *ld, LDAPMessage *res, int *errcodep,
/// char **matcheddnp, char **errmsgp, char ***referralsp, LDAPControl
/// ***serverctrlsp, int freeit)`.
#[allow(clippy::too_many_arguments)]
pub fn parse_result(
    j: &mut Job,
    ld: u128,
    res: u128,
    code: u128,
    matched: u128,
    errmsg: u128,
    refs: u128,
    ctrls: u128,
    freeit: i64,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let Some((_, m)) = result_msg(s, j, res) else {
            return ret(h(s, k).set_error(LDAP_NO_RESULTS_RETURNED, None, None));
        };
        let codec = s.codec_of(Some(k));
        let r = m.op.result().cloned().unwrap_or_else(LdapResult::success);
        let n = i32::try_from(r.code.number()).unwrap_or(LDAP_OTHER);
        mem::put_i32(j, code, n);
        if !mem::is_null(matched) {
            let p = if r.matched_dn.is_empty() {
                mi::FatPtr::null()
            } else {
                give_str(s, j, codec, &r.matched_dn)
            };
            mem::put_ptr(j, matched, p);
        }
        if !mem::is_null(errmsg) {
            let p = if r.message.is_empty() {
                mi::FatPtr::null()
            } else {
                give_str(s, j, codec, &r.message)
            };
            mem::put_ptr(j, errmsg, p);
        }
        if !mem::is_null(refs) {
            let p = if r.referral.is_empty() {
                mi::FatPtr::null()
            } else {
                give_strings(s, j, codec, &r.referral)
            };
            mem::put_ptr(j, refs, p);
        }
        if !mem::is_null(ctrls) {
            let p = if m.controls.is_empty() {
                mi::FatPtr::null()
            } else {
                give_controls(s, j, codec, &m.controls)
            };
            mem::put_ptr(j, ctrls, p);
        }
        h(s, k).set_error(n, Some(r.matched_dn), Some(r.message));
        maybe_free(s, j, res, freeit);
        ret(LDAP_SUCCESS)
    })
}

/// `ldap_parse_extended_result` and its `_w_controls` form: the
/// response's name and value (and controls).
#[allow(clippy::too_many_arguments)]
pub fn parse_extended(
    j: &mut Job,
    ld: u128,
    res: u128,
    oidp: u128,
    datap: u128,
    ctrls: u128,
    freeit: i64,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let Some((_, m)) = result_msg(s, j, res) else {
            return ret(LDAP_NO_RESULTS_RETURNED);
        };
        let ProtocolOp::ExtendedResponse {
            result,
            name,
            value,
        } = &m.op
        else {
            return ret(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
        };
        let codec = s.codec_of(Some(k));
        if !mem::is_null(oidp) {
            let p = name
                .as_ref()
                .map_or(mi::FatPtr::null(), |n| give_str(s, j, codec, n));
            mem::put_ptr(j, oidp, p);
        }
        if !mem::is_null(datap) {
            let p = value
                .as_ref()
                .map_or(mi::FatPtr::null(), |v| give_berval(s, j, v));
            mem::put_ptr(j, datap, p);
        }
        if !mem::is_null(ctrls) {
            let p = if m.controls.is_empty() {
                mi::FatPtr::null()
            } else {
                give_controls(s, j, codec, &m.controls)
            };
            mem::put_ptr(j, ctrls, p);
        }
        let n = i32::try_from(result.code.number()).unwrap_or(LDAP_OTHER);
        h(s, k).set_error(
            n,
            Some(result.matched_dn.clone()),
            Some(result.message.clone()),
        );
        maybe_free(s, j, res, freeit);
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_parse_extended_result(LDAP *ld, LDAPMessage *res, char
/// **resultoidp, struct berval **resultdatap, int freeit)`.
pub fn parse_extended_result(
    j: &mut Job,
    ld: u128,
    res: u128,
    oidp: u128,
    datap: u128,
    freeit: i64,
) -> i64 {
    parse_extended(j, ld, res, oidp, datap, 0, freeit)
}

/// `int ldap_parse_sasl_bind_result(LDAP *ld, LDAPMessage *res, struct
/// berval **servercredp, int freeit)`: the bind's result code.
pub fn parse_sasl_bind_result(j: &mut Job, ld: u128, res: u128, credp: u128, freeit: i64) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let Some((_, m)) = result_msg(s, j, res) else {
            return ret(LDAP_NO_RESULTS_RETURNED);
        };
        let ProtocolOp::BindResponse {
            result,
            server_sasl_creds,
        } = &m.op
        else {
            return ret(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
        };
        if !mem::is_null(credp) {
            let p = server_sasl_creds
                .as_ref()
                .map_or(mi::FatPtr::null(), |v| give_berval(s, j, v));
            mem::put_ptr(j, credp, p);
        }
        h(s, k).bind_controls.clone_from(&m.controls);
        let n = i32::try_from(result.code.number()).unwrap_or(LDAP_OTHER);
        h(s, k).set_error(
            n,
            Some(result.matched_dn.clone()),
            Some(result.message.clone()),
        );
        maybe_free(s, j, res, freeit);
        ret(n)
    })
}

/// `int ldap_parse_reference(LDAP *ld, LDAPMessage *ref, char
/// ***referralsp, LDAPControl ***serverctrlsp, int freeit)` and its
/// `_np` form.
pub fn parse_reference(
    j: &mut Job,
    ld: u128,
    rf: u128,
    refs: u128,
    ctrls: u128,
    freeit: i64,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let Some((_, _, m)) = message(s, j, rf) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let ProtocolOp::SearchResultReference(urls) = &m.op else {
            return ret(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
        };
        let codec = s.codec_of(Some(k));
        if !mem::is_null(refs) {
            let p = give_strings(s, j, codec, urls);
            mem::put_ptr(j, refs, p);
        }
        if !mem::is_null(ctrls) {
            let p = if m.controls.is_empty() {
                mi::FatPtr::null()
            } else {
                give_controls(s, j, codec, &m.controls)
            };
            mem::put_ptr(j, ctrls, p);
        }
        maybe_free(s, j, rf, freeit);
        ret(LDAP_SUCCESS)
    })
}
