// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldap_set_option` and `ldap_get_option`. A null handle reaches the
//! defaults new handles take. Integer options take a pointer to an
//! `int`; on/off options a pointer to an `int` (or `LDAP_OPT_ON`'s
//! pointer value; null is off). Options QS/400 does not build answer
//! `LDAP_NOT_SUPPORTED`.

use mi::Job;

use super::{give_controls, give_str, key, ret, with};
use crate::consts::*;
use crate::mem;
use crate::state::{Opts, State};

/// An on/off value: null is off, an `int` behind the pointer says,
/// and a pointer that addresses nothing (`LDAP_OPT_ON`) is on.
fn onoff(j: &Job, p: u128) -> bool {
    if mem::is_null(p) {
        return false;
    }
    mem::get_i32(j, p).is_none_or(|v| v != 0)
}

fn int_in(j: &Job, p: u128) -> Result<i32, i32> {
    mem::get_i32(j, p).ok_or(LDAP_PARAM_ERROR)
}

fn set_opts(o: &mut Opts, j: &Job, opt: i32, v: u128, codec: mem::Codec) -> Result<(), i32> {
    match opt {
        LDAP_OPT_DEREF => {
            let n = int_in(j, v)?;
            if !(LDAP_DEREF_NEVER..=LDAP_DEREF_ALWAYS).contains(&n) {
                return Err(LDAP_PARAM_ERROR);
            }
            o.deref = n;
        }
        LDAP_OPT_SIZELIMIT => o.sizelimit = int_in(j, v)?.max(0),
        LDAP_OPT_TIMELIMIT => o.timelimit = int_in(j, v)?.max(0),
        LDAP_OPT_REFHOPLIMIT => o.refhoplimit = int_in(j, v)?.max(0),
        LDAP_OPT_SSL_TIMEOUT => o.ssl_timeout = int_in(j, v)?.max(1),
        LDAP_OPT_PROTOCOL_VERSION => {
            let n = int_in(j, v)?;
            if !(LDAP_VERSION2..=LDAP_VERSION3).contains(&n) {
                return Err(LDAP_PARAM_ERROR);
            }
            o.version = n;
        }
        LDAP_OPT_REFERRALS => o.referrals = onoff(j, v),
        LDAP_OPT_UTF8_IO => o.utf8 = onoff(j, v),
        // Interrupted reads always restart.
        LDAP_OPT_RESTART => {
            if !onoff(j, v) {
                return Err(LDAP_NOT_SUPPORTED);
            }
        }
        LDAP_OPT_SERVER_CONTROLS => o.sctrls = mem::read_controls(j, v, codec),
        LDAP_OPT_CLIENT_CONTROLS => o.cctrls = mem::read_controls(j, v, codec),
        _ => return Err(LDAP_NOT_SUPPORTED),
    }
    Ok(())
}

/// `int ldap_set_option(LDAP *ld, int option, const void *invalue)`.
pub fn set_option(j: &mut Job, ld: u128, opt: i64, v: u128) -> i64 {
    let opt = mem::int(opt);
    with(j, |s, j| {
        let k = key(s, j, ld);
        if k.is_none() && !mem::is_null(ld) {
            return ret(LDAP_PARAM_ERROR);
        }
        let codec = s.codec_of(k);
        let r = match (opt, k) {
            (LDAP_OPT_ERROR_NUMBER, None) => int_in(j, v).map(|n| s.errno = n),
            (LDAP_OPT_ERROR_NUMBER, Some(k)) => int_in(j, v).map(|n| super::h(s, k).errno = n),
            (LDAP_OPT_ERROR_STRING | LDAP_OPT_MATCHED_DN, Some(k)) => {
                let t = codec.arg(j, v);
                let hd = super::h(s, k);
                if opt == LDAP_OPT_ERROR_STRING {
                    hd.errmsg = t;
                } else {
                    hd.matched = t;
                }
                Ok(())
            }
            (LDAP_OPT_HOST_NAME, Some(k)) => {
                let t = codec.arg(j, v);
                let hd = super::h(s, k);
                if hd.client.is_some() {
                    Err(LDAP_PARAM_ERROR)
                } else {
                    let port = hd.hosts.first().map_or(389, |h| h.1);
                    hd.hosts = super::session::hosts(t.as_deref(), port);
                    Ok(())
                }
            }
            (_, None) => set_opts(&mut s.defaults, j, opt, v, codec),
            (_, Some(k)) => {
                let hd = super::h(s, k);
                let r = set_opts(&mut hd.opts, j, opt, v, codec);
                hd.sync();
                r
            }
        };
        ret(r.err().unwrap_or(LDAP_SUCCESS))
    })
}

