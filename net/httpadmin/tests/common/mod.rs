// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A browser over the handler (no sockets), the map from served URLs to
//! the prototype's files, and the golden comparison.

#![allow(dead_code)]

use httpadmin::fixture::{Fixture, PASSWORD, fixture};
use httpadmin::{Admin, PRODUCT};
use qsweb::golden::{Normalize, diff, links};
use qsweb::http::{Handler, Peer, Request, Response};

/// A signed-on browser over the fixture.
pub struct Browser {
    /// The product.
    pub admin: Admin,
    /// The fixture's parts.
    pub fx: Fixture,
    /// The session cookie.
    pub cookie: Option<String>,
    /// The CSRF token of the session.
    pub csrf: String,
}

fn peer() -> Peer {
    Peer {
        addr: "10.0.10.55:50000".parse().unwrap(),
        secure: false,
    }
}

impl Browser {
    /// A browser that has not signed on.
    pub fn new() -> Self {
        let fx = fixture();
        Self {
            admin: Admin::new(fx.env.clone()),
            fx,
            cookie: None,
            csrf: String::new(),
        }
    }

    /// Signed on as `QSECOFR`.
    pub fn signed_on() -> Self {
        let mut b = Self::new();
        let r = b.sign_on("QSECOFR", PASSWORD);
        assert_eq!(r.status, 303, "{}", r.text());
        b
    }

    /// Send a request as this browser.
    pub fn send(&mut self, mut req: Request) -> Response {
        if let Some(c) = &self.cookie {
            req.headers.push(("cookie".into(), format!("QS400SID={c}")));
        }
        let r = self.admin.handle(&req, &peer());
        if let Some(sc) = r.header("Set-Cookie") {
            let v = sc.split(';').next().unwrap_or("").trim_start_matches("QS400SID=").to_string();
            self.cookie = (!v.is_empty()).then_some(v);
        }
        if let Some(c) = &self.cookie
            && let Some(s) = self.admin.sessions.get(c)
        {
            self.csrf = s.csrf;
        }
        r
    }

    /// `GET` a route under the prefix (or an absolute path).
    pub fn get(&mut self, route: &str) -> Response {
        let path = if route.starts_with("/qs400/") { route.to_string() } else { PRODUCT.url(route) };
        self.send(Request::new("GET", &path))
    }

    /// `POST` fields to a route, with the CSRF token.
    pub fn post(&mut self, route: &str, fields: &[(&str, &str)]) -> Response {
        let mut all: Vec<(&str, &str)> = vec![("csrf", "")];
        let tok = self.csrf.clone();
        all[0].1 = &tok;
        all.extend_from_slice(fields);
        self.post_raw(route, &all)
    }

    /// `POST` exactly these fields.
    pub fn post_raw(&mut self, route: &str, fields: &[(&str, &str)]) -> Response {
        let path = if route.starts_with("/qs400/") { route.to_string() } else { PRODUCT.url(route) };
        let mut req = Request::new("POST", &path);
        req.body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", qsweb::http::form::encode(k), qsweb::http::form::encode(v)))
            .collect::<Vec<_>>()
            .join("&")
            .into_bytes();
        req.headers.push(("content-type".into(), "application/x-www-form-urlencoded".into()));
        self.send(req)
    }

    /// Sign on.
    pub fn sign_on(&mut self, user: &str, password: &str) -> Response {
        self.post_raw("/signon", &[("user", user), ("password", password)])
    }
}

/// A golden page's text (`http/instances.html`).
pub fn golden(file: &str) -> String {
    let p = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The prototype file a served URL stands for, as `dir/name.html`.
pub fn proto(url: &str) -> Option<String> {
    if url == qsweb::frame::CSS_PATH {
        return Some("shared/qs400.css".into());
    }
    if let Some(r) = url.strip_prefix("/qs400/navigator/") {
        let p = r.split('?').next().unwrap_or("");
        return Some(if p.starts_with("job/") && p.ends_with("/general") {
            "nav/job-props-general.html".into()
        } else if p.starts_with("job/") && p.ends_with("/end") {
            "nav/end-job-confirm.html".into()
        } else {
            "nav/active-jobs.html".into()
        });
    }
    let r = url.strip_prefix(PRODUCT.prefix)?;
    let (path, query) = r.split_once('?').unwrap_or((r, ""));
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    let file = match segs.as_slice() {
        ["help" | "msg", ..] => return Some("shared/help.html".into()),
        ["signon" | "signoff"] => "signon".into(),
        ["instances"] => "instances".into(),
        ["instance", _, "confirm"] => {
            if query.contains("op=delete") {
                return Some("shared/confirm.html".into());
            }
            "confirm-stop".into()
        }
        ["instance", _, "remove"] => return Some("shared/confirm.html".into()),
        ["instance", _, "directory"] => "instance-directory-props".into(),
        ["instance", _, page, ..] => format!("instance-{page}"),
        ["create", "step", n] => format!("create-{n}"),
        ["certs", "stores"] => "cert-stores".into(),
        ["certs", "store", ..] => "cert-store-system".into(),
        ["certs", "create", "step", n] => format!("cert-create-{n}"),
        ["certs", "create", "result"] => "cert-create-result".into(),
        ["certs", "import"] => "cert-import".into(),
        ["certs", "apps", ..] => "cert-apps".into(),
        ["global"] => "global".into(),
        _ => return None,
    };
    Some(format!("http/{file}.html"))
}

/// `target` relative to a page in directory `from` (`http`).
pub fn relative(target: &str, from: &str) -> String {
    match target.split_once('/') {
        Some((dir, name)) if dir == from => name.to_string(),
        _ => format!("../{target}"),
    }
}

/// Compare a served page with golden file `file` (`http/instances.html`);
/// `fix` edits the golden text first (documented golden corrections).
pub fn assert_golden(served: &str, file: &str, fix: &[(&str, &str)]) {
    let mut g = golden(file);
    for (from, to) in fix {
        assert!(g.contains(from), "golden fix not applicable to {file}: {from}");
        g = g.replace(from, to);
    }
    let dir = file.split('/').next().unwrap_or("");
    let map = |u: &str| proto(u).map(|t| relative(&t, dir));
    if let Some(d) = diff(served, &g, &Normalize { url: &map }) {
        panic!("{file} differs from the served page at {d}");
    }
}

/// Every URL on a page, GET or POST.
pub fn page_links(html: &str) -> Vec<(String, String)> {
    links(html)
}
