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
const PENDING: &[&str] = &[];

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
        // A wizard's start link sets it up and redirects into it.
        if r.status == 303
            && let Some(to) = r.header("Location").filter(|l| l.starts_with(PRODUCT.prefix))
        {
            queue.push_back(to.to_string());
            continue;
        }
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
                // The certificate store paths (/QIBM/...) are the
                // compatibility interface; application IDs are QS400_.
                let l = line.replace("/QIBM/", "/");
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

/// What the instance job asks for its TLS port: the certificate's
/// label, or the message ID it logs when the application cannot be
/// used.
fn job_trust(b: &Browser, app: &str) -> Result<Option<String>, &'static str> {
    let m = b.fx.certs.manager();
    certs::admin::identity::for_app(&m.ctx, app)
        .map(|t| t.identity.map(|i| i.label))
        .map_err(|e| certs::admin::msgid(&e))
}

/// A `*SYSTEM` server certificate the TLS page offers, not the
/// store's default.
fn a_server_cert(b: &Browser) -> String {
    use httpadmin::env::Certs as _;
    let soon = httpadmin::fixture::now() + 61 * 86_400;
    let rows = b.fx.certs.entries("*SYSTEM").unwrap();
    rows.into_iter()
        .find(|r| r.kind == httpadmin::env::EntryType::Server && r.not_after > soon && !r.is_default)
        .unwrap()
        .label
}

/// The `*SYSTEM` store's default certificate.
fn store_default(b: &Browser) -> Option<String> {
    use httpadmin::env::Certs as _;
    b.fx.certs.stores().into_iter().find(|s| s.name == "*SYSTEM").and_then(|s| s.default)
}

/// The page a redirect leads to, with its message.
fn flashed(b: &mut Browser, r: &qsweb::http::Response) -> String {
    assert_eq!(r.status, 303, "{}", r.text());
    let to = r.header("Location").unwrap().to_string();
    b.get(&to).text()
}

