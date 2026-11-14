// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Searches in every form: `ldap_search` (message ID), `_s`, `_st`
//! (with a timeout), `_ext` and `_ext_s` (controls, a timeout that is
//! also the server's time limit, a size limit), and by URL. The
//! synchronous forms follow referrals when `LDAP_OPT_REFERRALS` is on;
//! the asynchronous ones return references for the program.

use std::time::Duration;

use ldap::Filter;
use ldap::control::Control;
use ldap::msg::{LdapMessage, ProtocolOp, Scope, SearchRequest};
use ldap::url::LdapUrl;
use mi::Job;

use super::result::{give_chain, wait_all};
use super::{h, on, ret, text, timeval};
use crate::consts::*;
use crate::mem;
use crate::state::State;

/// What a search call asks for, before it is a request.
pub struct Ask {
    /// The base.
    pub base: u128,
    /// The scope.
    pub scope: i64,
    /// The filter string.
    pub filter: u128,
    /// The attributes (`char **`).
    pub attrs: u128,
    /// Types only.
    pub attrsonly: i64,
}

fn scope(n: i32) -> Option<Scope> {
    match n {
        LDAP_SCOPE_BASE => Some(Scope::Base),
        LDAP_SCOPE_ONELEVEL => Some(Scope::One),
        LDAP_SCOPE_SUBTREE => Some(Scope::Sub),
        _ => None,
    }
}

/// The request of a call; the error code when it cannot be one.
pub fn request(
    s: &mut State,
    j: &Job,
    k: u64,
    a: &Ask,
    size: Option<i32>,
    timeout: Option<Duration>,
) -> Result<SearchRequest, i32> {
    let base = text(s, j, Some(k), a.base).unwrap_or_default();
    let Some(sc) = scope(mem::int(a.scope)) else {
        return Err(h(s, k).set_error(
            LDAP_PARAM_ERROR,
            None,
            Some("the scope is not valid".into()),
        ));
    };
    let ftext = text(s, j, Some(k), a.filter).unwrap_or_else(|| "(objectClass=*)".into());
    let filter = match Filter::parse(&ftext) {
        Ok(f) => f,
        Err(e) => return Err(h(s, k).set_error(LDAP_FILTER_ERROR, None, Some(e.to_string()))),
    };
    let codec = s.codec_of(Some(k));
    let o = &h(s, k).opts;
    let mut r = SearchRequest::new(base, sc, filter);
    r.deref = crate::state::deref(o.deref);
    r.size_limit = size.unwrap_or(o.sizelimit);
    r.time_limit = match timeout {
        Some(t) if !t.is_zero() => {
            i32::try_from(t.as_secs() + u64::from(t.subsec_nanos() > 0)).unwrap_or(i32::MAX)
        }
        _ => o.timelimit,
    };
    r.types_only = mem::int(a.attrsonly) != 0;
    r.attrs = codec.strings(j, a.attrs);
    Ok(r)
}

/// Sends a search; its message ID.
pub fn start(
    s: &mut State,
    k: u64,
    r: SearchRequest,
    ctrls: Option<Vec<Control>>,
) -> Result<i32, i32> {
    h(s, k).send(ProtocolOp::SearchRequest(r), ctrls)
}

/// Runs a search to its end; the messages.
pub fn run(
    s: &mut State,
    k: u64,
    r: SearchRequest,
    ctrls: Option<Vec<Control>>,
    timeout: Option<Duration>,
) -> Result<Vec<LdapMessage>, i32> {
    let hd = h(s, k);
    if !hd.opts.referrals {
        let id = hd.send(ProtocolOp::SearchRequest(r), ctrls)?;
        return wait_all(hd, id, timeout);
    }
    let ctrls = ctrls.unwrap_or_else(|| hd.opts.sctrls.clone());
    let c = hd.connect()?;
    let keep = c.options.timeout;
    c.options.timeout = timeout;
    let out = c.search(&r, &ctrls);
    c.options.timeout = keep;
    let out = match out {
        Ok(o) => o,
        Err(e) => return Err(hd.fail(&e)),
    };
    let mut msgs = Vec::new();
    for e in out.entries {
        msgs.push(LdapMessage {
            id: 0,
            op: ProtocolOp::SearchResultEntry {
                dn: e.dn,
                attrs: e.attrs,
            },
            controls: e.controls,
        });
    }
    for r in out.references {
        msgs.push(LdapMessage::new(0, ProtocolOp::SearchResultReference(r)));
    }
    msgs.push(LdapMessage {
        id: 0,
        op: ProtocolOp::SearchResultDone(out.result),
        controls: out.controls,
    });
    Ok(msgs)
}

