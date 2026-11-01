// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Managing instances: starting, stopping, deleting, creating and
//! changing their configuration through the pages.

mod common;

use common::Browser;
use httpadmin::env::Audit;
use httpadmin::fixture::PASSWORD;
use httpadmin::settings::Settings;
use httpd::files::Files;

/// The fixture with `WEBUSER` allowed to sign on through the
/// administrators group, but without `*IOSYSCFG`.
fn group_member() -> Browser {
    let mut b = Browser::new();
    let s = Settings {
        admin_group: "WEBUSERS".into(),
        ..Settings::default()
    };
    s.save(b.fx.files.as_ref()).unwrap();
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.status, 303, "{}", r.text());
    b
}

fn running(b: &Browser, name: &str) -> bool {
    b.fx.jobs.jobs.lock().unwrap().iter().any(|j| j.instance == name)
}

fn number(b: &Browser, name: &str) -> String {
    b.fx.jobs.jobs.lock().unwrap().iter().find(|j| j.instance == name).map(|j| j.number.clone()).unwrap_or_default()
}

/// The message waiting on the instance list after a redirect.
fn shown(b: &mut Browser, r: &qsweb::http::Response) -> String {
    assert_eq!(r.status, 303, "{}", r.text());
    let to = r.header("Location").unwrap().to_string();
    b.get(&to).text()
}

fn refused(b: &Browser) -> bool {
    b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { user, .. } if user == "WEBUSER"))
}

#[test]
fn start_stop_restart() {
    let mut b = Browser::signed_on();
    // GET only shows the confirmation.
    assert_eq!(b.get("/instance/INTRANET/confirm?op=stop").status, 200);
    assert!(running(&b, "INTRANET"));
    // No token, no action.
    assert_eq!(b.post_raw("/instance/INTRANET/confirm?op=stop", &[("ok", "OK")]).status, 403);
    assert!(running(&b, "INTRANET"));
    // Cancel goes back untouched.
    let r = b.post("/instance/INTRANET/confirm?op=stop", &[("cancel", "Cancel")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    assert!(running(&b, "INTRANET"));
    // A bad delay is refused.
    let r = b.post("/instance/INTRANET/confirm?op=stop", &[("how", "*CNTRLD"), ("delay", "x"), ("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8120"));
    assert!(running(&b, "INTRANET"));
    let r = b.post("/instance/INTRANET/confirm?op=stop", &[("how", "*CNTRLD"), ("delay", "30"), ("ok", "OK")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("HTP8002") && t.contains("HTTP server instance INTRANET ended."), "{t}");
    assert!(!running(&b, "INTRANET"));
    let reg = httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap();
    assert_eq!(reg.get("INTRANET").unwrap().ended_how, "ENDTCPSVR *HTTP, controlled");
    // Start it again.
    let r = b.post("/instance/INTRANET/confirm?op=start", &[("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8001"));
    assert!(running(&b, "INTRANET"));
    // Restart: a new job.
    let before = number(&b, "API");
    let r = b.post("/instance/API/confirm?op=restart", &[("how", "*IMMED"), ("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8001"));
    assert_ne!(number(&b, "API"), before);
}

#[test]
fn admin_cannot_stop_itself() {
    let mut b = Browser::signed_on();
    for op in ["stop", "restart"] {
        let r = b.post(&format!("/instance/ADMIN/confirm?op={op}"), &[("how", "*IMMED"), ("ok", "OK")]);
        assert!(shown(&mut b, &r).contains("HTP8104"));
        assert!(running(&b, "ADMIN"));
    }
}

#[test]
fn actions_need_iosyscfg() {
    let mut b = group_member();
    assert_eq!(b.get("/instance/INTRANET/confirm?op=stop").status, 200);
    let r = b.post("/instance/INTRANET/confirm?op=stop", &[("how", "*IMMED"), ("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("HTP8110") && r.text().contains("*IOSYSCFG"));
    assert!(running(&b, "INTRANET"));
    assert!(refused(&b));
    let r = b.post("/instance/WEBSTORE/confirm?op=delete", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(b.fx.files.exists("/www/webstore/conf/httpd.conf"));
}

#[test]
fn delete() {
    let mut b = Browser::signed_on();
    let t = b.get("/instance/WEBSTORE/confirm?op=delete").text();
    assert!(t.contains("14 files, 212 MB"));
    let r = b.post("/instance/INTRANET/confirm?op=delete", &[("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8103"));
    assert!(b.fx.files.exists("/www/intranet/conf/httpd.conf"));
    let r = b.post("/instance/WEBSTORE/confirm?op=delete", &[("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8021"));
    assert!(!b.fx.files.exists("/www/webstore"));
    assert_eq!(b.get("/instance/WEBSTORE/general").status, 404);
    assert!(!b.get("/instances").text().contains(">WEBSTORE</a>"));
}
