// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A browser over the handler (no sockets), the map from served URLs to
//! the prototype's files, and the golden comparison.

#![allow(dead_code)]

use navigator::fixture::{Fixture, PASSWORD, fixture};
use navigator::{Navigator, PRODUCT};
use qsweb::golden::{Normalize, diff, links};
use qsweb::http::{Handler, Peer, Request, Response};

/// A signed-on browser over the fixture.
pub struct Browser {
    /// The product.
    pub nav: Navigator,
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
            nav: Navigator::new(fx.env.clone()),
            fx,
            cookie: None,
            csrf: String::new(),
        }
    }

    /// Signed on as `QSECOFR`.
    pub fn signed_on() -> Self {
        Self::as_user("QSECOFR")
    }

    /// Signed on as `user`.
    pub fn as_user(user: &str) -> Self {
        let mut b = Self::new();
        let r = b.sign_on(user, PASSWORD);
        assert_eq!(r.status, 303, "{}", r.text());
        b
    }

    /// Send a request as this browser.
    pub fn send(&mut self, mut req: Request) -> Response {
        if let Some(c) = &self.cookie {
            req.headers.push(("cookie".into(), format!("QS400SID={c}")));
        }
        let r = self.nav.handle(&req, &peer());
        if let Some(sc) = r.header("Set-Cookie") {
            let v = sc
                .split(';')
                .next()
                .unwrap_or("")
                .trim_start_matches("QS400SID=")
                .to_string();
            self.cookie = (!v.is_empty()).then_some(v);
        }
        if let Some(c) = &self.cookie
            && let Some(s) = self.nav.sessions.get(c)
        {
            self.csrf = s.csrf;
        }
        r
    }

    /// `GET` a route under the prefix (or an absolute path).
    pub fn get(&mut self, route: &str) -> Response {
        let path = if route.starts_with("/qs400/") {
            route.to_string()
        } else {
            PRODUCT.url(route)
        };
        self.send(Request::new("GET", &path))
    }

    /// `POST` fields to a route, with the CSRF token.
    pub fn post(&mut self, route: &str, fields: &[(&str, &str)]) -> Response {
        let tok = self.csrf.clone();
        let mut all: Vec<(&str, &str)> = vec![("csrf", &tok)];
        all.extend_from_slice(fields);
        self.post_raw(route, &all)
    }

    /// `POST` exactly these fields.
    pub fn post_raw(&mut self, route: &str, fields: &[(&str, &str)]) -> Response {
        let path = if route.starts_with("/qs400/") {
            route.to_string()
        } else {
            PRODUCT.url(route)
        };
        let mut req = Request::new("POST", &path);
        req.body = fields
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    qsweb::http::form::encode(k),
                    qsweb::http::form::encode(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&")
            .into_bytes();
        req.headers.push((
            "content-type".into(),
            "application/x-www-form-urlencoded".into(),
        ));
        self.send(req)
    }

    /// Sign on.
    pub fn sign_on(&mut self, user: &str, password: &str) -> Response {
        self.post_raw("/signon", &[("user", user), ("password", password)])
    }
}