/// An instance made before instances registered their application
/// IDs: enabling TLS says what is missing, registers the ID, assigns
/// the certificate chosen; then the job finds it (no `TCP1C15`).
#[test]
fn tls_on_an_unregistered_instance() {
    use httpadmin::env::Certs as _;
    use httpd::files::Files as _;
    let mut b = Browser::signed_on();
    let spec = httpd::instance::Spec {
        name: "QSTEST".into(),
        description: "Test".into(),
        root: "/www/qstest".into(),
        docroot: "/www/qstest/htdocs".into(),
        sample: false,
        addr: None,
        port: 8080,
        access_log: None,
        keep_days: 0,
        user: "QTMHHTTP".into(),
        autostart: false,
        copy_from: None,
    };
    httpd::instance::create(b.fx.files.as_ref(), &spec, "11/01/26 10:00:00", 1).unwrap();
    let app = "QS400_HTTP_SERVER_QSTEST";
    assert_eq!(job_trust(&b, app), Err("TCP1C15"));

    // The page lists *SYSTEM's server certificates, none chosen, and
    // says the instance has no TLS port.
    let cert = a_server_cert(&b);
    let t = b.get("/instance/QSTEST/tls").text();
    assert!(t.contains("<select name=\"cert\"><option>(none)</option>"), "{t}");
    assert!(t.contains(&format!("<option>{cert} (expires ")), "{t}");
    assert!(t.contains("The instance has no TLS port.") && !t.contains("TLS 1.1 (not"), "{t}");
    let on = |c: &str| {
        let fields = [("tls", "on"), ("t13", "on"), ("t12", "on"), ("cipher", "*DFT (system value QSSLCSL)"), ("ca", "none"), ("apply", "Apply")];
        let mut v: Vec<(String, String)> = fields.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        v.push(("cert".into(), c.to_string()));
        v
    };
    let post = |b: &mut Browser, c: &str| {
        let f = on(c);
        let f: Vec<(&str, &str)> = f.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let r = b.post("/instance/QSTEST/tls", &f);
        flashed(b, &r)
    };
    let t = post(&mut b, &cert);
    assert!(t.contains("HTP8124") && !t.contains("HTP8120"), "{t}");

    // A TLS port registers the ID: unassigned, it has the store's
    // default; with no default TLS needs a certificate chosen.
    let r = b.post("/instance/QSTEST/ports", &[("addr", "*ALL"), ("port", "8443"), ("tls", "on"), ("add", "Add")]);
    assert!(flashed(&mut b, &r).contains("HTP8022"));
    let default = store_default(&b).unwrap();
    assert_eq!(job_trust(&b, app), Ok(Some(default.clone())));
    b.fx.certs.remove("QSECOFR", "*SYSTEM", &default).unwrap();
    assert_eq!(store_default(&b), None);
    assert!(post(&mut b, "(none)").contains("HTP8125"));

    assert!(post(&mut b, &format!("{cert} (expires 01/01/30)")).contains("HTP8022"));
    assert_eq!(b.fx.certs.app_label(app), Some(cert.clone()));
    assert_eq!(job_trust(&b, app), Ok(Some(cert.clone())));
    let c = String::from_utf8(b.fx.files.read("/www/qstest/conf/httpd.conf").unwrap()).unwrap();
    assert!(
        c.contains("<VirtualHost *:8443>\n   SSLEngine On\n   SSLAppName QS400_HTTP_SERVER_QSTEST\n   SSLProtocolDisable TLSv1 TLSv1.1\n</VirtualHost>"),
        "{c}"
    );
    // The assigned certificate comes first; no (none).
    let t = b.get("/instance/QSTEST/tls").text();
    assert!(t.contains(&format!("<select name=\"cert\"><option>{cert} (expires ")), "{t}");
    // The job's plan: TLS on 8443 for the application, 1.2 and 1.3 on.
    let plan = httpd::serve::site::Plan::new("QSTEST", "/www/qstest", &httpd::conf::parse(&c).unwrap());
    let tls = plan.listeners.iter().find(|l| l.port == 8443).and_then(|l| l.tls.clone()).unwrap();
    assert_eq!(tls.app_id, app);
    assert!(!tls.disabled.iter().any(|d| d == "TLSv1.2" || d == "TLSv1.3"));
}

/// A new instance has its application ID; deleting it takes the ID,
/// its files and its registry entry away.
#[test]
fn create_registers_and_delete_removes() {
    use httpadmin::env::Certs as _;
    use httpd::files::Files as _;
    let mut b = Browser::signed_on();
    b.post("/create/step/1", &[("create_new_instance", "x")]);
    let steps: [&[(&str, &str)]; 5] = [
        &[("name", "SHOP"), ("desc", "Shop"), ("base", "new")],
        &[],
        &[("docroot", "/www/shop/htdocs")],
        &[("addr", "*ALL"), ("port", "8090")],
        &[("log", "yes")],
    ];
    for (i, f) in steps.iter().enumerate() {
        let mut f = f.to_vec();
        f.push(("next", "Next >"));
        b.post(&format!("/create/step/{}", i + 1), &f);
    }
    let r = b.post("/create/step/6", &[("finish", "Create")]);
    assert!(flashed(&mut b, &r).contains("HTP8020"));
    let app = "QS400_HTTP_SERVER_SHOP";
    assert!(b.fx.certs.apps().iter().any(|a| a.id == app && a.server));
    assert!(job_trust(&b, app).is_ok());

    let r = b.post("/instance/SHOP/confirm?op=delete", &[("ok", "OK")]);
    assert!(flashed(&mut b, &r).contains("HTP8021"));
    assert!(!b.fx.files.exists("/www/shop"));
    assert!(!b.fx.certs.apps().iter().any(|a| a.id == app));
    assert!(httpd::registry::Registry::load(b.fx.files.as_ref()).unwrap().get("SHOP").is_none());
}

/// Files whose trees cannot be removed.
struct Stuck(std::sync::Arc<httpd::files::MemFiles>);

