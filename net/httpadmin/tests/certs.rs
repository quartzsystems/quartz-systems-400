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

fn step1(b: &mut Browser, label: &str, kt: &str, size: &str, curve: &str, sig: &str) -> String {
    let r = b.post(
        "/certs/create/step/1",
        &[("label", label), ("kt", kt), ("ks", size), ("curve", curve), ("sig", sig), ("days", "90"), ("next", "Next >")],
    );
    r.header("Location").unwrap_or("").to_string()
}

fn step2(b: &mut Browser, cn: &str) {
    let r = b.post("/certs/create/step/2", &[("cn", cn), ("san", cn), ("o", "Quartz Systems"), ("c", "us"), ("next", "Next >")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/create/step/3"));
}

#[test]
fn create_wizard_checks_each_step() {
    let mut b = Browser::signed_on();
    // Later steps need the earlier ones.
    assert_eq!(b.get("/certs/create/step/3").header("Location"), Some("/qs400/httpadmin/certs/create/step/1"));
    // From a store page's form: the wizard is for that store.
    let r = b.post("/certs/create/step/1?store=WEBSTORE", &[]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/create/step/1"));
    let t = b.get("/certs/create/step/1").text();
    let crumb = &t[t.find("class=\"crumb\"").unwrap()..];
    assert!(crumb[..crumb.find("</div>").unwrap()].contains("WEBSTORE"), "{t}");
    // SHA-384 with RSA, a label in use, no common name.
    assert!(step1(&mut b, "x", "rsa", "2048", "P-256", "SHA-384").ends_with("/step/1"));
    assert!(b.get("/certs/create/step/1").text().contains("HTP8322"));
    assert!(step1(&mut b, "webstore-dev", "ec", "2048", "P-256", "SHA-256").ends_with("/step/1"));
    assert!(b.get("/certs/create/step/1").text().contains("already has a certificate labeled webstore-dev"));
    assert!(step1(&mut b, "dev2", "ec", "2048", "P-256", "SHA-256").ends_with("/step/2"));
    let r = b.post("/certs/create/step/2", &[("cn", ""), ("next", "Next >")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/create/step/2"));
    assert!(b.get("/certs/create/step/2").text().contains("a common name is required"));
    // Cancel goes back to the store and forgets the wizard.
    let r = b.post("/certs/create/step/2", &[("cancel", "Cancel")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/WEBSTORE"));
    assert_eq!(b.get("/certs/create/step/2").header("Location"), Some("/qs400/httpadmin/certs/create/step/1"));
}

#[test]
fn self_signed_and_local_ca_certificates() {
    let mut b = Browser::signed_on();
    assert!(step1(&mut b, "p384", "ec", "2048", "P-384", "SHA-384").ends_with("/step/2"));
    step2(&mut b, "p384.quartz.example");
    let r = b.post("/certs/create/step/3", &[("s", "self"), ("ku1", "on"), ("dft", "on"), ("finish", "Create")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/create/result"));
    let t = b.get("/certs/create/result").text();
    assert!(t.contains("Certificate p384 created in store *SYSTEM and signed by its own key."), "{t}");
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    let r = rows.iter().find(|r| r.label == "p384").unwrap();
    assert_eq!((r.key.as_str(), r.self_signed, r.is_default), ("ECDSA P-384", true, true));
    assert_eq!(r.subject, "CN=p384.quartz.example,O=Quartz Systems,C=US");
    assert_eq!((r.not_after - r.not_before) / 86_400, 90);
    // Done goes to the store; the result is gone.
    let r = b.post("/certs/create/result", &[("finish", "Done")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/store/*SYSTEM"));
    assert_eq!(b.get("/certs/create/result").header("Location"), Some("/qs400/httpadmin/certs/create/step/1"));
    // The local CA signs a client certificate.
    assert!(step1(&mut b, "client1", "rsa", "3072", "P-256", "SHA-256").ends_with("/step/2"));
    step2(&mut b, "client1");
    b.post("/certs/create/step/3", &[("s", "local"), ("ca", "QSHQ400 Local CA"), ("ku2", "on"), ("finish", "Create")]);
    let t = b.get("/certs/create/result").text();
    assert!(t.contains("signed by QSHQ400 Local CA") && t.contains("CN=QSHQ400 Local CA, O=Quartz Systems, C=US"), "{t}");
    let r = b.post("/certs/create/result", &[("chain", "Download with chain")]);
    let pem = b.get(r.header("Location").unwrap()).text();
    assert_eq!(pem.matches("-----BEGIN CERTIFICATE-----").count(), 2);
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    let c = rows.iter().find(|r| r.label == "client1").unwrap();
    assert_eq!((c.kind, c.key.as_str()), (httpadmin::env::EntryType::Client, "RSA 3072"));
    // The audit journal has the changes.
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Change { what, .. } if what == "certificate client1 created")));
}

#[test]
fn a_request_for_an_external_ca() {
    let mut b = Browser::signed_on();
    step1(&mut b, "ext-req", "ec", "2048", "P-256", "SHA-256");
    step2(&mut b, "ext.quartz.example");
    b.post("/certs/create/step/3", &[("s", "csr"), ("ku1", "on"), ("finish", "Create")]);
    let t = b.get("/certs/create/result").text();
    assert!(t.contains("HTP8303") && t.contains("-----BEGIN CERTIFICATE REQUEST-----"), "{t}");
    assert!(!t.contains("Download with chain"));
    let r = b.post("/certs/create/result", &[("download", "Download")]);
    let csr = b.get(r.header("Location").unwrap());
    assert!(csr.text().starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
    assert!(b.get("/certs/store/*SYSTEM/requests").text().contains(">ext-req<"));
}

#[test]
fn renew_starts_from_the_certificate() {
    let mut b = Browser::signed_on();
    let r = b.get("/certs/create/step/1?store=*SYSTEM&renew=api.quartz.example");
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/create/step/1"));
    let t = b.get("/certs/create/step/1").text();
    assert!(t.contains("value=\"api.quartz.example-new\"") && t.contains("value=\"ec\" checked"), "{t}");
    b.post("/certs/create/step/1", &[("label", "api.quartz.example-new"), ("kt", "ec"), ("ks", "2048"), ("curve", "P-256"), ("sig", "SHA-256"), ("days", "365"), ("next", "Next >")]);
    let t = b.get("/certs/create/step/2").text();
    assert!(t.contains("name=\"cn\" value=\"api.quartz.example\"") && t.contains("name=\"o\" value=\"Quartz Systems\""), "{t}");
}
