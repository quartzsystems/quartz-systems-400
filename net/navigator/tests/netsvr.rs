// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer: the pages against their approved renderings
//! (`tests/golden/nav/netsvr-*.html`; `QS400_UPDATE_GOLDEN=1` rewrites
//! them after a deliberate change), and what the actions do through
//! the APIs over the fixture's host.

#![allow(clippy::many_single_char_names, clippy::doc_markdown)]

mod common;

use common::{Browser, golden};
use navigator::env::Audit;
use navigator::env::netsvr::NetServer;
use qsweb::golden::{Normalize, diff};

/// The page at `route`, served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

/// Compare `served` with approved page `file`, or write it.
fn approved(b: &Browser, served: &str, file: &str) {
    let path = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        let text = served.replace(&b.csrf, "CSRF");
        std::fs::write(&path, text).expect("writes the golden page");
        return;
    }
    let g = golden(file);
    if let Some(d) = diff(served, &g, &Normalize { url: &|_| None }) {
        panic!("{file} differs from the served page at {d}");
    }
}

fn called(b: &Browser) -> Vec<String> {
    std::mem::take(&mut *b.fx.netsvr.called.lock().unwrap())
}

#[test]
fn pages_are_the_approved_ones() {
    let mut b = Browser::signed_on();
    for (route, file) in [
        ("/netsvr", "nav/netsvr.html"),
        ("/netsvr/shares", "nav/netsvr-shares.html"),
        ("/netsvr/shares/DOCS", "nav/netsvr-share.html"),
        ("/netsvr/shares/add", "nav/netsvr-share-add.html"),
        ("/netsvr/prtshr", "nav/netsvr-prtshr.html"),
        ("/netsvr/prtshr/PRT01", "nav/netsvr-prtshare.html"),
        ("/netsvr/sessions", "nav/netsvr-sessions.html"),
        ("/netsvr/sessions/1", "nav/netsvr-session.html"),
        ("/netsvr/disabled", "nav/netsvr-disabled.html"),
        ("/netsvr/confirm?op=end", "nav/netsvr-end.html"),
        ("/netsvr/shares/HOME/confirm", "nav/netsvr-remove.html"),
        ("/netsvr/sessions/1/confirm", "nav/netsvr-endsess.html"),
        (
            "/netsvr/disabled/TESTUSER/confirm",
            "nav/netsvr-enable.html",
        ),
    ] {
        let t = ok(&mut b, route);
        approved(&b, &t, file);
    }
    // Every page has its help.
    for key in ["netsvr", "nsshares", "nsprtshr", "nssessions", "nsdisabled"] {
        assert!(
            navigator::help::TOPICS.iter().any(|t| t.key == key),
            "help {key}"
        );
    }
}

#[test]
fn the_server_page_reads_current_and_pending() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/netsvr");
    // Current 6000, pending 3600; 3.0 pending.
    assert!(
        t.contains("<td class=\"mono\"><span class=\"mono\">6000</span></td>")
            || t.contains(">6000<"),
        "{t}"
    );
    assert!(t.contains("value=\"3600\""), "{t}");
    assert!(t.contains("<option selected>3.0</option>"), "{t}");
    assert!(t.contains("123301/QSYS/QZLSSERVER"), "{t}");
    assert_eq!(called(&b), ["QZLSOLST", "QZLSOLST"]);
}

