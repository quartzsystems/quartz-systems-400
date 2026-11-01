// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate pages' actions: authority, CSRF, messages, what the
//! stores hold afterwards, and store passwords kept off every page,
//! message and audit entry.

mod common;

use certs::files::StoreFiles;
use common::Browser;
use httpadmin::env::{Audit, Certs};
use httpadmin::fixture::PASSWORD;

/// Everything the product kept or showed that a secret must not be in.
fn leaks(b: &Browser, pages: &[String], secret: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        if p.contains(secret) {
            out.push(format!("page {i}"));
        }
    }
    for a in b.fx.system.audit.lock().unwrap().iter() {
        if format!("{a:?}").contains(secret) {
            out.push(format!("audit {a:?}"));
        }
    }
    if let Some(s) = b.cookie.as_ref().and_then(|c| b.admin.sessions.get(c))
        && format!("{s:?}").contains(secret)
    {
        out.push("session".into());
    }
    out
}

#[test]
fn certificates_need_allobj_and_secadm() {
    let mut b = Browser::new();
    assert_eq!(b.sign_on("WEBADM", PASSWORD).status, 303);
    for route in ["/certs/stores", "/certs/store/*SYSTEM", "/certs/apps", "/certs/create/step/1", "/certs/import"] {
        let r = b.get(route);
        assert_eq!(r.status, 403, "{route}");
        assert!(r.text().contains("*ALLOBJ"), "{route}");
    }
    let r = b.post("/certs/stores", &[("sn", "X"), ("p1", "secret1"), ("p2", "secret1")]);
    assert_eq!(r.status, 403);
    assert!(b.fx.certs.stores().iter().all(|s| s.name != "X"));
    let audit = b.fx.system.audit.lock().unwrap();
    assert!(audit.iter().any(|a| matches!(a, Audit::Refused { user, need, .. } if user == "WEBADM" && need == "*ALLOBJ")));
}

