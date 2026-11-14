// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions: `ldap_init`, `ldap_open`, the secure forms (`ldap_ssl_*`,
//! `ldap_app_ssl_*_np`, `StartTLS`), `ldap_unbind*` and `ldap_version`.
//! A handle connects at its first operation (`ldap_open` at once).

use mi::Job;

use super::{h, key, null, on, ptr, ret, text, with};
use crate::consts::*;
use crate::mem;
use crate::state::{Handle, State, TlsWant, tls_config};

/// The servers a host argument names: a blank-separated list of
/// `host`, `host:port`, `[v6]:port` or LDAP URLs; none means the
/// local system.
#[must_use]
pub fn hosts(arg: Option<&str>, port: u16) -> Vec<(String, u16)> {
    let arg = arg
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("localhost");
    let mut out = Vec::new();
    for w in arg.split_whitespace() {
        if let Ok(u) = ldap::url::LdapUrl::parse(w) {
            out.push((
                u.host.clone().unwrap_or_else(|| "localhost".into()),
                u.port.unwrap_or(port),
            ));
            continue;
        }
        if let Some(rest) = w.strip_prefix('[')
            && let Some((h, p)) = rest.split_once(']')
        {
            let p = p
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(port);
            out.push((h.to_string(), p));
            continue;
        }
        match w.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') => {
                out.push((h.to_string(), p.parse().unwrap_or(port)));
            }
            _ => out.push((w.to_string(), port)),
        }
    }
    out
}

fn port_of(p: i64, dft: i32) -> u16 {
    match u16::try_from(p) {
        Ok(0) | Err(_) => u16::try_from(dft).unwrap_or(389),
        Ok(n) => n,
    }
}

fn make(
    s: &mut State,
    j: &mut Job,
    host: u128,
    port: i64,
    dft: i32,
    version: i32,
) -> (u64, mi::FatPtr) {
    let codec = s.codec();
    let host = codec.arg(j, host);
    let mut opts = s.defaults.clone();
    opts.version = version;
    let hs = Handle::new(hosts(host.as_deref(), port_of(port, dft)), opts);
    let p = s.new_handle(j, hs);
    let k = mem::native(j, mem::raw(p)).expect("just made");
    (k, p)
}

/// `LDAP *ldap_init(const char *host, int port)`.
pub fn init(j: &mut Job, host: u128, port: i64) -> u128 {
    with(j, |s, j| {
        ptr(make(s, j, host, port, LDAP_PORT, LDAP_VERSION3).1)
    })
}

/// `LDAP *ldap_open(const char *host, int port)`: connected, LDAP version 2 as
/// the API defines it.
pub fn open(j: &mut Job, host: u128, port: i64) -> u128 {
    with(j, |s, j| {
        let (k, p) = make(s, j, host, port, LDAP_PORT, LDAP_VERSION2);
        match h(s, k).connect() {
            Ok(_) => ptr(p),
            Err(code) => {
                s.errno = code;
                s.drop_handle(j, k);
                null()
            }
        }
    })
}

/// `LDAP *ldap_ssl_init(char *host, int port, char *name)`: LDAPS with
/// the environment `ldap_ssl_client_init` set up, `name` the client
/// certificate's label.
pub fn ssl_init(j: &mut Job, host: u128, port: i64, name: u128) -> u128 {
    with(j, |s, j| {
        let Some(env) = s.ssl.clone() else {
            s.errno = LDAP_SSL_CLIENT_INIT_NOT_CALLED;
            return null();
        };
        let label = s.codec().arg(j, name);
        let (k, p) = make(s, j, host, port, LDAPS_PORT, LDAP_VERSION3);
        h(s, k).tls = Some(with_label(env, label));
        ptr(p)
    })
}

fn with_label(w: TlsWant, label: Option<String>) -> TlsWant {
    match (w, label) {
        (TlsWant::System { .. }, l) => TlsWant::System { label: l },
        (TlsWant::KeyRing { path, password, .. }, l) => TlsWant::KeyRing {
            path,
            password,
            label: l,
        },
        (w @ TlsWant::App(_), _) => w,
    }
}

fn set_env(j: &Job, s: &mut State, want: TlsWant, reason: u128) -> i64 {
    if s.ssl.is_some() {
        mem::put_i32(j, reason, 0);
        return ret(LDAP_SSL_ALREADY_INITIALIZED);
    }
    match tls_config(&want) {
        Ok(_) => {
            s.ssl = Some(want);
            mem::put_i32(j, reason, 0);
            ret(LDAP_SUCCESS)
        }
        Err(e) => {
            tracing::debug!(error = %e, "ldap secure environment");
            mem::put_i32(j, reason, 1);
            s.errno = LDAP_SSL_INITIALIZE_FAILED;
            ret(LDAP_SSL_INITIALIZE_FAILED)
        }
    }
}

