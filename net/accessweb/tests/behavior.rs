// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions, sign-on rules, CSRF, policies, preferences, links: what
//! holds for every page.

mod common;

use std::collections::{BTreeSet, VecDeque};

use accessweb::PRODUCT;
use accessweb::env::Audit;
use accessweb::env::policy::{PolicyStore, Setting};
use accessweb::fixture::PASSWORD;
use accessweb::policy;
use common::{Browser, page_links, proto};

#[test]
fn sign_on_rules() {
    let mut b = Browser::new();
    // Not signed on: every page sends you to the sign-on page.
    let r = b.get("/home");
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/accessweb/signon")));
    // A wrong password stays on the page with the message and a PW entry.
    let r = b.sign_on("cwellman", "nope");
    assert_eq!(r.status, 200);
    assert!(r.text().contains("CPF22E2"), "{}", r.text());
    assert!(r.text().contains("value=\"CWELLMAN\""));
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Password { code: 'P', .. })));
    // Any profile the system accepts signs on.
    let r = b.sign_on("webuser", PASSWORD);
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/accessweb/home")));
    assert!(r.header("Set-Cookie").unwrap().contains("HttpOnly; SameSite=Strict"));
    // The password warning shows once, on the first page.
    let t = b.get("/home").text();
    assert!(t.contains("User: <b>WEBUSER</b>") && t.contains("QAD4031"), "{t}");
    assert!(!b.get("/home").text().contains("QAD4031"));
    // The previous sign-on and where it came from.
    assert!(t.contains("Last sign-on 10/29/26 13:05:00"), "{t}");
    // Sign off ends the session.
    let r = b.get("/signoff");
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/signon"));
    assert_eq!(b.get("/home").status, 303);
}

#[test]
fn sign_on_policy() {
    let mut b = Browser::new();
    b.fx.policy.put("WEBUSERS", &[(policy::SIGNON.into(), Setting::Deny)]).unwrap();
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.status, 200);
    assert!(r.text().contains("XWA0001"), "{}", r.text());
    assert!(b.cookie.is_none());
    // Others still sign on.
    assert_eq!(b.sign_on("CWELLMAN", PASSWORD).status, 303);
}

#[test]
fn expired_password() {
    let mut b = Browser::new();
    b.fx.system.expired.lock().unwrap().push("WEBUSER".into());
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/password"));
    let t = b.get("/password").text();
    assert!(t.contains("QAD4022"), "{t}");
    // Nothing else opens.
    assert_eq!(b.get("/home").header("Location"), Some("/qs400/accessweb/password"));
    // Mismatched confirmation: the desktop's message, nothing sent.
    let t = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS2"), ("change", "Change")]).text();
    assert!(t.contains("QAD4027"), "{t}");
    // The system's rules.
    let t = b.post("/password", &[("p0", PASSWORD), ("p1", "SHORT"), ("p2", "SHORT"), ("change", "Change")]).text();
    assert!(t.contains("CPF2234"), "{t}");
    let r = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS1"), ("change", "Change")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/signon?changed=WEBUSER"));
    assert!(b.get("/signon?changed=WEBUSER").text().contains("QAD4023"));
    assert_eq!(b.sign_on("WEBUSER", "NEWPASS1").header("Location"), Some("/qs400/accessweb/home"));
}