/// `int ldap_search(LDAP *ld, const char *base, int scope, const char
/// *filter, char **attrs, int attrsonly)`: the message ID, -1 on an
/// error.
pub fn search(
    j: &mut Job,
    ld: u128,
    base: u128,
    sc: i64,
    filter: u128,
    attrs: u128,
    only: i64,
) -> i64 {
    on(j, ld, -1, |s, j, k| {
        let a = Ask {
            base,
            scope: sc,
            filter,
            attrs,
            attrsonly: only,
        };
        request(s, j, k, &a, None, None)
            .and_then(|r| start(s, k, r, None))
            .map_or(-1, i64::from)
    })
}

fn sync(j: &mut Job, ld: u128, a: &Ask, tv: Option<u128>, res: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        mem::put_ptr(j, res, mi::FatPtr::null());
        let t = tv.and_then(|p| timeval(j, p));
        let r = match request(s, j, k, a, None, t) {
            Ok(r) => r,
            Err(e) => return ret(e),
        };
        match run(s, k, r, None, t) {
            Ok(m) => ret(give_chain(s, j, k, m, res)),
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_search_s(LDAP *ld, const char *base, int scope, const char
/// *filter, char **attrs, int attrsonly, LDAPMessage **res)`.
#[allow(clippy::too_many_arguments)]
pub fn search_s(
    j: &mut Job,
    ld: u128,
    base: u128,
    sc: i64,
    filter: u128,
    attrs: u128,
    only: i64,
    res: u128,
) -> i64 {
    sync(
        j,
        ld,
        &Ask {
            base,
            scope: sc,
            filter,
            attrs,
            attrsonly: only,
        },
        None,
        res,
    )
}

/// `int ldap_search_st(LDAP *ld, const char *base, int scope, const
/// char *filter, char **attrs, int attrsonly, struct timeval *timeout,
/// LDAPMessage **res)`.
#[allow(clippy::too_many_arguments)]
pub fn search_st(
    j: &mut Job,
    ld: u128,
    base: u128,
    sc: i64,
    filter: u128,
    attrs: u128,
    only: i64,
    tv: u128,
    res: u128,
) -> i64 {
    sync(
        j,
        ld,
        &Ask {
            base,
            scope: sc,
            filter,
            attrs,
            attrsonly: only,
        },
        Some(tv),
        res,
    )
}

/// The size limit argument of an `_ext` call: -1 (or less) takes the
/// handle's, 0 is none.
fn size_arg(n: i64) -> Option<i32> {
    let n = mem::int(n);
    (n >= 0).then_some(n)
}

/// `int ldap_search_ext(LDAP *ld, const char *base, int scope, const
/// char *filter, char **attrs, int attrsonly, LDAPControl
/// **serverctrls, LDAPControl **clientctrls, struct timeval *timeout,
/// int sizelimit, int *msgidp)`.
#[allow(clippy::too_many_arguments)]
pub fn search_ext(
    j: &mut Job,
    ld: u128,
    base: u128,
    sc: i64,
    filter: u128,
    attrs: u128,
    only: i64,
    sctrls: u128,
    cctrls: u128,
    tv: u128,
    size: i64,
    msgidp: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let ctrls = match super::control::op_controls(s, j, k, sctrls, cctrls) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let a = Ask {
            base,
            scope: sc,
            filter,
            attrs,
            attrsonly: only,
        };
        let t = timeval(j, tv);
        let r = match request(s, j, k, &a, size_arg(size), t) {
            Ok(r) => r,
            Err(e) => return ret(e),
        };
        match start(s, k, r, ctrls) {
            Ok(id) => {
                mem::put_i32(j, msgidp, id);
                ret(LDAP_SUCCESS)
            }
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_search_ext_s(LDAP *ld, const char *base, int scope, const
/// char *filter, char **attrs, int attrsonly, LDAPControl
/// **serverctrls, LDAPControl **clientctrls, struct timeval *timeout,
/// int sizelimit, LDAPMessage **res)`.
#[allow(clippy::too_many_arguments)]
pub fn search_ext_s(
    j: &mut Job,
    ld: u128,
    base: u128,
    sc: i64,
    filter: u128,
    attrs: u128,
    only: i64,
    sctrls: u128,
    cctrls: u128,
    tv: u128,
    size: i64,
    res: u128,
) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        mem::put_ptr(j, res, mi::FatPtr::null());
        let ctrls = match super::control::op_controls(s, j, k, sctrls, cctrls) {
            Ok(c) => c,
            Err(e) => return ret(e),
        };
        let a = Ask {
            base,
            scope: sc,
            filter,
            attrs,
            attrsonly: only,
        };
        let t = timeval(j, tv);
        let r = match request(s, j, k, &a, size_arg(size), t) {
            Ok(r) => r,
            Err(e) => return ret(e),
        };
        match run(s, k, r, ctrls, t) {
            Ok(m) => ret(give_chain(s, j, k, m, res)),
            Err(e) => ret(e),
        }
    })
}

/// The request of an LDAP URL against the handle: its server must be
/// the handle's (an unconnected handle takes it).
fn url_request(s: &mut State, j: &Job, k: u64, url: u128, only: i64) -> Result<SearchRequest, i32> {
    let Some(t) = text(s, j, Some(k), url) else {
        return Err(h(s, k).set_error(LDAP_PARAM_ERROR, None, None));
    };
    let u = match LdapUrl::parse(&t) {
        Ok(u) => u,
        Err(e) => return Err(h(s, k).set_error(LDAP_PARAM_ERROR, None, Some(e.to_string()))),
    };
    let hd = h(s, k);
    if let Some(host) = &u.host {
        let port = u.port_or_default();
        if hd.client.is_none() {
            hd.hosts = vec![(host.clone(), port)];
        } else if !hd
            .host_used
            .as_deref()
            .is_some_and(|x| x.eq_ignore_ascii_case(host))
        {
            return Err(hd.set_error(
                LDAP_PARAM_ERROR,
                None,
                Some(format!("the URL names {host}, not this session's server")),
            ));
        }
    }
    let filter = match u.filter() {
        Ok(f) => f,
        Err(e) => return Err(hd.set_error(LDAP_FILTER_ERROR, None, Some(e.to_string()))),
    };
    let mut r = SearchRequest::new(u.dn.clone(), u.scope.unwrap_or(Scope::Base), filter);
    r.deref = crate::state::deref(hd.opts.deref);
    r.size_limit = hd.opts.sizelimit;
    r.time_limit = hd.opts.timelimit;
    r.types_only = mem::int(only) != 0;
    r.attrs.clone_from(&u.attrs);
    Ok(r)
}

/// `int ldap_url_search(LDAP *ld, const char *url, int attrsonly)`:
/// the message ID, -1 on an error.
pub fn url_search(j: &mut Job, ld: u128, url: u128, only: i64) -> i64 {
    on(j, ld, -1, |s, j, k| {
        url_request(s, j, k, url, only)
            .and_then(|r| start(s, k, r, None))
            .map_or(-1, i64::from)
    })
}

/// `int ldap_url_search_st(LDAP *ld, const char *url, int attrsonly,
/// struct timeval *timeout, LDAPMessage **res)`.
pub fn url_search_st(j: &mut Job, ld: u128, url: u128, only: i64, tv: u128, res: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        mem::put_ptr(j, res, mi::FatPtr::null());
        let t = timeval(j, tv);
        let r = match url_request(s, j, k, url, only) {
            Ok(r) => r,
            Err(e) => return ret(e),
        };
        match run(s, k, r, None, t) {
            Ok(m) => ret(give_chain(s, j, k, m, res)),
            Err(e) => ret(e),
        }
    })
}

/// `int ldap_url_search_s(LDAP *ld, const char *url, int attrsonly,
/// LDAPMessage **res)`.
pub fn url_search_s(j: &mut Job, ld: u128, url: u128, only: i64, res: u128) -> i64 {
    url_search_st(j, ld, url, only, 0, res)
}
