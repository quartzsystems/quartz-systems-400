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

/// Answer the wizard's steps 1 to 5 with `set` over the defaults;
/// gives the last response.
fn wizard(b: &mut Browser, set: &[(&str, &str)]) -> qsweb::http::Response {
    let steps: [&[&str]; 5] = [&["name", "desc", "base", "copyfrom"], &["root"], &["docroot"], &["addr", "ip", "port"], &["log", "logpath", "fmt", "rot"]];
    let defaults = [("name", "SHOP"), ("desc", "Shop"), ("base", "new"), ("port", "8090"), ("addr", "*ALL"), ("log", "yes"), ("docroot", "/www/shop/htdocs")];
    b.post("/create/step/1", &[("create_new_instance", "x")]);
    let mut last = None;
    for (i, keys) in steps.iter().enumerate() {
        let mut f: Vec<(&str, &str)> = Vec::new();
        for k in *keys {
            if let Some((_, v)) = set.iter().chain(defaults.iter()).find(|(n, _)| n == k) {
                f.push((k, v));
            }
        }
        if i == 2 {
            f.push(("sample", "on"));
        }
        f.push(("next", "Next >"));
        let r = b.post(&format!("/create/step/{}", i + 1), &f);
        let to = r.header("Location").unwrap_or("").to_string();
        if !to.ends_with(&format!("/create/step/{}", i + 2)) {
            return r;
        }
        last = Some(r);
    }
    last.unwrap()
}

#[test]
fn create_instance() {
    let mut b = Browser::signed_on();
    // A step not reached yet goes back to the furthest one.
    let r = b.get("/create/step/4");
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/create/step/1"));
    let r = wizard(&mut b, &[("rot", "When size exceeds 10 MB")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/create/step/6"));
    assert!(!b.fx.files.exists("/www/shop"), "nothing is made before Create");
    let t = b.get("/create/step/6").text();
    assert!(t.contains("/www/shop/logs/access_log (Combined, when size exceeds 10 MB)"));
    let r = b.post("/create/step/6", &[("finish", "Create")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("HTP8020") && t.contains(">SHOP</a>"), "{t}");
    let conf = String::from_utf8(b.fx.files.read("/www/shop/conf/httpd.conf").unwrap()).unwrap();
    assert!(conf.contains("Listen *:8090\n") && conf.contains("ServerUserID QTMHHTTP\n"), "{conf}");
    assert!(conf.contains("LogMaint logs/access_log 30 10\n"), "{conf}");
    assert!(b.fx.files.exists("/www/shop/htdocs/index.html"));
    let reg = httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap();
    assert_eq!(reg.get("SHOP").unwrap().description, "Shop");
    // The answers are gone.
    assert_eq!(b.get("/create/step/3").header("Location"), Some("/qs400/httpadmin/create/step/1"));
}

#[test]
fn create_copy() {
    let mut b = Browser::signed_on();
    wizard(&mut b, &[("name", "intra2"), ("base", "copy"), ("copyfrom", "INTRANET"), ("port", "8181"), ("docroot", "/www/intra2/htdocs")]);
    let r = b.post("/create/step/6", &[("finish", "Create")]);
    assert!(shown(&mut b, &r).contains("HTP8020"));
    let conf = String::from_utf8(b.fx.files.read("/www/intra2/conf/httpd.conf").unwrap()).unwrap();
    assert!(conf.contains("ServerRoot /www/intra2\nListen *:8181\nDocumentRoot /www/intra2/htdocs\n"), "{conf}");
}

#[test]
fn create_checks() {
    let mut b = Browser::signed_on();
    for (name, id) in [("1BAD", "HTP8120"), ("WEBSTORE", "HTP8122")] {
        let r = wizard(&mut b, &[("name", name)]);
        assert_eq!(r.header("Location"), Some("/qs400/httpadmin/create/step/1"));
        assert!(b.get("/create/step/1").text().contains(id), "{name}");
    }
    // A low port needs *IOSYSCFG on the server user profile.
    Settings { default_user: "WEBUSER".into(), ..Settings::default() }.save(b.fx.files.as_ref()).unwrap();
    let r = wizard(&mut b, &[("port", "81")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/create/step/4"));
    assert!(b.get("/create/step/4").text().contains("HTP8202"));
    // The instance limit.
    Settings { max_instances: 8, ..Settings::default() }.save(b.fx.files.as_ref()).unwrap();
    wizard(&mut b, &[]);
    assert!(b.get("/create/step/1").text().contains("HTP8123"));
    // Cancel forgets the answers.
    let r = b.post("/create/step/1", &[("cancel", "Cancel")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    assert!(!b.get("/create/step/1").text().contains("value=\"SHOP\""));
}

#[test]
fn create_needs_iosyscfg() {
    let mut b = group_member();
    wizard(&mut b, &[]);
    let r = b.post("/create/step/6", &[("finish", "Create")]);
    assert_eq!(r.status, 403);
    assert!(refused(&b));
    assert!(!b.fx.files.exists("/www/shop"));
}