#[test]
fn change_password() {
    let mut b = Browser::signed_on();
    let r = b.post("/password", &[("p0", PASSWORD), ("p1", "NEWPASS1"), ("p2", "NEWPASS1"), ("change", "Change")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/home"));
    let t = b.get("/home").text();
    assert!(t.contains("QAD4023</a> - Password changed for CWELLMAN on QSHQ400."), "{t}");
    assert!(!b.get("/password").text().contains("QAD4031"));
    let t = b.post("/password", &[("p0", "wrong"), ("p1", "NEWPASS2"), ("p2", "NEWPASS2"), ("change", "Change")]).text();
    assert!(t.contains("CPF22E2"), "{t}");
}

#[test]
fn posts_need_the_token() {
    let mut b = Browser::signed_on();
    assert_eq!(b.post_raw("/preferences", &[("apply", "Apply")]).status, 403);
    assert_eq!(b.post_raw("/preferences", &[("csrf", "0000"), ("apply", "Apply")]).status, 403);
}

#[test]
fn policies_effective_and_changed() {
    let mut b = Browser::signed_on();
    // WEBUSER inherits WEBUSERS' Deny.
    let t = b.get("/policies?p=WEBUSER").text();
    assert!(t.contains("Effective policies for WEBUSER, including group WEBUSERS."));
    // Deny Print screen for WEBUSER itself: only the change is kept.
    let mut fields: Vec<(String, String)> = Vec::new();
    for s in policy::SECTIONS {
        for (i, (key, _)) in s.functions.iter().enumerate() {
            let denied = accessweb::fixture::policy::WEBUSERS_DENIED.contains(key) || *key == policy::S5250_PRINT;
            fields.push((format!("p_{}_{i}", s.key), if denied { "D" } else { "A" }.into()));
        }
    }
    fields.push(("apply".into(), "Apply".into()));
    let f: Vec<(&str, &str)> = fields.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let r = b.post("/policies?p=WEBUSER", &f);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=WEBUSER"));
    assert_eq!(b.fx.policy.settings("WEBUSER"), [(policy::S5250_PRINT.to_string(), Setting::Deny)]);
    let t = b.get("/policies?p=WEBUSER").text();
    assert!(t.contains("XWA0002"), "{t}");
    assert!(t.contains("User WEBUSER"), "{t}");
    // Deny wins: *PUBLIC's Deny reaches CWELLMAN too.
    b.fx.policy.put("*PUBLIC", &[(policy::DB_SQL.into(), Setting::Deny)]).unwrap();
    assert!(!policy::allowed(&b.fx.env, "CWELLMAN", policy::DB_SQL));
    b.fx.policy.put("CWELLMAN", &[(policy::DB_SQL.into(), Setting::Allow)]).unwrap();
    assert!(!policy::allowed(&b.fx.env, "CWELLMAN", policy::DB_SQL));
    // Copy policies from WEBUSER to group WAREHOUSE.
    let r = b.post("/policies/copy?p=WAREHOUSE", &[("from", "WEBUSER (user)"), ("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=WAREHOUSE"));
    assert_eq!(b.fx.policy.settings("WAREHOUSE"), b.fx.policy.settings("WEBUSER"));
    // Go picks another profile.
    let r = b.post("/policies", &[("p", "ADMINS (group)")]);
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/policies?p=ADMINS"));
}

#[test]
fn policies_need_secadm() {
    let mut b = Browser::as_user("WEBUSER");
    let r = b.get("/policies");
    assert_eq!(r.status, 403);
    let t = r.text();
    assert!(t.contains("XWA0001") && t.contains("Customize - Change policies (administrator)"), "{t}");
    assert!(t.contains("by the system: it needs special authority"), "{t}");
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { need, .. } if need == "policy policies.change")));
    // Direct POST refused the same way.
    assert_eq!(b.post("/policies?p=WEBUSER", &[("apply", "Apply")]).status, 403);
}

#[test]
fn denied_function_refused_on_post() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy.put("WEBUSERS", &[(policy::PREFS_CHANGE.into(), Setting::Deny)]).unwrap();
    let t = b.get("/preferences").text();
    assert!(t.contains("name=\"apply\" value=\"Apply\" disabled"), "{t}");
    let r = b.post("/preferences", &[("apply", "Apply"), ("rpp", "50")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("XWA0001"));
    assert!(r.text().contains("is set to <b>Deny</b> for group <span class=\"mono\">WEBUSERS</span>."));
}

#[test]
fn preferences_saved_and_used() {
    let mut b = Browser::signed_on();
    let r = b.post(
        "/preferences?tab=database",
        &[("ds", "QGPL"), ("nm", "sys"), ("do", "CSV"), ("mr", "100"), ("apply", "Apply")],
    );
    assert_eq!(r.header("Location"), Some("/qs400/accessweb/preferences?tab=database"));
    let t = b.get("/preferences?tab=database").text();
    assert!(t.contains("XWA0003") && t.contains("<option selected>QGPL</option>"), "{t}");
    assert!(t.contains("value=\"sys\" checked") && !t.contains("Rows per page"), "{t}");
    // A tab's post leaves the others' fields alone.
    let stored = b.fx.prefs.map.lock().unwrap().get("CWELLMAN").cloned().unwrap();
    assert!(stored.contains(&("ss".into(), "27".into())), "{stored:?}");
    // A refused value: nothing saved.
    let t = b.post("/preferences?tab=database", &[("ds", "QGPL"), ("nm", "sql"), ("do", "CSV"), ("mr", "x"), ("apply", "Apply")]).text();
    assert!(t.contains("XWA0004</a> - Value x is not valid for Maximum rows returned."), "{t}");
    // Dates as the user chose them.
    b.post(
        "/preferences",
        &[("rpp", "25"), ("df", "YYYY-MM-DD"), ("tf", "HH:MM:SS (24 hour)"), ("tz", "UTC"), ("lang", "English (US)"), ("h", "win"), ("ds", "QGPL"), ("nm", "sql"), ("do", "HTML"), ("mr", "500"), ("ss", "24"), ("cl", "g"), ("fs", "10pt"), ("cu", "Underline"), ("hl", "compact"), ("ok", "OK")],
    );
    let t = b.get("/home").text();
    assert!(t.contains("Last sign-on 2026-10-29 17:05:00"), "{t}");
    // Compact, no counts (the box was not sent).
    assert!(!t.contains("class=\"home\"") && !t.contains("spooled files"), "{t}");
    // Restore defaults.
    b.post("/preferences", &[("restore_defaults", "Restore defaults")]);
    let stored = b.fx.prefs.map.lock().unwrap().get("CWELLMAN").cloned().unwrap();
    assert_eq!(stored, [("_from".to_string(), "10.0.10.55".to_string())]);
}

