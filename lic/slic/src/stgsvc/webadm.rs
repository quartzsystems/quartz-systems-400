// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTP Server Administration's requests (`net/httpadmin`, the `ADMIN`
//! job): the audit entries it writes for its web users, and its
//! certificate calls, run by `httpadmin::certmgr` over the store files
//! `certs.rs` stages (the stashes open with the LIC key, held here).

use certs::admin::Ctx;
use certs::paths::{LOCAL_CA_STORE, USER_STORES};
use httpadmin::certmgr::wire::{self, CertCall, CertReply};
use httpadmin::certmgr::{Manager, SYSTEM_STORES, local_signer, user_stores};
use httpadmin::env::CertFailure;
use sec::audit::{Event, Kind};
use sec::auth::principal::Principal;

use crate::ipl::system::System;

/// The profile the administration server runs as.
pub const SERVER_USER: &str = "QTMHHTTP";

/// The store files a call may touch: the system's stores, the stores
/// someone made, the local CA's and a new store's.
fn store_paths(system: &mut System, call: &CertCall) -> Vec<String> {
    let mut v: Vec<String> = SYSTEM_STORES.iter().map(|s| s.2.to_string()).collect();
    v.extend([USER_STORES.to_string(), LOCAL_CA_STORE.to_string()]);
    let made =
        super::certs::with_stores(system, &[USER_STORES], false, user_stores).unwrap_or_default();
    v.extend(made.into_iter().map(|(_, p)| p));
    if let CertCall::CreateStore { path, .. } = call {
        v.push(path.clone());
    }
    v
}

/// `web_certs`: the call run for the job's user.
pub fn certs(system: &mut System, principal: &Principal, call: &[u8]) -> Vec<u8> {
    let refused = |t: String| CertReply::Refused(CertFailure::Invalid(t)).encode();
    let Some(call) = CertCall::decode(call) else {
        return refused("the certificate call was not understood".into());
    };
    // The server reads, and registers its instances' application IDs.
    let server = principal.user == SERVER_USER && (!call.changes() || call.by_server());
    if !server && super::certs::may_manage(Some(principal)).is_err() {
        return refused(format!(
            "user {} needs the *ALLOBJ and *SECADM special authorities to manage certificates",
            principal.user
        ));
    }
    let paths = store_paths(system, &call);
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let changes = call.changes();
    let r = super::certs::with_stores(system, &refs, changes, |ctx| {
        let m = Manager {
            ctx: Ctx {
                files: ctx.files,
                sealer: ctx.sealer,
                now: ctx.now,
                params: ctx.params,
            },
            signers: local_signer(ctx).into_iter().collect(),
            shown: None,
        };
        wire::answer(&m, call)
    });
    match r {
        Ok(reply) => reply.encode(),
        Err(e) => refused(e),
    }
}

/// The object fields of `a`: an IFS path, else `LIB/NAME`.
fn on(e: Event, a: &str) -> Event {
    if a.starts_with('/') {
        let name = a.rsplit('/').next().unwrap_or("");
        e.object(name, "", "*STMF").ifs([0; 16], a, None)
    } else {
        let (lib, name) = a.split_once('/').unwrap_or(("", a));
        e.object(name, lib, "*N")
    }
}

/// The event for `web_audit` from the server job `server` (`ADMIN`,
/// `NAVIGATOR`); `None` when the kind is not one it writes.
pub fn event(server: &str, kind: &str, user: &str, a: &str, b: &str) -> Option<Event> {
    let e = match kind {
        "PW" => Event::new(Kind::Pw).set("ETYP", a).set("USRP", user),
        "AF" => on(Event::new(Kind::Af).set("VIOL", "A"), a).set("OPER", format!("{server} {b}")),
        "ZC" => on(Event::new(Kind::Zc).set("ACCY", "C"), a).set("OPER", b),
        _ => return None,
    };
    Some(e.by(user))
}

/// Record `web_audit`'s entry when the job is a web server's
/// (`QTMHHTTP`).
pub fn audit(job_user: &str, server: &str, kind: &str, user: &str, a: &str, b: &str) {
    if job_user != SERVER_USER {
        return;
    }
    if let Some(e) = event(server, kind, user, a, b) {
        sec::audit::record(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries() {
        let e = event("ADMIN", "AF", "BOB", "QHTTPSVR/ADMIN", "*IOSYSCFG").unwrap();
        assert_eq!(
            (e.get("ONAM"), e.get("OLIB"), e.get("VIOL")),
            ("ADMIN", "QHTTPSVR", "A")
        );
        assert_eq!(e.user.as_deref(), Some("BOB"));
        let p = event("ADMIN", "PW", "BOB", "R", "").unwrap();
        assert_eq!((p.get("ETYP"), p.get("USRP")), ("R", "BOB"));
        let z = event("ADMIN", "ZC", "BOB", "/www/web1/conf/httpd.conf", "apply").unwrap();
        assert_eq!(
            (z.get("ONAM"), z.path()),
            ("httpd.conf", "/www/web1/conf/httpd.conf")
        );
        assert!(event("ADMIN", "XX", "BOB", "", "").is_none());
    }
}
