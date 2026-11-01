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

fn conf(b: &Browser, name: &str) -> String {
    String::from_utf8(b.fx.files.read(&format!("/www/{}/conf/httpd.conf", name.to_ascii_lowercase())).unwrap()).unwrap()
}

/// The general page's fields as they are now, with `set` over them.
fn general_form<'a>(set: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut f = vec![("desc", "Web store"), ("docroot", "/www/webstore/htdocs"), ("usrprf", "QTMHHTTP"), ("threads", "40"), ("hot", "WEBSTORE2")];
    for (k, v) in set {
        f.retain(|(n, _)| n != k);
        f.push((k, v));
    }
    f
}

#[test]
fn general_apply() {
    let mut b = Browser::signed_on();
    let before = conf(&b, "WEBSTORE");
    // Start goes to the confirmation, changing nothing.
    let r = b.post("/instance/WEBSTORE/general", &general_form(&[("start", "Start")]));
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instance/WEBSTORE/confirm?op=start"));
    assert_eq!(conf(&b, "WEBSTORE"), before);
    for (set, id) in [
        (vec![("threads", "500")], "HTP8120"),
        (vec![("usrprf", "NOBODY")], "HTP8201"),
        (vec![("usrprf", "WEBUSER")], "HTP8202"),
        (vec![("docroot", "htdocs")], "HTP8120"),
    ] {
        let mut f = general_form(&set);
        f.push(("apply", "Apply"));
        let r = b.post("/instance/WEBSTORE/general", &f);
        assert!(shown(&mut b, &r).contains(id), "{id}");
        assert_eq!(conf(&b, "WEBSTORE"), before);
    }
    let mut f = general_form(&[("threads", "60"), ("docroot", "/www/webstore/site"), ("autostart", "on"), ("hotbackup", "on"), ("hot", "TESTSRV")]);
    f.push(("ok", "OK"));
    let r = b.post("/instance/WEBSTORE/general", &f);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let after = conf(&b, "WEBSTORE");
    assert!(after.contains("ThreadsPerChild 60\n") && after.contains("DocumentRoot /www/webstore/site\n"));
    assert!(after.contains("<Directory /www/webstore/site>\n   Order Allow,Deny\n"), "{after}");
    assert_eq!(String::from_utf8(b.fx.files.read("/www/webstore/conf/httpd.conf.bak").unwrap()).unwrap(), before);
    let reg = httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap();
    let e = reg.get("WEBSTORE").unwrap();
    assert!(e.autostart && e.hot_backup_active && !e.restart_required);
    assert_eq!((e.description.as_str(), e.hot_backup.as_deref()), ("Web store", Some("TESTSRV")));
    // A running instance needs a restart to use a change.
    let mut f = general_form(&[("docroot", "/www/intranet/htdocs"), ("threads", "50")]);
    f.push(("apply", "Apply"));
    b.post("/instance/INTRANET/general", &f);
    assert!(httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap().get("INTRANET").unwrap().restart_required);
}

#[test]
fn another_server_user_gets_authority() {
    let mut b = Browser::signed_on();
    // QTMHHTTP owns the files: nothing to give.
    let mut f = general_form(&[]);
    f.push(("apply", "Apply"));
    b.post("/instance/WEBSTORE/general", &f);
    assert!(b.fx.jobs.granted.lock().unwrap().is_empty());
    // Another profile reads the tree and writes its logs and the registry.
    let mut f = general_form(&[("usrprf", "WEBADM")]);
    f.push(("apply", "Apply"));
    let r = b.post("/instance/WEBSTORE/general", &f);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    assert!(conf(&b, "WEBSTORE").contains("ServerUserID WEBADM\n"));
    assert_eq!(*b.fx.jobs.granted.lock().unwrap(), httpadmin::env::authorize_commands("/www/webstore", "WEBADM"));
    let cmds = httpadmin::env::authorize_commands("/www/webstore", "WEBADM");
    assert!(cmds.iter().any(|c| c.contains("'/www/webstore/logs'") && c.contains("DTAAUT(*RWX)")));
    assert!(cmds.iter().any(|c| c.contains("'/www/instances.json'")));
}

#[test]
fn general_needs_iosyscfg() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    let mut f = general_form(&[("threads", "60")]);
    f.push(("apply", "Apply"));
    assert_eq!(b.post("/instance/WEBSTORE/general", &f).status, 403);
    assert!(refused(&b));
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

