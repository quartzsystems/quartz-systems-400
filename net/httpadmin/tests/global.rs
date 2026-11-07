// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Global settings: authority, validation, what is written, the
//! session timeout, the networks and TLS rules, the audit flag.

mod common;

use common::Browser;
use httpadmin::env::Audit;
use httpadmin::fixture::PASSWORD;
use httpadmin::settings::Settings;

fn form<'a>(over: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut f = vec![
        ("usrprf", "QTMHHTTP"),
        ("root", "/www/"),
        ("thr", "40"),
        ("max", "50"),
        ("maxthr", "200"),
        ("to", "30"),
        ("from", ""),
        ("audit", "on"),
        ("apply", "Apply"),
    ];
    for (k, v) in over {
        f.retain(|(a, _)| a != k);
        if !v.is_empty() || *k == "from" {
            f.push((k, v));
        }
    }
    f
}

#[test]
fn needs_iosyscfg() {
    let mut b = Browser::new();
    let s = Settings {
        admin_group: "WEBUSERS".into(),
        ..Settings::default()
    };
    s.save(b.fx.files.as_ref()).unwrap();
    assert_eq!(b.sign_on("WEBUSER", PASSWORD).status, 303);
    let r = b.get("/global");
    assert_eq!(r.status, 403);
    assert!(r.text().contains("*IOSYSCFG"));
    assert_eq!(b.post("/global", &form(&[("to", "5")])).status, 403);
    assert_eq!(Settings::load(b.fx.files.as_ref()).session_minutes, 30);
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { user, need, .. } if user == "WEBUSER" && need == "*IOSYSCFG")));
}

#[test]
fn apply_writes_the_settings_and_audits() {
    let mut b = Browser::signed_on();
    assert_eq!(b.post_raw("/global", &form(&[("to", "45")])).status, 403);
    let r = b.post(
        "/global",
        &form(&[
            ("usrprf", "webadm"),
            ("root", "/www/sites"),
            ("thr", "60"),
            ("to", "45"),
            ("from", "10.0.0.0/8, ::1/128"),
        ]),
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/global"));
    let t = b.get("/global").text();
    assert!(t.contains("HTP8330"), "{t}");
    assert!(t.contains("value=\"10.0.0.0/8, ::1/128\"") && t.contains("value=\"/www/sites/\""));
    let s = Settings::load(b.fx.files.as_ref());
    assert_eq!(
        (
            s.default_user.as_str(),
            s.default_threads,
            s.session_minutes
        ),
        ("WEBADM", 60, 45)
    );
    assert!(!s.admin_tls);
    let audit = b.fx.system.audit.lock().unwrap();
    let zc = audit.iter().find_map(|a| match a {
        Audit::Change { object, what, .. } if object == httpadmin::settings::PATH => {
            Some(what.clone())
        }
        _ => None,
    });
    assert!(zc.unwrap().contains("session timeout 30 to 45"));
    drop(audit);
    // OK goes back to the instances; Cancel changes nothing.
    let r = b.post(
        "/global",
        &form(&[("to", "20"), ("apply", ""), ("ok", "OK")]),
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    let r = b.post(
        "/global",
        &form(&[("to", "99"), ("apply", ""), ("cancel", "Cancel")]),
    );
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
    assert_eq!(Settings::load(b.fx.files.as_ref()).session_minutes, 20);
}

#[test]
fn refuses_bad_values() {
    let mut b = Browser::signed_on();
    for (over, text) in [
        (vec![("usrprf", "NOSUCH")], "HTP8201"),
        (vec![("root", "www")], "must start with /"),
        (vec![("thr", "300")], "exceed the maximum"),
        (vec![("to", "0")], "the session timeout is 1 to 1440"),
        (
            vec![("from", "10.0.0.0/40")],
            "is not an address or network",
        ),
        (
            vec![("from", "192.168.0.0/16")],
            "do not include your own address 10.0.10.55",
        ),
    ] {
        let r = b.post("/global", &form(&over));
        assert_eq!(r.header("Location"), Some("/qs400/httpadmin/global"));
        let t = b.get("/global").text();
        assert!(t.contains(text), "{over:?}: {t}");
    }
    assert_eq!(Settings::load(b.fx.files.as_ref()), Settings::default());
}

#[test]
fn audit_off_and_tls_take_effect() {
    let mut b = Browser::signed_on();
    // Turning auditing off is itself audited; later changes are not.
    b.post(
        "/global",
        &[
            ("usrprf", "QTMHHTTP"),
            ("root", "/www/"),
            ("thr", "40"),
            ("max", "50"),
            ("maxthr", "200"),
            ("to", "30"),
            ("from", ""),
            ("apply", "Apply"),
        ],
    );
    let n = b.fx.system.audit.lock().unwrap().len();
    assert!(n > 0);
    b.get("/global");
    b.post(
        "/global",
        &[
            ("usrprf", "QTMHHTTP"),
            ("root", "/www/"),
            ("thr", "41"),
            ("max", "50"),
            ("maxthr", "200"),
            ("to", "30"),
            ("from", ""),
            ("apply", "Apply"),
        ],
    );
    assert_eq!(b.fx.system.audit.lock().unwrap().len(), n);
    // TLS required: plain requests go to 2010.
    b.post("/global", &form(&[("admtls", "on"), ("audit", "")]));
    let r = b.get("/instances");
    assert_eq!(r.status, 303);
    assert!(
        r.header("Location").unwrap().starts_with("https://")
            && r.header("Location")
                .unwrap()
                .contains(":2010/qs400/httpadmin/instances")
    );
    let s = Settings::load(b.fx.files.as_ref());
    assert!(s.admin_tls && !s.audit);
    // The TLS port went down: 2001 serves instead of redirecting there.
    b.admin.set_tls_up(false);
    assert_eq!(b.get("/instances").status, 200);
}

#[test]
fn tls_is_not_required_while_its_port_is_down() {
    let mut b = Browser::signed_on();
    b.admin.set_tls_up(false);
    let r = b.post("/global", &form(&[("admtls", "on")]));
    let page = b
        .get(r.header("Location").unwrap_or("/qs400/httpadmin/global"))
        .text();
    assert!(page.contains("is not listening"), "{page}");
    assert!(!Settings::load(b.fx.files.as_ref()).admin_tls);
}