#[test]
fn home_follows_policies() {
    let mut b = Browser::as_user("WEBUSER");
    b.fx.policy.put("WEBUSERS", &[(policy::DB_BROWSE.into(), Setting::Deny)]).unwrap();
    let t = b.get("/home").text();
    assert!(!t.contains(">Database</a><br>") && t.contains(">Files</a><br>"), "{t}");
}

#[test]
fn messages_and_help_resolve() {
    let mut b = Browser::signed_on();
    let r = b.get("/msg/XWA0001?d=Jobs%20-%20End%20server%20jobs&d=WEBUSER");
    assert_eq!(r.status, 200);
    let t = r.text();
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("<h2>Message</h2>") < at("<h2>Cause</h2>") && at("<h2>Cause</h2>") < at("<h2>Recovery</h2>"));
    assert!(t.contains("Function Jobs - End server jobs is not allowed for user profile WEBUSER"));
    assert_eq!(b.get("/msg/CPF9999").status, 404);
    assert!(b.get("/help/policies").text().contains("<title>Help: Policies - QS/400</title>"));
    assert!(b.get("/help/nothing").text().contains("Contents"));
    assert_eq!(b.get(qsweb::frame::CSS_PATH).bytes(), qsweb::CSS);
}

#[test]
fn areas_not_built_say_so() {
    let mut b = Browser::signed_on();
    // The first area still to come.
    let Some(route) = PENDING.first().map(|p| p.1[0]) else { return };
    let r = b.get(route);
    assert_eq!(r.status, 501);
    assert!(r.text().contains("XWA0099"), "{}", r.text());
}

/// Routes not built yet, by area: each area's agent removes its own.
const PENDING: &[(&str, &[&str])] = &[
    ("print", &["/print"]),
    ("msgs", &["/messages"]),
    ("jobs", &["/jobs"]),
    ("cmd", &["/cmd"]),
    ("download", &["/download"]),
];

/// Every link and form on every reachable page resolves to a page of
/// this product (or the pages of others it links to).
#[test]
fn crawl() {
    let mut b = Browser::signed_on();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([PRODUCT.url("/home")]);
    let mut broken = Vec::new();
    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.clone())
            || url.ends_with("/signoff")
            || url.contains("/qs400/navigator/")
            || url.contains("/qs400/httpadmin/")
        {
            continue;
        }
        let r = b.get(&url);
        // The tree's [+] redirects.
        if r.status == 303
            && let Some(to) = r.header("Location").filter(|l| l.starts_with(PRODUCT.prefix))
        {
            queue.push_back(to.to_string());
            continue;
        }
        if r.status != 200 {
            let route = url.strip_prefix(PRODUCT.prefix).unwrap_or(&url);
            if PENDING.iter().flat_map(|p| p.1.iter()).any(|p| route.starts_with(p)) {
                continue;
            }
            broken.push(format!("{url}: {}", r.status));
            continue;
        }
        if url == qsweb::frame::CSS_PATH {
            continue;
        }
        let page = url.split('?').next().unwrap_or("").to_string();
        for (method, href) in page_links(&r.text()) {
            let abs = if href.is_empty() {
                url.clone()
            } else if href.starts_with('?') {
                format!("{page}{href}")
            } else {
                href.clone()
            };
            if abs == "#" {
                continue;
            }
            assert!(proto(&abs).is_some() || abs == qsweb::frame::CSS_PATH, "{url} links to {abs}");
            if method == "GET" {
                queue.push_back(abs);
            }
        }
    }
    assert!(broken.is_empty(), "broken links: {broken:#?}");
    assert!(seen.len() > 5, "{seen:?}");
}

/// No other vendor's product or feature names in the sources or pages.
#[test]
fn no_ibm_wording() {
    let words = [
        "IBM", "iSeries", "System i", "AS/400", "OS/400", "i5/OS", "WebSphere", "Navigator for i", "Access Client Solutions",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    for dir in ["src", "tests/behavior.rs", "tests/golden.rs", "tests/common", "tests/golden/web"] {
        for f in walk(&root.join(dir)) {
            let t = std::fs::read_to_string(&f).unwrap_or_default();
            for (n, line) in t.lines().enumerate() {
                if line.contains("let words") || line.contains("\"IBM\", \"iSeries\"") {
                    continue;
                }
                for w in words {
                    if line.contains(w) {
                        hits.push(format!("{}:{}: {w}", f.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(hits.is_empty(), "{hits:#?}");
}

fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
    if d.is_file() {
        return vec![d.to_path_buf()];
    }
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(d) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}