#[test]
fn ports_add_remove() {
    let mut b = Browser::signed_on();
    let r = b.post("/instance/WEBSTORE/ports", &[("addr", "*ALL"), ("port", "8443"), ("tls", "on"), ("add", "Add")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("Listen *:443\nListen *:8443\n"), "{c}");
    assert!(c.ends_with("<VirtualHost *:8443>\n   SSLEngine On\n   SSLAppName QS400_HTTP_SERVER_WEBSTORE\n</VirtualHost>\n"), "{c}");
    for (addr, port, id) in [("*ALL", "80", "HTP8105"), ("10.9.9.9", "8090", "HTP8120"), ("*ALL", "0", "HTP8120")] {
        let r = b.post("/instance/WEBSTORE/ports", &[("addr", addr), ("port", port), ("add", "Add")]);
        assert!(shown(&mut b, &r).contains(id), "{port}");
    }
    // Remove 443: its Listen and its virtual host.
    let r = b.post("/instance/WEBSTORE/ports", &[("l2", "on"), ("remove", "Remove")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let c = conf(&b, "WEBSTORE");
    assert!(!c.contains("Listen *:443\n") && !c.contains("<VirtualHost *:443>"), "{c}");
    assert!(c.contains("<VirtualHost *:8443>"));
    // Not every one.
    let r = b.post("/instance/WEBSTORE/ports", &[("l1", "on"), ("l2", "on"), ("remove", "Remove")]);
    assert!(shown(&mut b, &r).contains("HTP8120"));
    assert!(conf(&b, "WEBSTORE").contains("Listen *:80\n"));
}

#[test]
fn ports_low_port_needs_iosyscfg_on_server_profile() {
    let mut b = Browser::signed_on();
    let c = conf(&b, "TESTSRV").replace("ServerUserID QTMHHTTP", "ServerUserID WEBUSER");
    b.fx.files.put("/www/testsrv/conf/httpd.conf", c.as_bytes());
    let r = b.post("/instance/TESTSRV/ports", &[("addr", "*ALL"), ("port", "81"), ("add", "Add")]);
    assert!(shown(&mut b, &r).contains("HTP8202"));
    assert_eq!(conf(&b, "TESTSRV"), c);
    let r = b.post("/instance/TESTSRV/ports", &[("addr", "*ALL"), ("port", "8001"), ("add", "Add")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
}

#[test]
fn ports_need_iosyscfg_and_token() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    assert_eq!(b.post("/instance/WEBSTORE/ports", &[("port", "8090"), ("add", "Add")]).status, 403);
    assert!(refused(&b));
    assert_eq!(b.post_raw("/instance/WEBSTORE/ports", &[("port", "8090"), ("add", "Add")]).status, 403);
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

#[test]
fn directories_change_add_remove() {
    let mut b = Browser::signed_on();
    // Change the document root's directory.
    let r = b.post(
        "/instance/WEBSTORE/directory?path=/www/webstore/htdocs",
        &[("fs", "/www/webstore/htdocs"), ("url", "/"), ("idx", "on"), ("fsl", "on"), ("di", "index.html index.htm"), ("ao", "None"), ("order", "Allow,Deny"), ("allow", "all"), ("deny", "10.9.0.0/16"), ("apply", "Apply")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instance/WEBSTORE/directory?path=/www/webstore/htdocs"));
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("<Directory /www/webstore/htdocs>\n   Order Allow,Deny\n   Options +Indexes\n   Allow From all\n   Deny From 10.9.0.0/16\n</Directory>\n"), "{c}");
    assert!(b.fx.files.exists("/www/webstore/conf/httpd.conf.bak"));
    // Add a protected directory under the document root.
    let r = b.post("/instance/WEBSTORE/directories", &[("newtype", "Directory")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instance/WEBSTORE/directory?new=Directory"));
    let bad = [("fs", "/www/webstore/htdocs/private"), ("url", ""), ("auth", "on"), ("realm", "Private"), ("vldl", "NOSLASH"), ("fsl", "on"), ("ok", "OK")];
    let r = b.post("/instance/WEBSTORE/directory?new=Directory", &bad);
    assert!(shown(&mut b, &r).contains("HTP8120"));
    let mut good = bad.to_vec();
    good[4] = ("vldl", "webstore/users");
    let r = b.post("/instance/WEBSTORE/directory?new=Directory", &good);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instance/WEBSTORE/directories"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("<Directory /www/webstore/htdocs/private>\n   AuthType Basic\n   AuthName Private\n   PasswdFile WEBSTORE/USERS\n   Require valid-user\n</Directory>\n"), "{c}");
    let t = b.get("/instance/WEBSTORE/directories").text();
    assert!(t.contains("<td class=\"mono\">/private/</td>"), "{t}");
    // An alias with listings.
    let r = b.post("/instance/WEBSTORE/directory?new=Alias", &[("fs", "/www/docs/"), ("url", "/docs/"), ("idx", "on"), ("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("Alias /docs/ /www/docs/\n") && c.contains("<Location /docs/>\n   Options +Indexes\n</Location>\n"), "{c}");
    // Remove one: the GET only asks.
    assert_eq!(b.get("/instance/WEBSTORE/remove?path=/images/").status, 200);
    assert!(conf(&b, "WEBSTORE").contains("Alias /images/"));
    let r = b.post("/instance/WEBSTORE/remove?path=/images/", &[("ok", "OK")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    assert!(!conf(&b, "WEBSTORE").contains("Alias /images/"));
}

#[test]
fn directories_need_iosyscfg() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    let r = b.post("/instance/WEBSTORE/remove?path=/images/", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    let r = b.post("/instance/WEBSTORE/directory?new=Alias", &[("fs", "/www/docs/"), ("url", "/docs/"), ("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(refused(&b));
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

#[test]
fn tls_apply() {
    let mut b = Browser::signed_on();
    let before = conf(&b, "WEBSTORE");
    // Nothing checked is refused.
    let r = b.post("/instance/WEBSTORE/tls", &[("tls", "on"), ("cipher", "*STRONG"), ("ca", "none"), ("apply", "Apply")]);
    assert!(shown(&mut b, &r).contains("HTP8120"));
    assert_eq!(conf(&b, "WEBSTORE"), before);
    let r = b.post(
        "/instance/WEBSTORE/tls",
        &[("tls", "on"), ("t13", "on"), ("cipher", "*STRONG"), ("ca", "req"), ("apply", "Apply")],
    );
    assert!(shown(&mut b, &r).contains("HTP8022"));
    let c = conf(&b, "WEBSTORE");
    assert!(
        c.contains("<VirtualHost *:443>\n   SSLEngine On\n   SSLAppName QS400_HTTP_SERVER_WEBSTORE\n   SSLProtocolDisable TLSv1 TLSv1.1 TLSv1.2\n   SSLCipherSpec *STRONG\n   SSLClientAuth Required\n</VirtualHost>"),
        "{c}"
    );
    let t = b.get("/instance/WEBSTORE/tls").text();
    assert!(t.contains("<option selected>*STRONG</option>") && t.contains("value=\"req\" checked"));
    // Off.
    let r = b.post("/instance/WEBSTORE/tls", &[("t13", "on"), ("t12", "on"), ("cipher", "*DFT (system value QSSLCSL)"), ("ca", "none"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("   SSLEngine Off\n") && !c.contains("SSLCipherSpec") && !c.contains("SSLClientAuth"), "{c}");
    assert!(b.get("/instance/WEBSTORE/ports").text().contains("<td>No</td>"));
    // No TLS port to turn on.
    let r = b.post("/instance/TESTSRV/tls", &[("tls", "on"), ("t13", "on"), ("cipher", "*STRONG"), ("apply", "Apply")]);
    assert!(shown(&mut b, &r).contains("HTP8120"));
}

#[test]
fn tls_needs_iosyscfg() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    assert_eq!(b.post("/instance/WEBSTORE/tls", &[("t13", "on"), ("apply", "Apply")]).status, 403);
    assert!(refused(&b));
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

fn logging_form<'a>(set: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut f = vec![
        ("al", "on"), ("alp", "/www/webstore/logs/access_log"), ("alf", "Combined"), ("elp", "/www/webstore/logs/error_log"),
        ("ll", "warn"), ("rot", "daily"), ("at", "00:00"), ("size", "10"), ("keep", "30"), ("ccsid", "1208"),
    ];
    for (k, v) in set {
        f.retain(|(n, _)| n != k);
        f.push((k, v));
    }
    f
}

#[test]
fn logging_apply() {
    let mut b = Browser::signed_on();
    let before = conf(&b, "WEBSTORE");
    // The page as it is changes nothing in the file.
    let mut f = logging_form(&[]);
    f.push(("apply", "Apply"));
    let r = b.post("/instance/WEBSTORE/logging", &f);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    assert_eq!(conf(&b, "WEBSTORE"), before);
    for (set, id) in [(vec![("at", "25:00")], "HTP8120"), (vec![("keep", "0")], "HTP8120"), (vec![("elp", "logs/x")], "HTP8120")] {
        let mut f = logging_form(&set);
        f.push(("apply", "Apply"));
        let r = b.post("/instance/WEBSTORE/logging", &f);
        assert!(shown(&mut b, &r).contains(id));
        assert_eq!(conf(&b, "WEBSTORE"), before);
    }
    let mut f = logging_form(&[("alf", "Common"), ("ll", "info"), ("rot", "size"), ("size", "50"), ("keep", "7"), ("ccsid", "819")]);
    f.push(("ok", "OK"));
    let r = b.post("/instance/WEBSTORE/logging", &f);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    let c = conf(&b, "WEBSTORE");
    assert!(c.contains("CustomLog logs/access_log common\n") && c.contains("LogFormat \"%h %l %u %t \\\"%r\\\" %>s %b\" common\n"), "{c}");
    assert!(c.contains("LogLevel info\nLogMaint logs/access_log 7 50\nLogMaint logs/error_log 7 50\n"), "{c}");
    assert!(c.contains("LogCCSID 819\n"), "{c}");
    // Daily at a time; no access log.
    let mut f = logging_form(&[("at", "03:30")]);
    f.retain(|(k, _)| *k != "al");
    f.push(("apply", "Apply"));
    b.post("/instance/WEBSTORE/logging", &f);
    let c = conf(&b, "WEBSTORE");
    assert!(!c.contains("CustomLog") && c.contains("LogMaint logs/error_log 30 0 03:30\n"), "{c}");
    httpd::conf::parse(&c).unwrap();
}

#[test]
fn logging_needs_iosyscfg() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    let mut f = logging_form(&[("ll", "debug")]);
    f.push(("apply", "Apply"));
    assert_eq!(b.post("/instance/WEBSTORE/logging", &f).status, 403);
    assert!(refused(&b));
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

#[test]
fn config_validate_apply_reset() {
    let mut b = Browser::signed_on();
    let before = conf(&b, "INTRANET");
    let bad = before.replace("ThreadsPerChild 40", "ThreadsPerChild many").replace('\n', "\r\n");
    let r = b.post("/instance/INTRANET/config", &[("conf", &bad), ("validate", "Validate")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("HTP8100") && t.contains("Configuration error at line 8 column 1"), "{t}");
    assert!(t.contains("Error at line 8 column 1") && t.contains("ThreadsPerChild many"));
    assert_eq!(conf(&b, "INTRANET"), before, "validate never writes");
    // Apply refuses it too.
    let r = b.post("/instance/INTRANET/config", &[("conf", &bad), ("apply", "Apply")]);
    assert!(shown(&mut b, &r).contains("HTP8100"));
    assert_eq!(conf(&b, "INTRANET"), before);
    // Reset goes back to the file.
    let r = b.post("/instance/INTRANET/config", &[("reset", "Reset")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("ThreadsPerChild 40") && t.contains("Syntax OK"));
    let good = before.replace("ThreadsPerChild 40", "ThreadsPerChild 80");
    let r = b.post("/instance/INTRANET/config", &[("conf", &good), ("validate", "Validate")]);
    assert!(shown(&mut b, &r).contains("HTP8101"));
    assert_eq!(conf(&b, "INTRANET"), before);
    let r = b.post("/instance/INTRANET/config", &[("conf", &good.replace('\n', "\r\n")), ("apply", "Apply")]);
    assert!(shown(&mut b, &r).contains("HTP8022"));
    assert_eq!(conf(&b, "INTRANET"), good);
    assert_eq!(String::from_utf8(b.fx.files.read("/www/intranet/conf/httpd.conf.bak").unwrap()).unwrap(), before);
    let reg = httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap();
    assert!(reg.get("INTRANET").unwrap().restart_required);
}

#[test]
fn config_apply_needs_token_and_iosyscfg() {
    let mut b = group_member();
    let before = conf(&b, "WEBSTORE");
    let r = b.post_raw("/instance/WEBSTORE/config", &[("conf", "Listen 81\n"), ("apply", "Apply")]);
    assert_eq!(r.status, 403);
    let r = b.post("/instance/WEBSTORE/config", &[("conf", "Listen 8181\n"), ("apply", "Apply")]);
    assert_eq!(r.status, 403);
    assert!(refused(&b));
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

/// Files whose configuration writes fail, as if the job died before
/// the rename.
struct FailingWrites(std::sync::Arc<httpd::files::MemFiles>);

impl Files for FailingWrites {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.0.read(path)
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        if path.ends_with("/httpd.conf") {
            return Err(std::io::Error::other("ended before the rename"));
        }
        self.0.write_atomic(path, data)
    }
    fn append(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
        self.0.append(path, data)
    }
    fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        self.0.rename(from, to)
    }
    fn mkdir_all(&self, path: &str) -> std::io::Result<()> {
        self.0.mkdir_all(path)
    }
    fn remove(&self, path: &str) -> std::io::Result<()> {
        self.0.remove(path)
    }
    fn remove_tree(&self, path: &str) -> std::io::Result<()> {
        self.0.remove_tree(path)
    }
    fn list(&self, dir: &str) -> std::io::Result<Vec<httpd::files::Entry>> {
        self.0.list(dir)
    }
    fn stat(&self, path: &str) -> std::io::Result<httpd::files::Entry> {
        self.0.stat(path)
    }
}

#[test]
fn config_failed_write_keeps_the_file() {
    let fx = httpadmin::fixture::fixture();
    let env = httpadmin::Env {
        files: std::sync::Arc::new(FailingWrites(fx.files.clone())),
        ..fx.env.clone()
    };
    let mut b = Browser {
        admin: httpadmin::Admin::new(env),
        fx,
        cookie: None,
        csrf: String::new(),
    };
    assert_eq!(b.sign_on("QSECOFR", PASSWORD).status, 303);
    let before = conf(&b, "WEBSTORE");
    let r = b.post("/instance/WEBSTORE/config", &[("conf", &before.replace("LogLevel warn", "LogLevel info")), ("apply", "Apply")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("HTP8106") && t.contains("LogLevel info"), "{t}");
    assert_eq!(conf(&b, "WEBSTORE"), before);
}

#[test]
fn logs_tail_filter_download() {
    let mut b = Browser::signed_on();
    let t = b.get("/instance/WEBSTORE/logs?n=50&q=cgi-bin").text();
    assert!(t.contains("Showing last 2 of 2 lines") && t.contains("CART.PGM") && !t.contains("robots.txt"), "{t}");
    assert!(!t.contains("<option selected>50</option>") && t.contains("value=\"cgi-bin\""));
    let r = b.get("/instance/WEBSTORE/logs/download?which=access");
    assert_eq!(r.status, 200);
    assert!(r.header("Content-Disposition").unwrap().contains("WEBSTORE-access_log.txt"));
    let qsweb::http::Body::Stream(mut from) = r.body else { panic!("not streamed") };
    let mut got = Vec::new();
    std::io::Read::read_to_end(&mut from, &mut got).unwrap();
    assert_eq!(got, b.fx.files.read("/www/webstore/logs/access_log").unwrap());
    b.fx.files.put("/www/webstore/logs/error_log", b"[warn] one\n[error] two\n[warn] three\n");
    let t = b.get("/instance/WEBSTORE/logs?which=error&q=warn").text();
    assert!(t.contains("<b>Error log</b>") && t.contains("[warn] three") && !t.contains("[error] two"));
    assert!(t.contains("name=\"which\" value=\"error\""));
    let t = b.get("/instance/WEBSTORE/logs?which=error&n=100").text();
    assert!(t.contains("Showing last 3 of 3 lines of /www/webstore/logs/error_log (rotated daily)"));
}

#[test]
fn create_starts_blank() {
    let mut b = Browser::signed_on();
    let answers = [("name", "qstest"), ("desc", "Quartz Systems Test"), ("docroot", "/www/qstest/htdocs")];
    let blank = |b: &mut Browser| {
        let t = b.get("/create/step/1").text();
        !t.contains("QSTEST") && !t.contains("Quartz Systems Test")
    };
    // Between steps the answers stay.
    wizard(&mut b, &answers);
    let r = b.post("/create/step/6", &[("back", "< Back")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("/www/qstest/logs/access_log"), "{t}");
    for n in [5, 4, 3, 2] {
        b.post(&format!("/create/step/{n}"), &[("back", "< Back")]);
    }
    assert!(b.get("/create/step/1").text().contains("Quartz Systems Test"));
    // The navigation item starts afresh.
    wizard(&mut b, &answers);
    b.get("/create/step/6");
    b.get("/instances");
    assert!(blank(&mut b));
    // So do Cancel and a failed Create, whose message says why.
    wizard(&mut b, &answers);
    b.post("/create/step/6", &[("cancel", "Cancel")]);
    assert!(blank(&mut b));
    b.fx.files.write_atomic("/www/qstest", b"in the way").unwrap();
    wizard(&mut b, &answers);
    let r = b.post("/create/step/6", &[("finish", "Create")]);
    let t = shown(&mut b, &r);
    assert!(t.contains("HTP8106"), "{t}");
    assert!(t.contains("/msg/HTP8106?d=QSTEST&amp;d="), "{t}");
    assert!(blank(&mut b));
}
