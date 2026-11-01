// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sessions, sign-on rules, CSRF, links: what holds for every page.

mod common;

use std::collections::{BTreeSet, VecDeque};

use common::{Browser, page_links, proto};
use httpadmin::PRODUCT;
use httpadmin::env::Audit;
use httpadmin::fixture::PASSWORD;

#[test]
fn sign_on_rules() {
    let mut b = Browser::new();
    // Not signed on: every page sends you to the sign-on page.
    let r = b.get("/instances");
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/httpadmin/signon")));
    // A wrong password stays on the page with the message and a PW entry.
    let r = b.sign_on("qsecofr", "nope");
    assert_eq!(r.status, 200);
    assert!(r.text().contains("CPF22E2"), "{}", r.text());
    assert!(r.text().contains("value=\"QSECOFR\""));
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Password { code: 'P', .. })));
    // A user without *IOSYSCFG is refused, with an AF entry.
    let r = b.sign_on("WEBUSER", PASSWORD);
    assert_eq!(r.status, 200);
    assert!(r.text().contains("HTP8111"));
    assert!(b.cookie.is_none());
    assert!(b.fx.system.audit.lock().unwrap().iter().any(|a| matches!(a, Audit::Refused { user, .. } if user == "WEBUSER")));
    // *IOSYSCFG alone is enough.
    let r = b.sign_on("webadm", PASSWORD);
    assert_eq!((r.status, r.header("Location")), (303, Some("/qs400/httpadmin/instances")));
    assert!(r.header("Set-Cookie").unwrap().contains("HttpOnly; SameSite=Strict"));
    assert!(b.get("/instances").text().contains("User: <b>WEBADM</b>"));
    // Sign off ends the session.
    let r = b.get("/signoff");
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/signon"));
    assert_eq!(b.get("/instances").status, 303);
}

#[test]
fn posts_need_the_token() {
    let mut b = Browser::signed_on();
    let r = b.post_raw("/instances", &[("x", "1")]);
    assert_eq!(r.status, 403);
    let r = b.post_raw("/instances", &[("csrf", "0000"), ("x", "1")]);
    assert_eq!(r.status, 403);
    assert_eq!(b.post("/instances", &[("x", "1")]).status, 200);
}

#[test]
fn messages_and_help_resolve() {
    let mut b = Browser::signed_on();
    let r = b.get("/msg/HTP8103?d=WEBSTORE");
    assert_eq!(r.status, 200);
    let t = r.text();
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("{s} missing"));
    assert!(at("<h2>Message</h2>") < at("<h2>Cause</h2>") && at("<h2>Cause</h2>") < at("<h2>Recovery</h2>"));
    assert!(t.contains("HTTP server instance WEBSTORE must be stopped first."));
    assert_eq!(b.get("/msg/HTP9999").status, 404);
    assert!(b.get("/help/instances").text().contains("<title>Help: Server instances - QS/400</title>"));
    assert!(b.get("/help/nothing").text().contains("Contents"));
    let css = b.get(qsweb::frame::CSS_PATH);
    assert_eq!(css.bytes(), qsweb::CSS);
}

#[test]
fn filters_sort_and_page() {
    let mut b = Browser::signed_on();
    let t = b.get("/instances?status=Stopped").text();
    assert!(t.contains(">WEBSTORE<") && t.contains(">TESTSRV<") && !t.contains(">ADMIN</a>"));
    assert!(t.contains("<option selected>Stopped</option>"));
    let t = b.get("/instances?name=A*&sort=name").text();
    let at: Vec<usize> = ["ACCESSWEB", "ADMIN", "APACHEDFT", "API"]
        .iter()
        .map(|n| t.find(&format!(">{n}</a>")).unwrap())
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
    assert!(!t.contains(">INTRANET</a>"));
    let t = b.get("/instances?rows=10&page=1").text();
    assert!(t.contains("Page 1 of 1") && t.contains("<option selected>10</option>"));
    assert!(b.get("/instances?name=NONE").text().contains("No instances match the filter."));
}

/// Routes not built yet; each step removes its own.
const PENDING: &[&str] = &["/certs/apps", "/global"];

/// Every link and form on every reachable page resolves to a page of
/// this product (or Navigator's, which is another product's to serve).
#[test]
fn crawl() {
    let mut b = Browser::signed_on();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([PRODUCT.url("/instances")]);
    let mut broken = Vec::new();
    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.clone()) || url.ends_with("/signoff") {
            continue;
        }
        let r = b.get(&url);
        if r.status != 200 {
            if PENDING.iter().any(|p| url.contains(p)) {
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
            if abs == "#" || abs.starts_with("/qs400/navigator/") {
                continue;
            }
            assert!(proto(&abs).is_some() || abs.starts_with(PRODUCT.prefix) || abs == qsweb::frame::CSS_PATH, "{url} links to {abs}");
            if method == "GET" {
                queue.push_back(abs);
            }
        }
    }
    assert!(broken.is_empty(), "broken links: {broken:#?}");
    assert!(seen.len() > 5, "{seen:?}");
}

/// No IBM product or feature names in the product's sources or pages.
#[test]
fn no_ibm_wording() {
    let words = ["IBM", "iSeries", "System i", "AS/400", "OS/400", "i5/OS", "WebSphere", "Digital Certificate Manager", "Navigator for i", "Access for Web for i"];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut hits = Vec::new();
    for dir in ["httpadmin/src", "qsweb/src", "qsweb/templates", "httpd/src"] {
        for f in walk(&root.join(dir)) {
            let t = std::fs::read_to_string(&f).unwrap_or_default();
            for (n, line) in t.lines().enumerate() {
                // IBM-compatible identifiers (QIBM_ application IDs,
                // /QIBM/ paths) are the compatibility interface.
                let l = line.replace("QIBM", "");
                for w in words {
                    if l.contains(w) {
                        hits.push(format!("{}:{}: {w}", f.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(hits.is_empty(), "{hits:#?}");
}

fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
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