#[test]
fn create_store_and_change_its_password() {
    let mut b = Browser::signed_on();
    let mut pages = Vec::new();
    // Not without the token.
    let r = b.post_raw("/certs/stores", &[("sn", "TESTST"), ("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-77")]);
    assert_eq!(r.status, 403);
    // Passwords that differ.
    let r = b.post("/certs/stores", &[("sn", "TESTST"), ("sp", "/QIBM/UserData/ICSS/Cert/"), ("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-78")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8322"), "{t}");
    pages.push(t);
    // Made.
    let r = b.post("/certs/stores", &[("sn", "testst"), ("sp", "/QIBM/UserData/ICSS/Cert/"), ("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-77"), ("create", "Create")]);
    assert_eq!(r.status, 303);
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8306") && t.contains("Certificate store TESTST created."), "{t}");
    assert!(t.contains("/QIBM/UserData/ICSS/Cert/TESTST.KDB"));
    pages.push(t);
    assert!(b.fx.certs.files.exists("/QIBM/UserData/ICSS/Cert/TESTST.KDB").unwrap());
    assert!(b.fx.certs.files.exists("/QIBM/UserData/ICSS/Cert/TESTST.RDB").unwrap());
    // The same name again.
    b.post("/certs/stores", &[("sn", "TESTST"), ("sp", "/QIBM/UserData/ICSS/Cert/"), ("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-77")]);
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8320"), "{t}");
    pages.push(t);

    // The password page; a wrong current password changes nothing.
    pages.push(b.get("/certs/store/TESTST/password").text());
    let r = b.post("/certs/store/TESTST/password", &[("old", "Wrong-Secret-1"), ("p1", "Yak-Secret-88"), ("p2", "Yak-Secret-88")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/TESTST/password"));
    let t = b.get("/certs/store/TESTST/password").text();
    assert!(t.contains("HTP8321") && t.contains("Password not correct for certificate store TESTST."), "{t}");
    pages.push(t);
    let r = b.post("/certs/store/TESTST/password", &[("old", "Zebra-Secret-77"), ("p1", "Yak-Secret-88"), ("p2", "Yak-Secret-88")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8307"), "{t}");
    pages.push(t);
    // The new password opens it; the old one does not.
    let ok = b.post("/certs/store/TESTST/password", &[("old", "Yak-Secret-88"), ("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-77")]);
    assert_eq!(ok.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    pages.push(b.get("/certs/stores").text());
    // The changes were audited, without the passwords.
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Change { what, .. } if what.contains("password changed"))));
    for secret in ["Zebra-Secret-77", "Yak-Secret-88", "Wrong-Secret-1"] {
        assert_eq!(leaks(&b, &pages, secret), Vec::<String>::new(), "{secret}");
    }
}

#[test]
fn tabs_list_by_kind() {
    let mut b = Browser::signed_on();
    let t = b.get("/certs/store/*SYSTEM/cas").text();
    assert!(t.contains("<b>Certificate authorities</b>"));
    assert!(t.contains(">Quartz Internal CA<") && t.contains(">QSHQ400 Local CA<"), "{t}");
    assert!(t.contains("<span class=\"ok\">Trusted</span>"));
    assert!(!t.contains(">api.quartz.example<"));
    let t = b.get("/certs/store/WEBSTORE/requests").text();
    assert!(t.contains(">webstore-2027-request<"), "{t}");
    assert!(b.get("/certs/store/*SYSTEM/requests").text().contains("No certificate requests are waiting in the store."));
    let t = b.get("/certs/store/*SYSTEM?sort=exp").text();
    let at = |s: &str| t.find(s).unwrap();
    assert!(at(">old-webstore<") < at(">navigator-2025<") && at(">navigator-2025<") < at(">api.quartz.example<"));
    assert_eq!(b.get("/certs/store/NOSUCH").status, 404);
}

#[test]
fn delete_a_certificate_through_the_confirmation() {
    let mut b = Browser::signed_on();
    let url = "/certs/confirm?op=delete&store=*SYSTEM&label=api.quartz.example";
    let t = b.get(url).text();
    assert!(t.contains("Confirm Delete Certificate") && t.contains("QIBM_HTTP_SERVER_API"), "{t}");
    // A GET deleted nothing; Cancel neither.
    assert!(b.fx.certs.entries("*SYSTEM").unwrap().iter().any(|r| r.label == "api.quartz.example"));
    let r = b.post(url, &[("cancel", "Cancel")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/*SYSTEM"));
    assert!(b.fx.certs.entries("*SYSTEM").unwrap().iter().any(|r| r.label == "api.quartz.example"));
    assert_eq!(b.post_raw(url, &[("ok", "OK")]).status, 403);
    let r = b.post(url, &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/*SYSTEM"));
    let t = b.get("/certs/store/*SYSTEM").text();
    assert!(t.contains("HTP8304") && !t.contains(">api.quartz.example<"), "{t}");
    assert_eq!(b.fx.certs.app_label("QIBM_HTTP_SERVER_API"), None);
}

#[test]
fn delete_a_store_and_set_a_default() {
    let mut b = Browser::signed_on();
    let r = b.post("/certs/store/*SYSTEM/default", &[]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/*SYSTEM/default"));
    let t = b.get("/certs/store/*SYSTEM/default").text();
    assert!(t.contains("<option selected>webstore.quartz.example</option>") && !t.contains(">old-webstore<"), "{t}");
    b.post("/certs/store/*SYSTEM/default", &[("label", "*.quartz.example"), ("ok", "OK")]);
    let t = b.get("/certs/store/*SYSTEM").text();
    assert!(t.contains("HTP8305"), "{t}");
    assert!(t.contains("<span class=\"ok\">Active</span> (default)"));
    let s = b.fx.certs.stores();
    assert_eq!(s[0].default.as_deref(), Some("*.quartz.example"));
    // The system's own stores cannot be deleted; WEBSTORE can.
    let t = b.get("/certs/confirm?op=dltstore&store=WEBSTORE").text();
    assert!(t.contains("Confirm Delete Certificate Store") && t.contains("/www/webstore/conf/webstore.kdb"));
    b.post("/certs/confirm?op=dltstore&store=*SYSTEM", &[("ok", "OK")]);
    assert!(b.get("/certs/stores").text().contains("HTP8320"));
    b.post("/certs/confirm?op=dltstore&store=WEBSTORE", &[("ok", "OK")]);
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8308") && !t.contains(">WEBSTORE<"), "{t}");
    assert!(!b.fx.certs.files.exists("/www/webstore/conf/webstore.kdb").unwrap());
}

#[test]
fn view_and_download() {
    let mut b = Browser::signed_on();
    let t = b.get("/certs/cert?store=*SYSTEM&label=api.quartz.example").text();
    assert!(t.contains("-----BEGIN CERTIFICATE-----") && t.contains("Fingerprint (SHA-256)"), "{t}");
    assert!(t.contains("CN=Quartz Internal Certificate Authority, O=Quartz Systems, C=US"));
    let r = b.post("/certs/cert?store=*SYSTEM&label=api.quartz.example", &[("chain", "Download with chain")]);
    let to = r.header("Location").unwrap().to_string();
    assert_eq!(to, "/qs400/httpadmin/certs/download?store=*SYSTEM&label=api.quartz.example&chain=1");
    let r = b.get(&to);
    assert_eq!(r.header("Content-Disposition"), Some("attachment; filename=\"api.quartz.example.pem\""));
    assert_eq!(r.text().matches("-----BEGIN CERTIFICATE-----").count(), 2, "the certificate and its root");
    let one = b.get("/certs/download?store=*SYSTEM&label=api.quartz.example").text();
    assert!(one.starts_with("-----BEGIN CERTIFICATE-----") && !one.contains("PRIVATE KEY"));
    let csr = b.get("/certs/download?store=WEBSTORE&label=webstore-2027-request");
    assert!(csr.text().starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
    assert_eq!(csr.header("Content-Disposition"), Some("attachment; filename=\"webstore-2027-request.csr\""));
}