fn keyring(s: &State, j: &Job, keyring: u128, pw: u128) -> TlsWant {
    let c = s.codec();
    match c.arg(j, keyring) {
        None => TlsWant::System { label: None },
        Some(path) => TlsWant::KeyRing {
            path,
            password: c.arg(j, pw),
            label: None,
        },
    }
}

/// `int ldap_ssl_client_init(char *keyring, char *keyring_pw, int
/// ssl_timeout, int *reason)`: a null key ring is the `*SYSTEM` store.
pub fn ssl_client_init(j: &mut Job, kr: u128, pw: u128, timeout: i64, reason: u128) -> i64 {
    with(j, |s, j| {
        let want = keyring(s, j, kr, pw);
        let t = mem::int(timeout);
        if t > 0 {
            s.defaults.ssl_timeout = t;
        }
        set_env(j, s, want, reason)
    })
}

/// `int ldap_ssl_environment_init(char *keyring, char *keyring_pw, char
/// *keyring_stash, int ssl_timeout, int *reason)`: as
/// `ldap_ssl_client_init`; a stash file is refused.
pub fn ssl_environment_init(
    j: &mut Job,
    kr: u128,
    pw: u128,
    stash: u128,
    timeout: i64,
    reason: u128,
) -> i64 {
    if !mem::is_null(stash) {
        mem::put_i32(j, reason, 0);
        return ret(LDAP_NOT_SUPPORTED);
    }
    ssl_client_init(j, kr, pw, timeout, reason)
}

/// `int ldap_app_ssl_client_init_np(char *app_id, int *reason)`.
pub fn app_ssl_client_init(j: &mut Job, app: u128, reason: u128) -> i64 {
    with(j, |s, j| match s.codec().arg(j, app) {
        Some(id) if !id.trim().is_empty() => {
            set_env(j, s, TlsWant::App(id.trim().to_string()), reason)
        }
        _ => {
            mem::put_i32(j, reason, 0);
            ret(LDAP_SSL_PARAM_ERROR)
        }
    })
}

fn app_want(s: &State, j: &Job, app: u128) -> Option<TlsWant> {
    match s.codec().arg(j, app) {
        Some(id) if !id.trim().is_empty() => Some(TlsWant::App(id.trim().to_string())),
        _ => s.ssl.clone(),
    }
}

/// `LDAP *ldap_app_ssl_init_np(char *host, int port, char *app_id)`: a
/// null ID takes `ldap_app_ssl_client_init_np`'s.
pub fn app_ssl_init(j: &mut Job, host: u128, port: i64, app: u128) -> u128 {
    with(j, |s, j| {
        let Some(want) = app_want(s, j, app) else {
            s.errno = LDAP_SSL_CLIENT_INIT_NOT_CALLED;
            return null();
        };
        let (k, p) = make(s, j, host, port, LDAPS_PORT, LDAP_VERSION3);
        h(s, k).tls = Some(want);
        ptr(p)
    })
}

fn start_ssl(s: &mut State, k: u64, want: TlsWant) -> i64 {
    let hd = h(s, k);
    if hd.client.is_some() {
        return ret(hd.set_error(
            LDAP_PARAM_ERROR,
            None,
            Some("the session is already connected".into()),
        ));
    }
    hd.tls = Some(want);
    ret(hd.set_error(LDAP_SUCCESS, None, None))
}

/// `int ldap_ssl_start(LDAP *ld, char *keyring, char *keyring_pw, char
/// *name)`: the session, not yet connected, connects with LDAPS.
pub fn ssl_start(j: &mut Job, ld: u128, kr: u128, pw: u128, name: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let label = s.codec().arg(j, name);
        let want = if mem::is_null(kr) {
            s.ssl.clone().unwrap_or(TlsWant::System { label: None })
        } else {
            keyring(s, j, kr, pw)
        };
        start_ssl(s, k, with_label(want, label))
    })
}

/// `int ldap_app_ssl_start_np(LDAP *ld, char *app_id)`.
pub fn app_ssl_start(j: &mut Job, ld: u128, app: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        match app_want(s, j, app) {
            Some(w) => start_ssl(s, k, w),
            None => ret(h(s, k).set_error(LDAP_SSL_CLIENT_INIT_NOT_CALLED, None, None)),
        }
    })
}

fn start_tls(s: &mut State, k: u64, want: &TlsWant) -> i64 {
    let cfg = match tls_config(want) {
        Ok(c) => c,
        Err(e) => return ret(h(s, k).set_error(LDAP_SSL_INITIALIZE_FAILED, None, Some(e))),
    };
    let hd = h(s, k);
    let c = match hd.connect() {
        Ok(c) => c,
        Err(code) => return ret(code),
    };
    c.options.tls = Some(cfg.clone());
    match c.start_tls(cfg) {
        Ok(()) => ret(hd.set_error(LDAP_SUCCESS, None, None)),
        Err(e) => ret(hd.fail(&e)),
    }
}