#[test]
fn apply_makes_values_pending_through_the_apis() {
    let mut b = Browser::signed_on();
    let fields = |idle: &str, domain: &str| {
        vec![
            ("apply", "Apply".to_string()),
            ("name", "QSHQ400".into()),
            ("domain", domain.into()),
            ("text", "Quartz Systems head office".into()),
            ("sysname", "*NO".into()),
            ("guest", "QSHGUEST".into()),
            ("codepage", "0".into()),
            ("idle", idle.into()),
            ("oplock", "30".into()),
            ("browse", "720000".into()),
            ("msgauth", "0".into()),
            ("encrypt", "0".into()),
            ("autl", "*NONE".into()),
            ("netbios", "*NO".into()),
            ("dialect", "3.0".into()),
            ("logon", "*NO".into()),
        ]
    };
    let post = |b: &mut Browser, f: Vec<(&str, String)>| {
        let v: Vec<(&str, &str)> = f.iter().map(|(k, v)| (*k, v.as_str())).collect();
        b.post("/netsvr", &v)
    };
    called(&b);
    // Only the idle time-out changed: one QZLSCHSI.
    let r = post(&mut b, fields("900", "QSHDOM"));
    assert_eq!(r.header("Location"), Some("/qs400/navigator/netsvr"));
    assert_eq!(called(&b), ["QZLSOLST", "QZLSOLST", "QZLSCHSI"]);
    let a = b.fx.netsvr.attributes("QSECOFR").unwrap();
    assert_eq!(
        (a.current.idle_timeout, a.pending.idle_timeout),
        (6000, 900)
    );
    // A value the API refuses stays on the page with its message.
    let t = post(&mut b, fields("abc", "NEWDOM")).text();
    assert!(t.contains("CPFB686"), "{t}");
    assert!(t.contains("value=\"NEWDOM\""), "{t}");
    // Without *IOSYSCFG: the Not authorized page and an AF entry.
    let mut w = Browser::as_user("QPGMR");
    let r = post(&mut w, fields("900", "X"));
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPF2207"));
    assert!(
        w.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Refused { object, .. } if object == "QSYS/QZLSCHSI"))
    );
}

#[test]
fn start_and_end() {
    let mut b = Browser::signed_on();
    // Active: Start is refused by the API.
    let r = b.post("/netsvr", &[("start", "Start")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/netsvr"));
    assert!(ok(&mut b, "/netsvr").contains("CPFB685"));
    let r = b.post("/netsvr", &[("end", "End...")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/netsvr/confirm?op=end")
    );
    b.post("/netsvr/confirm?op=end", &[("ok", "OK")]);
    assert!(!b.fx.netsvr.status("QSECOFR").active);
    assert!(b.fx.netsvr.sessions("QSECOFR").is_empty());
    let t = ok(&mut b, "/netsvr");
    assert!(t.contains("Start and apply pending values"), "{t}");
    // Reset makes the pending values current.
    b.post("/netsvr", &[("reset", "Start and apply pending values")]);
    let a = b.fx.netsvr.attributes("QSECOFR").unwrap();
    assert_eq!(a.current.idle_timeout, 3600);
    assert!(b.fx.netsvr.status("QSECOFR").active);
}

fn file_fields<'a>(name: &'a str, path: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("ok", "OK"),
        ("share", name),
        ("path", path),
        ("text", "Project files"),
        ("perm", "Read/write"),
        ("max", "*NOMAX"),
        ("conv", "*NO"),
        ("ccsid", "0"),
        ("ext", ""),
        ("encrypt", "*NO"),
        ("autl", "*NONE"),
    ]
}

#[test]
fn file_shares_are_added_changed_and_removed() {
    let mut b = Browser::signed_on();
    let r = b.post("/netsvr/shares", &[("add", "Add file share...")]);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/netsvr/shares/add")
    );
    let r = b.post("/netsvr/shares/add", &file_fields("proj", "/home/QPGMR"));
    assert_eq!(r.header("Location"), Some("/qs400/navigator/netsvr/shares"));
    assert!(called(&b).contains(&"QZLSADFS".to_string()));
    let t = ok(&mut b, "/netsvr/shares");
    assert!(t.contains(">PROJ<") && t.contains("/home/QPGMR"), "{t}");
    // A duplicate: the form again, with the API's message.
    let t = b
        .post("/netsvr/shares/add", &file_fields("PROJ", "/home"))
        .text();
    assert!(
        t.contains("CPFB68A") && t.contains("value=\"/home\""),
        "{t}"
    );
    // Change: read only, extensions.
    let mut f = file_fields("PROJ", "/home/QPGMR");
    f[4] = ("perm", "Read only");
    f[8] = ("ext", "txt .csv");
    f[0] = ("apply", "Apply");
    let r = b.post("/netsvr/shares/PROJ", &f);
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/netsvr/shares/PROJ")
    );
    let t = ok(&mut b, "/netsvr/shares/PROJ");
    assert!(t.contains("value=\"TXT CSV\""), "{t}");
    // Remove, confirmed.
    b.post("/netsvr/shares/PROJ/confirm", &[("ok", "OK")]);
    assert!(!ok(&mut b, "/netsvr/shares").contains(">PROJ<"));
    assert_eq!(b.get("/netsvr/shares/PROJ").status, 404);
    // A print share is not a file share's page.
    assert_eq!(b.get("/netsvr/shares/PRT01").status, 404);
}

