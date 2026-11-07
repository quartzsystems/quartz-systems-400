// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A browser over the handler (no sockets), the map from served URLs to
//! the prototype's files, and the golden comparison.

#![allow(dead_code)]

use accessweb::fixture::{Fixture, PASSWORD, USER, fixture};
use accessweb::{AccessWeb, PRODUCT};
use qsweb::golden::{Normalize, diff, links};
use qsweb::http::{Handler, Peer, Request, Response};

/// A signed-on browser over the fixture.
pub struct Browser {
    /// The product.
    pub app: AccessWeb,
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
            app: AccessWeb::new(fx.env.clone()),
            fx,
            cookie: None,
            csrf: String::new(),
        }
    }

    /// Signed on as `CWELLMAN`, the sign-on's message seen.
    pub fn signed_on() -> Self {
        let mut b = Self::as_user(USER);
        b.get("/home");
        b
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
        let r = self.app.handle(&req, &peer());
        if let Some(sc) = r.header("Set-Cookie") {
            let v = sc.split(';').next().unwrap_or("").trim_start_matches("QS400SID=").to_string();
            self.cookie = (!v.is_empty()).then_some(v);
        }
        if let Some(c) = &self.cookie
            && let Some(s) = self.app.sessions.get(c)
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
        let tok = self.csrf.clone();
        let mut all: Vec<(&str, &str)> = vec![("csrf", &tok)];
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

/// A golden page's text (`web/home.html`).
pub fn golden(file: &str) -> String {
    let p = format!("{}/tests/golden/{file}", env!("CARGO_MANIFEST_DIR"));
    // A checkout with CRLF line ends reads as the LF the fixes name.
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}")).replace("\r\n", "\n")
}

/// The prototype file a served URL stands for, as `dir/name.html`.
/// Each area keeps its own routes' entries.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
pub fn proto(url: &str) -> Option<String> {
    if url == qsweb::frame::CSS_PATH {
        return Some("shared/qs400.css".into());
    }
    // Pages of the other products this one links to.
    if let Some(r) = url.split_once("/qs400/navigator/").map(|x| x.1) {
        let segs: Vec<&str> = r.split('?').next().unwrap_or("").trim_matches('/').split('/').collect();
        let f = match segs.as_slice() {
            ["jobs"] => "active-jobs",
            ["job", _, _, _, "end"] => "end-job-confirm",
            ["job", _, _, _, "joblog"] => "job-props-joblog",
            ["job", _, _, _, _] => "job-props-general",
            ["msgq", _, _, _] => "message-detail",
            ["splf", ..] => "spooled-view",
            ["db", _, _, "columns"] => "table-columns",
            _ => return None,
        };
        return Some(format!("nav/{f}.html"));
    }
    if url.contains("/qs400/httpadmin/instance/") {
        return Some("http/instance-general.html".into());
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
    if segs.last() == Some(&"confirm") {
        return Some("shared/confirm.html".into());
    }
    let file = match segs.as_slice() {
        ["help" | "msg", ..] => return Some("shared/help.html".into()),
        ["signon" | "signoff"] => "signon".into(),
        // Group 1: sign-on, policies, home, preferences.
        ["home"] => "home".into(),
        ["password"] => "change-password".into(),
        ["policies", ..] => "policies".into(),
        ["preferences"] => "preferences".into(),
        // 5250.
        ["5250"] => "5250-config".into(),
        ["5250", "verify"] => "5250-verify".into(),
        ["5250", "script.js"] => return Some("web/5250-session.js".into()),
        ["5250", "session", _, "screen.png"] if q.get("scale") == Some("2") => return Some("shared/img/5250-24x80-x2.png".into()),
        ["5250", "session", _, "screen.png"] => return Some("shared/img/5250-24x80.png".into()),
        ["5250", "ws", _] => "5250-session".into(),
        ["5250", _, "download"] => "5250-config".into(),
        ["5250", _, "keymap", _] => "5250-keymap".into(),
        ["5250", _, "macros", ..] => "5250-macros".into(),
        ["5250", "session", _] if q.get("scale") == Some("2") => "5250-session-x2".into(),
        ["5250", "session", ..] => "5250-session".into(),
        ["5250", _, tab @ ("properties" | "keymap")] => format!("5250-{tab}"),
        // Database.
        ["db", "tables"] => "tables".into(),
        ["db", "copy", "to"] => "copy-import".into(),
        ["db", "copy", "from"] => "copy-export".into(),
        ["db", _, _] => "table-view".into(),
        ["db", _, _, "insert" | "edit"] => "table-insert".into(),
        ["sql"] => "run-sql".into(),
        ["sql", "result", _] => "run-sql-result-2".into(),
        ["requests", ..] => "my-requests".into(),
        // Files.
        ["files"] => "files".into(),
        ["files", "props"] => "file-props".into(),
        ["files", "shares"] => "my-files".into(),
        ["files", "edit"] => "file-edit".into(),
        ["files", "download" | "open" | "rename" | "mkdir" | "cut" | "copy" | "queue"] => "files".into(),
        // Print.
        ["print"] => "printer-output".into(),
        ["print", "props", _] => "splf-props".into(),
        ["print", "printers"] => "printers".into(),
        ["print", "view", ..] => return Some("nav/spooled-view.html".into()),
        ["print", "act"] => "printer-output".into(),
        // Messages.
        ["messages"] => "messages".into(),
        ["messages", "arrived"] => "message-arrived".into(),
        ["messages", "send"] => "send-message".into(),
        // Jobs.
        ["jobs"] => "my-jobs".into(),
        ["jobs", "server"] => "server-jobs".into(),
        // Commands and download.
        ["cmd"] => "run-command".into(),
        ["cmd", "prompt"] => "command-prompt".into(),
        ["cmd", "saved"] => "my-commands".into(),
        ["download"] => "download".into(),
        _ => return None,
    };
    Some(format!("web/{file}.html"))
}

/// `target` relative to a page in directory `from` (`web`).
pub fn relative(target: &str, from: &str) -> String {
    match target.split_once('/') {
        Some((dir, name)) if dir == from => name.to_string(),
        _ => format!("../{target}"),
    }
}

/// The header's user: the golden pages show `QSECOFR`, the fixture
/// signs on as `CWELLMAN` (data).
pub const HEADER_USER: (&str, &str) = ("User: <b>QSECOFR</b>", "User: <b>CWELLMAN</b>");

/// Compare a served page with golden file `file` (`web/home.html`);
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