fn get(s: &mut State, j: &mut Job, k: Option<u64>, opt: i32, out: u128) -> Result<(), i32> {
    let codec = s.codec_of(k);
    let o = match k {
        Some(k) => super::h(s, k).opts.clone(),
        None => s.defaults.clone(),
    };
    let int = |j: &Job, v: i32| {
        if mem::put_i32(j, out, v) {
            Ok(())
        } else {
            Err(LDAP_PARAM_ERROR)
        }
    };
    let give = |s: &mut State, j: &mut Job, t: Option<String>| {
        let p = match t {
            Some(t) => give_str(s, j, codec, &t),
            None => mi::FatPtr::null(),
        };
        if mem::put_ptr(j, out, p) {
            Ok(())
        } else {
            Err(LDAP_PARAM_ERROR)
        }
    };
    match opt {
        LDAP_OPT_DEREF => int(j, o.deref),
        LDAP_OPT_SIZELIMIT => int(j, o.sizelimit),
        LDAP_OPT_TIMELIMIT => int(j, o.timelimit),
        LDAP_OPT_REFHOPLIMIT => int(j, o.refhoplimit),
        LDAP_OPT_SSL_TIMEOUT => int(j, o.ssl_timeout),
        LDAP_OPT_PROTOCOL_VERSION => int(j, o.version),
        LDAP_OPT_REFERRALS => int(j, i32::from(o.referrals)),
        LDAP_OPT_UTF8_IO => int(j, i32::from(o.utf8)),
        LDAP_OPT_RESTART => int(j, LDAP_OPT_ON),
        LDAP_OPT_SERVER_CONTROLS | LDAP_OPT_CLIENT_CONTROLS => {
            let list = if opt == LDAP_OPT_SERVER_CONTROLS {
                o.sctrls
            } else {
                o.cctrls
            };
            let p = if list.is_empty() {
                mi::FatPtr::null()
            } else {
                give_controls(s, j, codec, &list)
            };
            if mem::put_ptr(j, out, p) {
                Ok(())
            } else {
                Err(LDAP_PARAM_ERROR)
            }
        }
        LDAP_OPT_ERROR_NUMBER => {
            let e = k.map_or(s.errno, |k| super::h(s, k).errno);
            int(j, e)
        }
        LDAP_OPT_EXT_ERROR => {
            let e = k.map_or(0, |k| super::h(s, k).ext_error);
            int(j, e)
        }
        LDAP_OPT_ERROR_STRING | LDAP_OPT_MATCHED_DN | LDAP_OPT_HOST_NAME => {
            let Some(k) = k else {
                return Err(LDAP_PARAM_ERROR);
            };
            let hd = super::h(s, k);
            let t = match opt {
                LDAP_OPT_ERROR_STRING => hd.errmsg.clone(),
                LDAP_OPT_MATCHED_DN => hd.matched.clone(),
                _ => Some(hd.host_used.clone().unwrap_or_else(|| {
                    hd.hosts
                        .iter()
                        .map(|(h, p)| format!("{h}:{p}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })),
            };
            give(s, j, t)
        }
        _ => Err(LDAP_NOT_SUPPORTED),
    }
}

/// `int ldap_get_option(LDAP *ld, int option, void *outvalue)`.
pub fn get_option(j: &mut Job, ld: u128, opt: i64, out: u128) -> i64 {
    let opt = mem::int(opt);
    with(j, |s, j| {
        let k = key(s, j, ld);
        if k.is_none() && !mem::is_null(ld) {
            return ret(LDAP_PARAM_ERROR);
        }
        ret(get(s, j, k, opt, out).err().unwrap_or(LDAP_SUCCESS))
    })
}
