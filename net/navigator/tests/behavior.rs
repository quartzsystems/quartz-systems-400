// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions, sign-on rules, CSRF, links: what holds for every page.

mod common;

use std::collections::{BTreeSet, VecDeque};

use common::{Browser, page_links, proto};
use navigator::PRODUCT;
use navigator::env::Audit;
use navigator::fixture::PASSWORD;

#[test]
fn sign_on_rules() {
    let mut b = Browser::new();
    // Not signed on: every page sends you to the sign-on page.
    let r = b.get("/home");
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/navigator/signon")));
    // A wrong password stays on the page with the message and a PW entry.
    let r = b.sign_on("qsecofr", "nope");
    assert_eq!(r.status, 200);
    assert!(r.text().contains("CPF22E2"), "{}", r.text());
    assert!(r.text().contains("value=\"QSECOFR\""));
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Password { code: 'P', .. })));
    // Any profile the system accepts signs on.
    let r = b.sign_on("webuser", PASSWORD);
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/navigator/home")));
    assert!(r.header("Set-Cookie").unwrap().contains("HttpOnly; SameSite=Strict"));
    assert!(b.get("/home").text().contains("User: <b>WEBUSER</b>"));
    // Sign off ends the session.
    let r = b.get("/signoff");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/signon"));
    assert_eq!(b.get("/home").status, 303);
}

#[test]
fn posts_need_the_token() {
    let mut b = Browser::signed_on();
    assert_eq!(b.post_raw("/home", &[("x", "1")]).status, 403);
    assert_eq!(b.post_raw("/home", &[("csrf", "0000"), ("x", "1")]).status, 403);
    assert_eq!(b.post("/home", &[("x", "1")]).status, 200);
}

#[test]
fn messages_and_help_resolve() {
    let mut b = Browser::signed_on();
    let r = b.get("/msg/CPF2207?d=ADMIN&d=QSYS&d=*JOB");
    assert_eq!(r.status, 200);
    let t = r.text();
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("<h2>Message</h2>") < at("<h2>Cause</h2>") && at("<h2>Cause</h2>") < at("<h2>Recovery</h2>"));
    assert!(t.contains("Not authorized to use object ADMIN in library QSYS type *JOB."));
    assert_eq!(b.get("/msg/CPF9999").status, 404);
    assert!(b.get("/help/home").text().contains("<title>Help: System status - QS/400</title>"));
    assert!(b.get("/help/nothing").text().contains("Contents"));
    assert_eq!(b.get(qsweb::frame::CSS_PATH).bytes(), qsweb::CSS);
}

#[test]
fn auto_refresh() {
    let mut b = Browser::signed_on();
    let t = b.get("/home?auto=120").text();
    assert!(t.contains("<meta http-equiv=\"refresh\" content=\"120\" />"));
    assert!(t.contains("<option selected>120</option>"));
    // Only the offered intervals.
    assert!(!b.get("/home?auto=1").text().contains("http-equiv=\"refresh\""));
}

#[test]
fn tree_groups_stay_open() {
    let mut b = Browser::signed_on();
    let r = b.get("/nav?open=Network&back=/qs400/navigator/home");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/home"));
    let t = b.get("/home").text();
    assert!(t.contains(">TCP/IP configuration</a>") && t.contains(">System status</a>"));
    b.get("/nav?close=Network&back=/qs400/navigator/home");
    assert!(!b.get("/home").text().contains(">TCP/IP configuration</a>"));
    // Only back to this product.
    let r = b.get("/nav?open=Network&back=//evil.example/");
    assert_eq!(r.header("Location"), Some("/qs400/navigator/"));
}

/// Routes not built yet: each step removes its own.
const PENDING: &[&str] = &[
    // Security, software, performance.
    "/audit", "/licpgm", "/ptf", "/fixes",
];

/// Every link and form on every reachable page resolves to a page of
/// this product (or HTTP Server Administration's instances).
#[test]
fn crawl() {
    let mut b = Browser::signed_on();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([PRODUCT.url("/home")]);
    let mut broken = Vec::new();
    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.clone()) || url.ends_with("/signoff") || url.contains("/qs400/httpadmin/") {
            continue;
        }
        let r = b.get(&url);
        // A wizard's start link or the tree's [+] redirects.
        if r.status == 303
            && let Some(to) = r.header("Location").filter(|l| l.starts_with(PRODUCT.prefix))
        {
            queue.push_back(to.to_string());
            continue;
        }
        if r.status != 200 {
            let route = url.strip_prefix(PRODUCT.prefix).unwrap_or(&url);
            if PENDING.iter().any(|p| route.starts_with(p)) {
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
    for dir in ["src", "tests/behavior.rs", "tests/golden.rs", "tests/common"] {
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