/// A golden page's text (`nav/home.html`).
pub fn golden(file: &str) -> String {
    let p = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The prototype file a served URL stands for, as `dir/name.html`.
/// Each area keeps its own routes' entries.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub fn proto(url: &str) -> Option<String> {
    if url == qsweb::frame::CSS_PATH {
        return Some("shared/qs400.css".into());
    }
    // HTTP Server Administration, another product, on port 2001.
    if url.contains("/qs400/httpadmin/instances") {
        return Some("http/instances.html".into());
    }
    let r = url.strip_prefix(PRODUCT.prefix)?;
    let (path, query) = r.split_once('?').unwrap_or((r, ""));
    let q = Request::new("GET", &format!("/x?{query}")).query();
    // The tree's [+] and [-] stand for the group's first page.
    if path == "/nav" {
        let g = q.get("open").or_else(|| q.get("close"))?;
        let first = PRODUCT.nav.iter().find(|n| n.label == g)?.items.first()?.1;
        return proto(&PRODUCT.url(first));
    }
    let segs: Vec<&str> = path.trim_matches('/').split('/').collect();
    let file = match segs.as_slice() {
        ["help" | "msg", ..] => return Some("shared/help.html".into()),
        ["signon" | "signoff"] => "signon".into(),
        ["home"] => "home".into(),
        // Work management.
        ["jobs"] => "active-jobs".into(),
        ["job", _, _, _, "end"] => "end-job-confirm".into(),
        ["job", _, _, _, "hold" | "release"] => "active-jobs".into(),
        ["job", _, _, _, "splf"] => "spooled-files".into(),
        ["job", _, _, _, tab] => format!("job-props-{tab}"),
        ["jobq" | "outq" | "splf" | "writers" | "jobscde", ..]
            if segs.last() == Some(&"confirm") =>
        {
            return Some("shared/confirm.html".into());
        }
        ["jobq", ..] => "job-queues".into(),
        ["sbs"] | ["sbs", _, "start"] => "subsystems".into(),
        ["sbs", _, "general" | "workstation" | "prestart"] => "subsystem-props".into(),
        ["sbs", _, tab @ ("pools" | "routing" | "autostart")] => format!("subsystem-props-{tab}"),
        ["sbs", _, "end"] | ["sbs", _, _, "confirm"] => return Some("shared/confirm.html".into()),
        ["outq"] | ["outq", _, _, "hold" | "release" | "props"] => "output-queues".into(),
        ["outq", _, _] => "spooled-files".into(),
        ["splf", .., "hold" | "release" | "move"] => "spooled-files".into(),
        ["splf", ..] => "spooled-view".into(),
        ["writers", ..] => "printers".into(),
        ["jobscde", ..] => "scheduled-jobs".into(),
        // Messages.
        ["msgq", _, "QSYSOPR"] => "qsysopr".into(),
        ["msgq", _, _] | ["mymsgs"] => "my-messages".into(),
        ["msgq", _, _, "clear"] => return Some("shared/confirm.html".into()),
        ["msgq", _, "QSYSOPR", _, "remove"] => "qsysopr".into(),
        ["msgq", _, _, _, "remove"] => "my-messages".into(),
        ["msgq", _, _, _] => "message-detail".into(),
        ["qhst"] => "history-log".into(),
        // Users and groups.
        ["users", "create", "step", n] => {
            format!("create-user-{}", if *n == "5" { "5" } else { "1" })
        }
        ["users"] => "users".into(),
        ["user", _, "delete" | "confirm"] => return Some("shared/confirm.html".into()),
        ["user", ..] => "user-props".into(),
        ["groups" | "group", ..] => "groups".into(),
        ["autl"] => "authorization-lists".into(),
        ["autl", _, "remove" | "delete"] => return Some("shared/confirm.html".into()),
        ["autl", ..] => "autl-entries".into(),
        // System.
        ["sysval"] if q.get("cat") == Some("security") => "security-values".into(),
        ["sysval"] => "system-values".into(),
        ["sysval", _] => "sysval-change".into(),
        ["disk", ..] => "disk-status".into(),
        ["hdwrsc", ..] if segs.last() == Some(&"confirm") => {
            return Some("shared/confirm.html".into());
        }
        ["hdwrsc", ..] => "hardware-resources".into(),
        ["time"] => "time-management".into(),
        // Power down / IPL has no prototype of its own: a form, then a
        // confirmation.
        ["power"] => return Some("shared/confirm.html".into()),
        // Network.
        ["tcp", ..] if segs.last() == Some(&"confirm") => {
            return Some("shared/confirm.html".into());
        }
        ["tcp"] | ["tcp", "hosts"] => "tcpip".into(),
        ["tcp", "ifc", ..] => "tcpip-interfaces".into(),
        ["tcp", "rte", ..] => "tcpip-routes".into(),
        ["tcp", "servers", ..] => "servers".into(),
        // Database.
        ["sql", ..] => "run-sql".into(),
        ["db"] | ["db", _] => "schemas".into(),
        ["db", _, _, "data"] => "table-data".into(),
        ["db", _, _, ..] => "table-columns".into(),
        // Security, software, performance.
        ["audit", ..] => "audit-journal".into(),
        ["licpgm", ..] if segs.last() == Some(&"confirm") => {
            return Some("shared/confirm.html".into());
        }
        ["licpgm", ..] => "licensed-programs".into(),
        // A cover letter opens in the help window.
        ["ptf" | "fixes", _, "cover"] => return Some("shared/help.html".into()),
        ["ptf", "confirm"] => "ptf-confirm".into(),
        ["ptf", ..] => "ptfs".into(),
        ["fixes", ..] => "fixes".into(),
        ["perf", "sys"] => "system-activity".into(),
        ["perf", "disk"] => "disk-activity".into(),
        _ => return None,
    };
    Some(format!("nav/{file}.html"))
}

/// `target` relative to a page in directory `from` (`nav`).
pub fn relative(target: &str, from: &str) -> String {
    match target.split_once('/') {
        Some((dir, name)) if dir == from => name.to_string(),
        _ => format!("../{target}"),
    }
}

/// Compare a served page with golden file `file` (`nav/home.html`);
/// `fix` edits the golden text first (documented golden corrections).
pub fn assert_golden(served: &str, file: &str, fix: &[(&str, &str)]) {
    assert_golden_text(served, golden(file), file, fix);
}

/// [`assert_golden`] over golden text `g` already read (and cut) from
/// `file`.
pub fn assert_golden_text(served: &str, mut g: String, file: &str, fix: &[(&str, &str)]) {
    for (from, to) in fix {
        assert!(
            g.contains(from),
            "golden fix not applicable to {file}: {from}"
        );
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
