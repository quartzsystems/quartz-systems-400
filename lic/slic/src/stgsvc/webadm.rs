// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTTP Server Administration's requests (`net/httpadmin`, the `ADMIN`
//! job): the audit entries it writes for its web users.

use sec::audit::{Event, Kind};

/// The profile the administration server runs as.
pub const SERVER_USER: &str = "QTMHHTTP";

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

/// The event for `web_audit`; `None` when the kind is not one it writes.
pub fn event(kind: &str, user: &str, a: &str, b: &str) -> Option<Event> {
    let e = match kind {
        "PW" => Event::new(Kind::Pw).set("ETYP", a).set("USRP", user),
        "AF" => on(Event::new(Kind::Af).set("VIOL", "A"), a).set("OPER", format!("httpadmin {b}")),
        "ZC" => on(Event::new(Kind::Zc).set("ACCY", "C"), a).set("OPER", b),
        _ => return None,
    };
    Some(e.by(user))
}

/// Record `web_audit`'s entry when the job is the administration
/// server's.
pub fn audit(job_user: &str, kind: &str, user: &str, a: &str, b: &str) {
    if job_user != SERVER_USER {
        return;
    }
    if let Some(e) = event(kind, user, a, b) {
        sec::audit::record(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries() {
        let e = event("AF", "BOB", "QHTTPSVR/ADMIN", "*IOSYSCFG").unwrap();
        assert_eq!((e.get("ONAM"), e.get("OLIB"), e.get("VIOL")), ("ADMIN", "QHTTPSVR", "A"));
        assert_eq!(e.user.as_deref(), Some("BOB"));
        let p = event("PW", "BOB", "R", "").unwrap();
        assert_eq!((p.get("ETYP"), p.get("USRP")), ("R", "BOB"));
        let z = event("ZC", "BOB", "/www/web1/conf/httpd.conf", "apply").unwrap();
        assert_eq!((z.get("ONAM"), z.path()), ("httpd.conf", "/www/web1/conf/httpd.conf"));
        assert!(event("XX", "BOB", "", "").is_none());
    }
}