impl httpd::files::Files for Stuck {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        self.0.read(path)
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> std::io::Result<()> {
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
        Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{path}/logs/error_log: not authorized")))
    }
    fn list(&self, dir: &str) -> std::io::Result<Vec<httpd::files::Entry>> {
        self.0.list(dir)
    }
    fn stat(&self, path: &str) -> std::io::Result<httpd::files::Entry> {
        self.0.stat(path)
    }
}

/// A delete whose files cannot all be removed still deletes the
/// instance, and says which files remain.
#[test]
fn delete_with_files_left() {
    use httpd::files::Files as _;
    let fx = httpadmin::fixture::fixture();
    let env = httpadmin::Env {
        files: std::sync::Arc::new(Stuck(fx.files.clone())),
        ..fx.env.clone()
    };
    let mut b = Browser {
        admin: httpadmin::Admin::new(env),
        fx,
        cookie: None,
        csrf: String::new(),
    };
    assert_eq!(b.sign_on("QSECOFR", PASSWORD).status, 303);
    let r = b.post("/instance/WEBSTORE/confirm?op=delete", &[("ok", "OK")]);
    let t = flashed(&mut b, &r);
    assert!(t.contains("HTP8023") && t.contains("/www/webstore remain"), "{t}");
    assert!(!t.contains(">WEBSTORE</a>"), "{t}");
    assert!(b.fx.files.exists("/www/webstore/conf/httpd.conf"));
    assert_eq!(b.get("/instance/WEBSTORE/general").status, 404);
}

/// Every page of the product, the confirmations too, carries its
/// wordmark.
#[test]
fn every_page_carries_the_product_title() {
    let mut b = Browser::signed_on();
    let mark = format!("<td class=\"wordmark\">{}</td>", PRODUCT.wordmark);
    let mut queue = VecDeque::from([
        PRODUCT.url("/instances"),
        PRODUCT.url("/instance/WEBSTORE/confirm?op=delete"),
        PRODUCT.url("/instance/INTRANET/confirm?op=stop"),
        PRODUCT.url("/msg/HTP8103?d=WEBSTORE"),
        PRODUCT.url("/joblog"),
        PRODUCT.url("/nothing"),
    ]);
    let mut seen = BTreeSet::new();
    let mut framed = 0;
    while let Some(url) = queue.pop_front() {
        if !seen.insert(url.clone()) || url.ends_with("/signoff") || url.contains("/help/") || url == qsweb::frame::CSS_PATH {
            continue;
        }
        let r = b.get(&url);
        let t = r.text();
        if r.status == 303 || !t.contains("<html") {
            continue;
        }
        assert!(t.contains(&mark), "{url}: no product title");
        framed += 1;
        for (method, href) in page_links(&t) {
            if method == "GET" && href.starts_with(PRODUCT.prefix) {
                queue.push_back(href);
            }
        }
    }
    assert!(framed > 20, "{framed}");
}

/// The message page's buttons say which was pressed: Job log shows
/// the administration server's job log, Back returns, Close leaves.
#[test]
fn message_page_job_log() {
    let mut b = Browser::signed_on();
    let url = "/msg/HTP8106?d=QSTEST&d=denied&back=%2Fqs400%2Fhttpadmin%2Finstance%2FWEBSTORE%2Fgeneral";
    let t = b.get(url).text();
    assert!(t.contains("name=\"joblog\" value=\"Job log\"") && t.contains("name=\"back\""), "{t}");
    let r = b.post(url, &[("joblog", "Job log")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/joblog"));
    let t = b.get("/joblog").text();
    assert!(t.contains("Job log: 123457/QTMHHTTP/ADMIN") && t.contains("HTP8001"), "{t}");
    let r = b.post(url, &[("back", "< Back")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instance/WEBSTORE/general"));
    let r = b.post(url, &[("close", "Close")]);
    assert_eq!(r.header("Location"), Some("/qs400/httpadmin/instances"));
}
