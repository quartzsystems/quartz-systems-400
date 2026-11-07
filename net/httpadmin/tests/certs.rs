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
    for route in [
        "/certs/stores",
        "/certs/store/*SYSTEM",
        "/certs/apps",
        "/certs/create/step/1",
        "/certs/import",
    ] {
        let r = b.get(route);
        assert_eq!(r.status, 403, "{route}");
        assert!(r.text().contains("*ALLOBJ"), "{route}");
    }
    let r = b.post(
        "/certs/stores",
        &[("sn", "X"), ("p1", "secret1"), ("p2", "secret1")],
    );
    assert_eq!(r.status, 403);
    assert!(b.fx.certs.stores().iter().all(|s| s.name != "X"));
    let audit = b.fx.system.audit.lock().unwrap();
    assert!(audit.iter().any(
        |a| matches!(a, Audit::Refused { user, need, .. } if user == "WEBADM" && need == "*ALLOBJ")
    ));
}

#[test]
fn create_store_and_change_its_password() {
    let mut b = Browser::signed_on();
    let mut pages = Vec::new();
    // Not without the token.
    let r = b.post_raw(
        "/certs/stores",
        &[
            ("sn", "TESTST"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    assert_eq!(r.status, 403);
    // Passwords that differ.
    let r = b.post(
        "/certs/stores",
        &[
            ("sn", "TESTST"),
            ("sp", "/QIBM/UserData/ICSS/Cert/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-78"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8322"), "{t}");
    pages.push(t);
    // Made.
    let r = b.post(
        "/certs/stores",
        &[
            ("sn", "testst"),
            ("sp", "/QIBM/UserData/ICSS/Cert/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
            ("create", "Create"),
        ],
    );
    assert_eq!(r.status, 303);
    let t = b.get("/certs/stores").text();
    assert!(
        t.contains("HTP8306") && t.contains("Certificate store TESTST created."),
        "{t}"
    );
    assert!(t.contains("/QIBM/UserData/ICSS/Cert/TESTST.KDB"));
    pages.push(t);
    assert!(
        b.fx.certs
            .files
            .exists("/QIBM/UserData/ICSS/Cert/TESTST.KDB")
            .unwrap()
    );
    assert!(
        b.fx.certs
            .files
            .exists("/QIBM/UserData/ICSS/Cert/TESTST.RDB")
            .unwrap()
    );
    // The same name again.
    b.post(
        "/certs/stores",
        &[
            ("sn", "TESTST"),
            ("sp", "/QIBM/UserData/ICSS/Cert/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8320"), "{t}");
    pages.push(t);

    // The store has a stash: the password page asks only for the new one.
    let t = b.get("/certs/store/TESTST/password").text();
    assert!(
        !t.contains("Current password") && t.contains("New password"),
        "{t}"
    );
    pages.push(t);
    let r = b.post(
        "/certs/store/TESTST/password",
        &[("p1", "Yak-Secret-88"), ("p2", "Yak-Secret-88")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8307"), "{t}");
    pages.push(t);
    // The new password opens it; the old one does not.
    let ctx = b.fx.certs.manager().ctx;
    let path = "/QIBM/UserData/ICSS/Cert/TESTST.KDB";
    assert!(
        ctx.open(&certs::admin::Keystore::new(path, "Yak-Secret-88"))
            .is_ok()
    );
    assert!(
        ctx.open(&certs::admin::Keystore::new(path, "Zebra-Secret-77"))
            .is_err()
    );
    // Without its stash the current password is asked for, and a wrong
    // one changes nothing.
    b.fx.certs
        .files
        .remove("/QIBM/UserData/ICSS/Cert/TESTST.RDB")
        .unwrap();
    let t = b.get("/certs/store/TESTST/password").text();
    assert!(t.contains("Current password"), "{t}");
    pages.push(t);
    let r = b.post(
        "/certs/store/TESTST/password",
        &[
            ("old", "Wrong-Secret-1"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/TESTST/password")
    );
    let t = b.get("/certs/store/TESTST/password").text();
    assert!(
        t.contains("HTP8321") && t.contains("Password not correct for certificate store TESTST."),
        "{t}"
    );
    pages.push(t);
    let r = b.post(
        "/certs/store/TESTST/password",
        &[("p1", "Zebra-Secret-77"), ("p2", "Zebra-Secret-77")],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/TESTST/password")
    );
    let ok = b.post(
        "/certs/store/TESTST/password",
        &[
            ("old", "Yak-Secret-88"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    assert_eq!(ok.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    pages.push(b.get("/certs/stores").text());
    // The changes were audited, without the passwords.
    assert!(
        b.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Change { what, .. } if what.contains("password changed")))
    );
    for secret in ["Zebra-Secret-77", "Yak-Secret-88", "Wrong-Secret-1"] {
        assert_eq!(leaks(&b, &pages, secret), Vec::<String>::new(), "{secret}");
    }
}

#[test]
fn tabs_list_by_kind() {
    let mut b = Browser::signed_on();
    let t = b.get("/certs/store/*SYSTEM/cas").text();
    assert!(t.contains("<b>Certificate authorities</b>"));
    assert!(
        t.contains(">Quartz Internal CA<") && t.contains(">QSHQ400 Local CA<"),
        "{t}"
    );
    assert!(t.contains("<span class=\"ok\">Trusted</span>"));
    assert!(!t.contains(">api.quartz.example<"));
    let t = b.get("/certs/store/WEBSTORE/requests").text();
    assert!(t.contains(">webstore-2027-request<"), "{t}");
    assert!(
        b.get("/certs/store/*SYSTEM/requests")
            .text()
            .contains("No certificate requests are waiting in the store.")
    );
    let t = b.get("/certs/store/*SYSTEM?sort=exp").text();
    let at = |s: &str| t.find(s).unwrap();
    assert!(
        at(">old-webstore<") < at(">navigator-2025<")
            && at(">navigator-2025<") < at(">api.quartz.example<")
    );
    assert_eq!(b.get("/certs/store/NOSUCH").status, 404);
}

#[test]
fn delete_a_certificate_through_the_confirmation() {
    let mut b = Browser::signed_on();
    let url = "/certs/confirm?op=delete&store=*SYSTEM&label=api.quartz.example";
    let t = b.get(url).text();
    assert!(
        t.contains("Confirm Delete Certificate") && t.contains("QS400_HTTP_SERVER_API"),
        "{t}"
    );
    // A GET deleted nothing; Cancel neither.
    assert!(
        b.fx.certs
            .entries("*SYSTEM")
            .unwrap()
            .iter()
            .any(|r| r.label == "api.quartz.example")
    );
    let r = b.post(url, &[("cancel", "Cancel")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM")
    );
    assert!(
        b.fx.certs
            .entries("*SYSTEM")
            .unwrap()
            .iter()
            .any(|r| r.label == "api.quartz.example")
    );
    assert_eq!(b.post_raw(url, &[("ok", "OK")]).status, 403);
    let r = b.post(url, &[("ok", "OK")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM")
    );
    let t = b.get("/certs/store/*SYSTEM").text();
    assert!(
        t.contains("HTP8304") && !t.contains(">api.quartz.example<"),
        "{t}"
    );
    assert_eq!(b.fx.certs.app_label("QS400_HTTP_SERVER_API"), None);
}

#[test]
fn delete_a_store_and_set_a_default() {
    let mut b = Browser::signed_on();
    let r = b.post("/certs/store/*SYSTEM/default", &[]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM/default")
    );
    let t = b.get("/certs/store/*SYSTEM/default").text();
    assert!(
        t.contains("<option selected>webstore.quartz.example</option>")
            && !t.contains(">old-webstore<"),
        "{t}"
    );
    b.post(
        "/certs/store/*SYSTEM/default",
        &[("label", "*.quartz.example"), ("ok", "OK")],
    );
    let t = b.get("/certs/store/*SYSTEM").text();
    assert!(t.contains("HTP8305"), "{t}");
    assert!(t.contains("<span class=\"ok\">Active</span> (default)"));
    let s = b.fx.certs.stores();
    assert_eq!(s[0].default.as_deref(), Some("*.quartz.example"));
    // The system's own stores cannot be deleted; WEBSTORE can.
    let t = b.get("/certs/confirm?op=dltstore&store=WEBSTORE").text();
    assert!(
        t.contains("Confirm Delete Certificate Store")
            && t.contains("/www/webstore/conf/webstore.kdb")
    );
    b.post("/certs/confirm?op=dltstore&store=*SYSTEM", &[("ok", "OK")]);
    assert!(b.get("/certs/stores").text().contains("HTP8320"));
    b.post("/certs/confirm?op=dltstore&store=WEBSTORE", &[("ok", "OK")]);
    let t = b.get("/certs/stores").text();
    assert!(t.contains("HTP8308") && !t.contains(">WEBSTORE<"), "{t}");
    assert!(
        !b.fx
            .certs
            .files
            .exists("/www/webstore/conf/webstore.kdb")
            .unwrap()
    );
}

#[test]
fn view_and_download() {
    let mut b = Browser::signed_on();
    let t = b
        .get("/certs/cert?store=*SYSTEM&label=api.quartz.example")
        .text();
    assert!(
        t.contains("-----BEGIN CERTIFICATE-----") && t.contains("Fingerprint (SHA-256)"),
        "{t}"
    );
    assert!(t.contains("CN=Quartz Internal Certificate Authority, O=Quartz Systems, C=US"));
    let r = b.post(
        "/certs/cert?store=*SYSTEM&label=api.quartz.example",
        &[("chain", "Download with chain")],
    );
    let to = r.header("Location").unwrap().to_string();
    assert_eq!(
        to,
        "/qs400/httpadmin/certs/download?store=*SYSTEM&label=api.quartz.example&chain=1"
    );
    let r = b.get(&to);
    assert_eq!(
        r.header("Content-Disposition"),
        Some("attachment; filename=\"api.quartz.example.pem\"")
    );
    assert_eq!(
        r.text().matches("-----BEGIN CERTIFICATE-----").count(),
        2,
        "the certificate and its root"
    );
    let one = b
        .get("/certs/download?store=*SYSTEM&label=api.quartz.example")
        .text();
    assert!(one.starts_with("-----BEGIN CERTIFICATE-----") && !one.contains("PRIVATE KEY"));
    let csr = b.get("/certs/download?store=WEBSTORE&label=webstore-2027-request");
    assert!(
        csr.text()
            .starts_with("-----BEGIN CERTIFICATE REQUEST-----")
    );
    assert_eq!(
        csr.header("Content-Disposition"),
        Some("attachment; filename=\"webstore-2027-request.csr\"")
    );
}

fn step1(b: &mut Browser, label: &str, kt: &str, size: &str, curve: &str, sig: &str) -> String {
    let r = b.post(
        "/certs/create/step/1",
        &[
            ("label", label),
            ("kt", kt),
            ("ks", size),
            ("curve", curve),
            ("sig", sig),
            ("days", "90"),
            ("next", "Next >"),
        ],
    );
    r.header("Location").unwrap_or("").to_string()
}

fn step2(b: &mut Browser, cn: &str) {
    let r = b.post(
        "/certs/create/step/2",
        &[
            ("cn", cn),
            ("san", cn),
            ("o", "Quartz Systems"),
            ("c", "us"),
            ("next", "Next >"),
        ],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/create/step/3")
    );
}

#[test]
fn create_wizard_checks_each_step() {
    let mut b = Browser::signed_on();
    // Later steps need the earlier ones.
    assert_eq!(
        b.get("/certs/create/step/3").header("Location"),
        Some("/qs400/httpadmin/certs/create/step/1")
    );
    // From a store page's form: the wizard is for that store.
    let r = b.post("/certs/create/step/1?store=WEBSTORE", &[]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/create/step/1")
    );
    let t = b.get("/certs/create/step/1").text();
    let crumb = &t[t.find("class=\"crumb\"").unwrap()..];
    assert!(
        crumb[..crumb.find("</div>").unwrap()].contains("WEBSTORE"),
        "{t}"
    );
    // SHA-384 with RSA, a label in use, no common name.
    assert!(step1(&mut b, "x", "rsa", "2048", "P-256", "SHA-384").ends_with("/step/1"));
    assert!(b.get("/certs/create/step/1").text().contains("HTP8322"));
    assert!(step1(&mut b, "webstore-dev", "ec", "2048", "P-256", "SHA-256").ends_with("/step/1"));
    assert!(
        b.get("/certs/create/step/1")
            .text()
            .contains("already has a certificate labeled webstore-dev")
    );
    assert!(step1(&mut b, "dev2", "ec", "2048", "P-256", "SHA-256").ends_with("/step/2"));
    let r = b.post("/certs/create/step/2", &[("cn", ""), ("next", "Next >")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/create/step/2")
    );
    assert!(
        b.get("/certs/create/step/2")
            .text()
            .contains("a common name is required")
    );
    // Cancel goes back to the store and forgets the wizard.
    let r = b.post("/certs/create/step/2", &[("cancel", "Cancel")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/WEBSTORE")
    );
    assert_eq!(
        b.get("/certs/create/step/2").header("Location"),
        Some("/qs400/httpadmin/certs/create/step/1")
    );
}

#[test]
fn self_signed_and_local_ca_certificates() {
    let mut b = Browser::signed_on();
    assert!(step1(&mut b, "p384", "ec", "2048", "P-384", "SHA-384").ends_with("/step/2"));
    step2(&mut b, "p384.quartz.example");
    let r = b.post(
        "/certs/create/step/3",
        &[
            ("s", "self"),
            ("ku1", "on"),
            ("dft", "on"),
            ("finish", "Create"),
        ],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/create/result")
    );
    let t = b.get("/certs/create/result").text();
    assert!(
        t.contains("Certificate p384 created in store *SYSTEM and signed by its own key."),
        "{t}"
    );
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    let r = rows.iter().find(|r| r.label == "p384").unwrap();
    assert_eq!(
        (r.key.as_str(), r.self_signed, r.is_default),
        ("ECDSA P-384", true, true)
    );
    assert_eq!(r.subject, "CN=p384.quartz.example,O=Quartz Systems,C=US");
    assert_eq!((r.not_after - r.not_before) / 86_400, 90);
    // Done goes to the store; the result is gone.
    let r = b.post("/certs/create/result", &[("finish", "Done")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM")
    );
    assert_eq!(
        b.get("/certs/create/result").header("Location"),
        Some("/qs400/httpadmin/certs/create/step/1")
    );
    // The local CA signs a client certificate.
    assert!(step1(&mut b, "client1", "rsa", "3072", "P-256", "SHA-256").ends_with("/step/2"));
    step2(&mut b, "client1");
    b.post(
        "/certs/create/step/3",
        &[
            ("s", "local"),
            ("ca", "QSHQ400 Local CA"),
            ("ku2", "on"),
            ("finish", "Create"),
        ],
    );
    let t = b.get("/certs/create/result").text();
    assert!(
        t.contains("signed by QSHQ400 Local CA")
            && t.contains("CN=QSHQ400 Local CA, O=Quartz Systems, C=US"),
        "{t}"
    );
    let r = b.post("/certs/create/result", &[("chain", "Download with chain")]);
    let pem = b.get(r.header("Location").unwrap()).text();
    assert_eq!(pem.matches("-----BEGIN CERTIFICATE-----").count(), 2);
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    let c = rows.iter().find(|r| r.label == "client1").unwrap();
    assert_eq!(
        (c.kind, c.key.as_str()),
        (httpadmin::env::EntryType::Client, "RSA 3072")
    );
    // The audit journal has the changes.
    assert!(
        b.fx.system.audit.lock().unwrap().iter().any(
            |a| matches!(a, Audit::Change { what, .. } if what == "certificate client1 created")
        )
    );
}

#[test]
fn a_request_for_an_external_ca() {
    let mut b = Browser::signed_on();
    step1(&mut b, "ext-req", "ec", "2048", "P-256", "SHA-256");
    step2(&mut b, "ext.quartz.example");
    b.post(
        "/certs/create/step/3",
        &[("s", "csr"), ("ku1", "on"), ("finish", "Create")],
    );
    let t = b.get("/certs/create/result").text();
    assert!(
        t.contains("HTP8303") && t.contains("-----BEGIN CERTIFICATE REQUEST-----"),
        "{t}"
    );
    assert!(!t.contains("Download with chain"));
    let r = b.post("/certs/create/result", &[("download", "Download")]);
    let csr = b.get(r.header("Location").unwrap());
    assert!(
        csr.text()
            .starts_with("-----BEGIN CERTIFICATE REQUEST-----")
    );
    assert!(
        b.get("/certs/store/*SYSTEM/requests")
            .text()
            .contains(">ext-req<")
    );
}

#[test]
fn renew_starts_from_the_certificate() {
    let mut b = Browser::signed_on();
    let r = b.get("/certs/create/step/1?store=*SYSTEM&renew=api.quartz.example");
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/create/step/1")
    );
    let t = b.get("/certs/create/step/1").text();
    assert!(
        t.contains("value=\"api.quartz.example-new\"") && t.contains("value=\"ec\" checked"),
        "{t}"
    );
    b.post(
        "/certs/create/step/1",
        &[
            ("label", "api.quartz.example-new"),
            ("kt", "ec"),
            ("ks", "2048"),
            ("curve", "P-256"),
            ("sig", "SHA-256"),
            ("days", "365"),
            ("next", "Next >"),
        ],
    );
    let t = b.get("/certs/create/step/2").text();
    assert!(
        t.contains("name=\"cn\" value=\"api.quartz.example\"")
            && t.contains("name=\"o\" value=\"Quartz Systems\""),
        "{t}"
    );
}

/// A certificate made elsewhere: PKCS#12 with `pw`, PEM with its key,
/// and the DER of a CA certificate.
fn outside_cert(cn: &str, pw: &str) -> (Vec<u8>, String, Vec<u8>) {
    use certs::crypto::kdf::Argon2Params;
    use certs::crypto::keys::KeyAlgorithm;
    use certs::store::{CertStore, StoreKind};
    use certs::x509::build::{CertKind, CertSpec, Subject};
    let files = certs::files::MemFiles::new();
    let fast = Argon2Params {
        m_cost: 64,
        t_cost: 1,
        p_cost: 1,
    };
    let now = httpadmin::fixture::now();
    let mut s = CertStore::create_with(
        &files,
        "/tmp/o.kdb",
        "outside1",
        StoreKind::System,
        now,
        fast,
    )
    .unwrap();
    s.create_self_signed(
        "x",
        &CertSpec::server(cn, now, 365),
        KeyAlgorithm::EcdsaP256,
    )
    .unwrap();
    let ca = CertSpec {
        subject: Subject::cn("Outside CA"),
        kind: CertKind::Ca,
        san: Vec::new(),
        not_before: now,
        days: 365,
    };
    s.create_self_signed("ca", &ca, KeyAlgorithm::EcdsaP256)
        .unwrap();
    let der = s.get("ca").unwrap().cert.clone().unwrap();
    (
        s.export_pkcs12("x", pw).unwrap(),
        s.export_pem("x", true).unwrap().to_string(),
        der,
    )
}

/// Post the import form as a browser does: multipart, a file part.
fn upload(
    b: &mut Browser,
    fields: &[(&str, &str)],
    file: Option<(&str, &[u8])>,
) -> qsweb::http::Response {
    let boundary = "----qs400test";
    let mut body = Vec::new();
    let csrf = b.csrf.clone();
    for (k, v) in [("csrf", csrf.as_str())].iter().chain(fields) {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    let (name, data) = file.unwrap_or(("", b""));
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut req = qsweb::http::Request::new("POST", &httpadmin::PRODUCT.url("/certs/import"));
    req.headers.push((
        "content-type".into(),
        format!("multipart/form-data; boundary={boundary}"),
    ));
    req.body = body;
    b.send(req)
}

#[test]
fn import_pkcs12_pem_der_and_paths() {
    let mut b = Browser::signed_on();
    let mut pages = Vec::new();
    // Import certificate... on a store page.
    let r = b.post("/certs/import?store=WEBSTORE", &[]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/import?store=WEBSTORE")
    );
    assert!(
        b.get("/certs/import?store=WEBSTORE")
            .text()
            .contains("<option selected>WEBSTORE</option>")
    );
    // PKCS#12, label from the common name; a wrong password first.
    let (p12, pem, der) = outside_cert("shop.quartz.example", "P12-Secret-55");
    let r = upload(
        &mut b,
        &[
            ("store", "*SYSTEM"),
            ("fmt", "auto"),
            ("pw", "Wrong-P12-1"),
            ("t", "srv"),
            ("label", "shop"),
        ],
        Some(("shop.p12", &p12)),
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/import?store=*SYSTEM")
    );
    let t = b.get("/certs/import?store=*SYSTEM").text();
    assert!(
        t.contains("HTP8320") && t.contains("the password does not open the file"),
        "{t}"
    );
    pages.push(t);
    let r = upload(
        &mut b,
        &[
            ("store", "*SYSTEM"),
            ("fmt", "p12"),
            ("pw", "P12-Secret-55"),
            ("t", "srv"),
            ("dft", "on"),
            ("import", "Import"),
        ],
        Some(("shop.p12", &p12)),
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM")
    );
    let t = b.get("/certs/store/*SYSTEM").text();
    assert!(
        t.contains("HTP8302")
            && t.contains("Certificate shop.quartz.example imported into store *SYSTEM."),
        "{t}"
    );
    pages.push(t);
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    let shop = rows
        .iter()
        .find(|r| r.label == "shop.quartz.example")
        .unwrap();
    assert!(shop.is_default && shop.kind == httpadmin::env::EntryType::Server);
    assert_eq!(leaks(&b, &pages, "P12-Secret-55"), Vec::<String>::new());
    assert_eq!(leaks(&b, &pages, "Wrong-P12-1"), Vec::<String>::new());
    // PEM with its key, into another store with a label.
    upload(
        &mut b,
        &[
            ("store", "WEBSTORE"),
            ("fmt", "pem"),
            ("t", "srv"),
            ("label", "shop-pem"),
        ],
        Some(("shop.pem", pem.as_bytes())),
    );
    assert!(
        b.fx.certs
            .entries("WEBSTORE")
            .unwrap()
            .iter()
            .any(|r| r.label == "shop-pem")
    );
    // A DER certificate from a stream file, as a CA.
    b.fx.files.put("/home/QSECOFR/root.cer", &der);
    let r = upload(
        &mut b,
        &[
            ("store", "*SIGNATUREVERIFICATION"),
            ("path", "/home/QSECOFR/root.cer"),
            ("t", "ca"),
        ],
        None,
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SIGNATUREVERIFICATION/cas")
    );
    let s = b.fx.certs.stores();
    assert_eq!(
        s.iter()
            .find(|s| s.name == "*SIGNATUREVERIFICATION")
            .unwrap()
            .certificates,
        4
    );
    // Nothing to import; a path that is not there.
    upload(&mut b, &[("store", "*SYSTEM")], None);
    assert!(b.get("/certs/import").text().contains("HTP8322"));
    upload(
        &mut b,
        &[("store", "*SYSTEM"), ("path", "/home/nothing.p12")],
        None,
    );
    let t = b.get("/certs/import").text();
    assert!(
        t.contains("HTP8320") && t.contains("value=\"/home/nothing.p12\""),
        "{t}"
    );
    // Not without the token.
    b.csrf = "0000".into();
    let r = upload(
        &mut b,
        &[("store", "*SYSTEM")],
        Some(("shop.pem", pem.as_bytes())),
    );
    assert_eq!(r.status, 403);
}

#[test]
fn assign_and_remove_application_certificates() {
    let mut b = Browser::signed_on();
    let t = b
        .get("/certs/apps?app=QS400_TELNET_SERVER&cert=webstore.quartz.example")
        .text();
    assert!(
        t.contains("<option selected>QS400_TELNET_SERVER</option>"),
        "{t}"
    );
    assert!(t.contains("<option selected>webstore.quartz.example (expires 03/14/27)</option>"));
    // Expired and expiring certificates are not offered.
    assert!(!t.contains("api.quartz.example (expires") && !t.contains("old-webstore (expires"));
    assert_eq!(
        b.post_raw(
            "/certs/apps",
            &[("app", "QS400_FTP_SERVER"), ("cert", "*.quartz.example")]
        )
        .status,
        403
    );
    let r = b.post(
        "/certs/apps",
        &[
            ("app", "QS400_FTP_SERVER"),
            ("cert", "*.quartz.example (expires 01/02/28)"),
            ("assign", "Assign"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/apps"));
    let t = b.get("/certs/apps").text();
    assert!(
        t.contains("HTP8310")
            && t.contains("Certificate *.quartz.example assigned to application QS400_FTP_SERVER."),
        "{t}"
    );
    assert_eq!(
        b.fx.certs.app_label("QS400_FTP_SERVER").as_deref(),
        Some("*.quartz.example")
    );
    // A CA is not something a server presents.
    b.post(
        "/certs/apps",
        &[("app", "QS400_FTP_SERVER"), ("cert", "Quartz Internal CA")],
    );
    assert!(b.get("/certs/apps").text().contains("HTP8320"));
    // Remove, through its confirmation.
    let url = "/certs/confirm?op=unassign&app=QS400_HTTP_SERVER_API";
    assert!(b.get(url).text().contains("Confirm Remove Assignment"));
    assert_eq!(
        b.fx.certs.app_label("QS400_HTTP_SERVER_API").as_deref(),
        Some("api.quartz.example")
    );
    let r = b.post(url, &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/apps"));
    assert!(b.get("/certs/apps").text().contains("HTP8311"));
    assert_eq!(b.fx.certs.app_label("QS400_HTTP_SERVER_API"), None);
    assert!(
        b.fx.system.audit.lock().unwrap().iter().any(
            |a| matches!(a, Audit::Change { object, .. } if object == "QS400_HTTP_SERVER_API")
        )
    );
}

/// A server certificate made in store `store` (as the store page's
/// Create does), self-signed.
fn cert_in(b: &Browser, store: &str, label: &str, cn: &str) {
    use certs::admin::Keystore;
    use certs::admin::entries::{self, Signer};
    use certs::crypto::keys::KeyAlgorithm;
    use certs::x509::build::CertSpec;
    let m = b.fx.certs.manager();
    let path =
        b.fx.certs
            .stores()
            .into_iter()
            .find(|s| s.name == store)
            .unwrap()
            .path;
    let spec = CertSpec::server(cn, httpadmin::fixture::now(), 365);
    entries::create(
        &m.ctx,
        &Keystore::new(&path, ""),
        label,
        &spec,
        KeyAlgorithm::EcdsaP256,
        &Signer::SelfSigned,
    )
    .unwrap();
}

/// The Certificate list is never silently empty: a missing `*SYSTEM`
/// says so and how to make it, and the stores page's Create makes it.
#[test]
fn applications_say_why_no_certificate_is_offered() {
    let mut b = Browser::signed_on();
    b.fx.certs.files.remove(certs::paths::SYSTEM_STORE).unwrap();
    let t = b.get("/certs/apps").text();
    assert!(
        t.contains("HTP8313") && t.contains("Certificate store *SYSTEM cannot be used"),
        "{t}"
    );
    // *SYSTEM made from the stores page, at its own path.
    let r = b.post(
        "/certs/stores",
        &[
            ("sn", "*system"),
            ("sp", "/elsewhere/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
            ("create", "Create"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/stores"));
    assert!(b.get("/certs/stores").text().contains("HTP8306"));
    let sys =
        b.fx.certs
            .stores()
            .into_iter()
            .find(|s| s.name == "*SYSTEM")
            .unwrap();
    assert_eq!(sys.path, certs::paths::SYSTEM_STORE);
    // It trusts the shipped roots but has nothing to present.
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    assert!(!rows.is_empty() && rows.iter().all(|r| r.kind == httpadmin::env::EntryType::Ca));
    let t = b.get("/certs/apps").text();
    assert!(
        t.contains("HTP8312") && t.contains("No certificate of store *SYSTEM can be assigned."),
        "{t}"
    );
    // Made again: refused.
    b.post(
        "/certs/stores",
        &[
            ("sn", "*SYSTEM"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    assert!(b.get("/certs/stores").text().contains("exists already"));
}

/// A certificate made in a store of one's own reaches an application:
/// Assign to application... offers Copy to *SYSTEM, which copies it with
/// its key and comes back to the Applications page with it chosen.
#[test]
fn a_certificate_of_another_store_is_copied_then_assigned() {
    let mut b = Browser::signed_on();
    b.post(
        "/certs/stores",
        &[
            ("sn", "QSTORE01"),
            ("sp", "/QIBM/UserData/ICSS/Cert/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    cert_in(&b, "QSTORE01", "ADMIN-CERT01", "qs400.ad.quartz.example");
    let view = "/certs/cert?store=QSTORE01&label=ADMIN-CERT01";
    assert!(b.get(view).text().contains("value=\"Copy to *SYSTEM\""));
    // The certificate's Assign to application...
    let r = b.post(view, &[("assign", "Assign to application...")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/apps?cert=ADMIN-CERT01&store=QSTORE01")
    );
    let t = b.get("/certs/apps?cert=ADMIN-CERT01&store=QSTORE01").text();
    assert!(
        t.contains("HTP8314")
            && t.contains("Certificate ADMIN-CERT01 is in store QSTORE01, not in *SYSTEM."),
        "{t}"
    );
    assert!(
        t.contains("value=\"Copy to *SYSTEM\"") && !t.contains("ADMIN-CERT01 (expires"),
        "{t}"
    );
    // Copy, not without the token.
    assert_eq!(b.post_raw(view, &[("copy", "Copy to *SYSTEM")]).status, 403);
    let r = b.post(view, &[("copy", "Copy to *SYSTEM")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/apps?cert=ADMIN-CERT01")
    );
    let t = b.get("/certs/apps?cert=ADMIN-CERT01").text();
    assert!(
        t.contains("HTP8309") && t.contains("Certificate ADMIN-CERT01 copied to store *SYSTEM."),
        "{t}"
    );
    assert!(
        t.contains("<option selected>ADMIN-CERT01 (expires") && !t.contains("HTP8314"),
        "{t}"
    );
    let opt = t
        .split("<option selected>ADMIN-CERT01")
        .nth(1)
        .unwrap()
        .split("</option>")
        .next()
        .unwrap();
    let opt = format!("ADMIN-CERT01{opt}");
    let r = b.post(
        "/certs/apps",
        &[
            ("app", "QS400_HTTP_SERVER_ADMIN"),
            ("cert", &opt),
            ("assign", "Assign"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/certs/apps"));
    assert_eq!(
        b.fx.certs.app_label("QS400_HTTP_SERVER_ADMIN").as_deref(),
        Some("ADMIN-CERT01")
    );
    // The old ID names the same application.
    assert_eq!(
        b.fx.certs.app_label("QIBM_HTTP_SERVER_ADMIN").as_deref(),
        Some("ADMIN-CERT01")
    );
    let row =
        b.fx.certs
            .entries("*SYSTEM")
            .unwrap()
            .into_iter()
            .find(|r| r.label == "ADMIN-CERT01")
            .unwrap();
    assert_eq!(row.kind, httpadmin::env::EntryType::Server);
    assert!(
        b.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Change { object, .. } if object == "QSTORE01"))
    );
    // A second copy is refused: the label is in use.
    b.post(view, &[("copy", "Copy to *SYSTEM")]);
    assert!(b.get(view).text().contains("HTP8320"));
}

/// Export with the private key, then import the file into *SYSTEM.
#[test]
fn export_with_key_then_import_into_the_system_store() {
    let mut b = Browser::signed_on();
    b.post(
        "/certs/stores",
        &[
            ("sn", "QSTORE01"),
            ("sp", "/QIBM/UserData/ICSS/Cert/"),
            ("p1", "Zebra-Secret-77"),
            ("p2", "Zebra-Secret-77"),
        ],
    );
    cert_in(&b, "QSTORE01", "ADMIN-CERT01", "qs400.ad.quartz.example");
    let export = "/certs/cert?store=QSTORE01&label=ADMIN-CERT01&export=1";
    let t = b.get(export).text();
    assert!(
        t.contains("Export with private key (PKCS#12)") && t.contains("value=\"Download PKCS#12\""),
        "{t}"
    );
    // A request has no key to export.
    assert!(
        !b.get("/certs/cert?store=WEBSTORE&label=webstore-2027-request&export=1")
            .text()
            .contains("Download PKCS#12")
    );
    let mut pages = vec![t];
    b.post(
        export,
        &[
            ("p1", "File-Secret-31"),
            ("p2", "File-Secret-32"),
            ("p12", "Download PKCS#12"),
        ],
    );
    let t = b.get(export).text();
    assert!(t.contains("HTP8322"), "{t}");
    pages.push(t);
    let r = b.post(
        export,
        &[
            ("p1", "File-Secret-31"),
            ("p2", "File-Secret-31"),
            ("p12", "Download PKCS#12"),
        ],
    );
    assert_eq!(r.status, 200);
    assert!(
        r.header("Content-Disposition")
            .unwrap_or("")
            .contains("ADMIN-CERT01.p12")
    );
    let p12 = r.bytes().to_vec();
    let r = upload(
        &mut b,
        &[
            ("store", "*SYSTEM"),
            ("fmt", "p12"),
            ("pw", "File-Secret-31"),
            ("t", "srv"),
            ("label", "ADMIN-CERT01"),
            ("import", "Import"),
        ],
        Some(("ADMIN-CERT01.p12", &p12)),
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/httpadmin/certs/store/*SYSTEM")
    );
    pages.push(b.get("/certs/store/*SYSTEM").text());
    let row =
        b.fx.certs
            .entries("*SYSTEM")
            .unwrap()
            .into_iter()
            .find(|r| r.label == "ADMIN-CERT01")
            .unwrap();
    assert_eq!(row.kind, httpadmin::env::EntryType::Server);
    b.fx.certs
        .assign("QSECOFR", "QIBM_HTTP_SERVER_ADMIN", Some("ADMIN-CERT01"))
        .unwrap();
    assert_eq!(
        b.fx.certs.app_label("QS400_HTTP_SERVER_ADMIN").as_deref(),
        Some("ADMIN-CERT01")
    );
    pages.push(b.get("/certs/apps").text());
    assert_eq!(leaks(&b, &pages, "File-Secret-31"), Vec::<String>::new());
}