/// `int ldap_start_tls_s_np(LDAP *ld, char *name)`: `StartTLS` with the
/// environment set up (or the `*SYSTEM` store), `name` the client
/// certificate's label.
pub fn start_tls_s(j: &mut Job, ld: u128, name: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        let label = text(s, j, Some(k), name);
        let want = with_label(
            s.ssl.clone().unwrap_or(TlsWant::System { label: None }),
            label,
        );
        start_tls(s, k, &want)
    })
}

/// `int ldap_start_tls_app_np(LDAP *ld, char *app_id)`.
pub fn start_tls_app(j: &mut Job, ld: u128, app: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, j, k| {
        match app_want(s, j, app) {
            Some(w) => start_tls(s, k, &w),
            None => ret(h(s, k).set_error(LDAP_SSL_CLIENT_INIT_NOT_CALLED, None, None)),
        }
    })
}

/// `int ldap_stop_tls_s_np(LDAP *ld)`: LDAP has no way back to the
/// clear on one connection, so the TLS session and the connection end;
/// the next operation connects again in the clear, unbound.
pub fn stop_tls_s(j: &mut Job, ld: u128) -> i64 {
    on(j, ld, ret(LDAP_PARAM_ERROR), |s, _, k| {
        let hd = h(s, k);
        if hd.tls.is_some() {
            return ret(hd.set_error(
                LDAP_PARAM_ERROR,
                None,
                Some("an LDAPS session cannot leave TLS".into()),
            ));
        }
        match hd.client.take() {
            Some(c) if c.is_tls() => {
                let _ = c.unbind();
                hd.outstanding.clear();
                hd.pending.clear();
                ret(hd.set_error(LDAP_SUCCESS, None, None))
            }
            other => {
                hd.client = other;
                ret(hd.set_error(
                    LDAP_PARAM_ERROR,
                    None,
                    Some("the session is not under TLS".into()),
                ))
            }
        }
    })
}

/// `int ldap_unbind(LDAP *ld)` and `ldap_unbind_s`.
pub fn unbind(j: &mut Job, ld: u128) -> i64 {
    with(j, |s, j| match key(s, j, ld) {
        Some(k) => {
            if let Some(hd) = s.drop_handle(j, k)
                && let Some(c) = hd.client
            {
                let _ = c.unbind();
            }
            ret(LDAP_SUCCESS)
        }
        None => ret(LDAP_PARAM_ERROR),
    })
}

/// `int ldap_unbind_ext(LDAP *ld, LDAPControl **sctrls, LDAPControl
/// **cctrls)`.
pub fn unbind_ext(j: &mut Job, ld: u128, sctrls: u128, cctrls: u128) -> i64 {
    with(j, |s, j| {
        let Some(k) = key(s, j, ld) else {
            return ret(LDAP_PARAM_ERROR);
        };
        let codec = s.codec_of(Some(k));
        if let Err(code) = super::control::client_controls(j, codec, cctrls, &[]) {
            return ret(h(s, k).set_error(code, None, None));
        }
        let ctrls = mem::read_controls(j, sctrls, codec);
        if let Some(hd) = s.drop_handle(j, k)
            && let Some(mut c) = hd.client
        {
            let _ = c.send(ldap::ProtocolOp::UnbindRequest, ctrls);
        }
        ret(LDAP_SUCCESS)
    })
}

/// `int ldap_version(LDAPVersion *ver)`: fills `sdk_version` (the
/// release as `VRM`, 120 for V1R2M0), `protocol_version` (3) and
/// `ssl_version` (13, TLS 1.3); returns `sdk_version`.
pub fn version(j: &mut Job, ver: u128) -> i64 {
    let r = release::RELEASE;
    let sdk = i32::from(r.v) * 100 + i32::from(r.r) * 10 + i32::from(r.m);
    if let Some(b) = mem::bytes(j, ver, 12) {
        b[0..4].copy_from_slice(&sdk.to_be_bytes());
        b[4..8].copy_from_slice(&LDAP_VERSION3.to_be_bytes());
        b[8..12].copy_from_slice(&13i32.to_be_bytes());
    }
    i64::from(sdk)
}

#[cfg(test)]
mod tests {
    use super::hosts;

    #[test]
    fn host_lists() {
        assert_eq!(hosts(None, 389), [("localhost".to_string(), 389)]);
        assert_eq!(
            hosts(Some("a b:1000 [::1]:7 ldap://c:99/"), 389),
            [
                ("a".to_string(), 389),
                ("b".to_string(), 1000),
                ("::1".to_string(), 7),
                ("c".to_string(), 99)
            ]
        );
    }
}