#[test]
fn owners_share_what_they_own() {
    // QPGMR has no *IOSYSCFG but owns /home/QPGMR.
    let mut b = Browser::as_user("QPGMR");
    let r = b.post("/netsvr/shares/add", &file_fields("MINE", "/home/QPGMR"));
    assert_eq!(r.header("Location"), Some("/qs400/navigator/netsvr/shares"));
    // Not /home, which QSYS owns: the API's refusal is Not authorized.
    let r = b.post("/netsvr/shares/add", &file_fields("ALL", "/home"));
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPFB684"), "{}", r.text());
}

#[test]
fn print_shares() {
    let mut b = Browser::signed_on();
    let f = [
        ("ok", "OK"),
        ("share", "LAB"),
        ("outq", "QGPL/QPRINT"),
        ("text", "Lab printer"),
        ("driver", "Generic PCL 6"),
        ("type", "*SCS"),
        ("prtf", "*NONE"),
        ("encrypt", "*NO"),
        ("autl", "*NONE"),
    ];
    let r = b.post("/netsvr/prtshr/add", &f);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/netsvr/prtshr"));
    let t = ok(&mut b, "/netsvr/prtshr");
    assert!(t.contains("QGPL/QPRINT") && t.contains("*SCS"), "{t}");
    // Without *IOSYSCFG, a queue someone else owns: Not authorized.
    let mut w = Browser::as_user("WEBUSER");
    let mut g = f;
    g[1] = ("share", "NOQ");
    let r = w.post("/netsvr/prtshr/add", &g);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPFB684"), "{}", r.text());
}

#[test]
fn sessions_end() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/netsvr/sessions");
    assert!(t.contains("ACCT-PC01") && t.contains("Guest"), "{t}");
    let t = ok(&mut b, "/netsvr/sessions/1");
    assert!(t.contains(">HOME<") && t.contains(">PRT01<"), "{t}");
    // Without *IOSYSCFG.
    let mut w = Browser::as_user("QSYSOPR");
    assert_eq!(
        w.post("/netsvr/sessions/1/confirm", &[("ok", "OK")]).status,
        403
    );
    b.post("/netsvr/sessions/1/confirm", &[("ok", "OK")]);
    assert_eq!(b.get("/netsvr/sessions/1").status, 404);
    assert!(called(&b).contains(&"QZLSENSS".to_string()));
}

#[test]
fn disabled_users_are_enabled() {
    // QSYSOPR may not manage the profile: the API's message.
    let mut w = Browser::as_user("QSYSOPR");
    w.post("/netsvr/disabled/TESTUSER/confirm", &[("ok", "OK")]);
    assert!(ok(&mut w, "/netsvr/disabled").contains("CPFB694"));
    let mut b = Browser::signed_on();
    assert!(ok(&mut b, "/netsvr/disabled").contains("TESTUSER"));
    b.post("/netsvr/disabled/TESTUSER/confirm", &[("ok", "OK")]);
    let t = ok(&mut b, "/netsvr/disabled");
    assert!(t.contains("No users are disabled for NetServer."), "{t}");
}
