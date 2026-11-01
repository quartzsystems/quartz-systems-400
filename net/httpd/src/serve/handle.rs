// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One request through the configuration: its site, the URL checked,
//! `Redirect`, `ScriptAlias` and `Alias` mapping, the sections that
//! apply, access by client address, Basic authentication, then the
//! file, the directory index or the listing. Error answers go through
//! `ErrorDocument`.

use std::net::SocketAddr;

use qsweb::http::{Request, Response};

use super::auth::{self, Gate};
use super::dirs::{self, DirConf};
use super::log::Level;
use super::site::{Site, redirect};
use super::{Server, statics};
use crate::conf::Section;

/// Where a request came in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctx {
    /// The server's end.
    pub local: SocketAddr,
    /// The client's end.
    pub peer: SocketAddr,
    /// TLS.
    pub secure: bool,
}

/// What became of a request, for the access log.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    /// The signed-on user.
    pub user: Option<String>,
    /// The file served.
    pub file: String,
}

/// The URL path cleaned: `//` and `/./` folded; `None` when it is not
/// absolute, holds a NUL or climbs with `..`.
pub fn clean(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.contains('\0') || path.contains('\\') {
        return None;
    }
    let mut out = String::new();
    for seg in path.split('/').filter(|s| !s.is_empty() && *s != ".") {
        if seg == ".." {
            return None;
        }
        out.push('/');
        out.push_str(seg);
    }
    if out.is_empty() || path.ends_with('/') {
        out.push('/');
    }
    Some(out)
}

/// `url` under a mapping prefix, the rest of it.
fn strip<'a>(url: &'a str, prefix: &str) -> Option<&'a str> {
    if prefix.ends_with('/') {
        url.strip_prefix(prefix).map(|_| &url[prefix.len() - 1..])
    } else if url == prefix {
        Some("")
    } else {
        url.strip_prefix(prefix).filter(|r| r.starts_with('/'))
    }
}

fn join(base: &str, rest: &str) -> String {
    let b = base.trim_end_matches('/');
    if rest.is_empty() {
        if b.is_empty() { "/".into() } else { b.to_string() }
    } else {
        format!("{b}{rest}")
    }
}

/// The file path a URL maps to, and whether it is under a
/// `ScriptAlias`.
pub fn map(site: &Site, url: &str) -> (String, bool) {
    for (p, t) in &site.script_alias {
        if let Some(rest) = strip(url, p) {
            return (join(t, rest), true);
        }
    }
    for (p, t) in &site.alias {
        if let Some(rest) = strip(url, p) {
            return (join(t, rest), false);
        }
    }
    (join(&site.doc_root, url), false)
}

/// The settings for a request.
pub fn settings(site: &Site, fs: &str, url: &str, method: &str) -> DirConf {
    let mut d = site.base.clone();
    let secs: Vec<&Section> = site.sections.iter().collect();
    for s in dirs::matching(&secs, Some(fs), url) {
        d.apply_section(s, method);
    }
    d
}

fn page(status: u16, title: &str, text: &str) -> Response {
    let body = format!(
        "<!DOCTYPE html>\n<html>\n<head><title>{status} {title}</title></head>\n<body>\n<h1>{title}</h1>\n<p>{text}</p>\n</body>\n</html>\n"
    );
    Response::new(status, "text/html; charset=utf-8", body.into_bytes())
}

/// The server's own page for an error status.
pub fn default_error(status: u16) -> Response {
    let (title, text) = match status {
        400 => ("Bad request", "The request could not be understood."),
        401 => ("Sign-on required", "This page needs a user and password."),
        403 => ("Forbidden", "You are not allowed to see this page."),
        404 => ("Not found", "The page you asked for is not on this server."),
        405 => ("Method not allowed", "This page cannot be used that way."),
        410 => ("Gone", "The page you asked for has been removed."),
        416 => ("Range not satisfiable", "The part of the file asked for is not there."),
        501 => ("Not implemented", "This server cannot do that yet."),
        503 => ("Unavailable", "The server is ending; try again later."),
        _ => ("Server error", "The server could not answer the request."),
    };
    page(status, title, text)
}

impl Server {
    /// An error status for a request, through `ErrorDocument`.
    pub(super) fn error_page(&self, status: u16, site: &Site, d: &DirConf, ctx: &Ctx, depth: u8) -> Response {
        let Some(doc) = d.error_doc(status) else {
            return default_error(status);
        };
        if doc.contains("://") {
            return Response::redirect(doc).with_status(302);
        }
        if doc.starts_with('/') {
            if depth > 0 {
                return default_error(status);
            }
            let inner = self.route(&Request::new("GET", doc), site, ctx, 1).0;
            return if inner.status == 200 { inner.with_status(status) } else { default_error(status) };
        }
        Response::new(status, "text/html; charset=utf-8", doc.trim_matches('"').as_bytes().to_vec())
    }

    /// Answer `req` on `site`.
    pub(super) fn route(&self, req: &Request, site: &Site, ctx: &Ctx, depth: u8) -> (Response, Outcome) {
        let mut out = Outcome::default();
        let client = ctx.peer.ip().to_string();
        let Some(url) = clean(&req.path) else {
            self.log_error(site, Level::Error, Some(&client), &format!("Invalid URL path {}", req.path));
            return (default_error(400), out);
        };
        let method = req.method.as_str();
        let (fs, script) = map(site, &url);
        let d = settings(site, &fs, &url, method);
        if let Some((r, to)) = redirect(&d, &url) {
            let resp = match to {
                Some(t) => {
                    let q = if req.query.is_empty() { String::new() } else { format!("?{}", req.query) };
                    Response::redirect(&format!("{t}{q}")).with_status(r.status)
                }
                None => self.error_page(r.status, site, &d, ctx, depth),
            };
            return (resp, out);
        }
        if !d.access.allows(ctx.peer.ip()) {
            self.log_error(site, Level::Error, Some(&client), &format!("Client denied by the configuration: {fs}"));
            return (self.error_page(403, site, &d, ctx, depth), out);
        }
        match auth::gate(&d, req.header("authorization"), self.deps.auth.as_ref()) {
            Gate::Open(u) => out.user = u,
            Gate::Challenge(realm) => {
                if req.header("authorization").is_some() {
                    self.log_error(site, Level::Error, Some(&client), &format!("User not accepted for {url}"));
                }
                let r = self.error_page(401, site, &d, ctx, depth);
                let realm = realm.replace('"', "'");
                return (r.with_status(401).with("WWW-Authenticate", &format!("Basic realm=\"{realm}\"")), out);
            }
            Gate::Forbidden(why) => {
                if let Some(w) = why {
                    self.log_error(site, Level::Error, Some(&client), &format!("{url}: {w}"));
                }
                return (self.error_page(403, site, &d, ctx, depth), out);
            }
        }
        if script {
            // TODO(later): CGI programs. Until they run, a ScriptAlias
            // answers 501 and says so in the error log.
            self.log_error(site, Level::Error, Some(&client), &format!("CGI is not supported yet: {fs}"));
            return (self.error_page(501, site, &d, ctx, depth), out);
        }
        if !matches!(method, "GET" | "HEAD" | "POST" | "OPTIONS") {
            return (self.error_page(501, site, &d, ctx, depth).with("Allow", "GET, HEAD, OPTIONS"), out);
        }
        if method == "OPTIONS" {
            return (Response::new(200, "text/plain; charset=utf-8", Vec::new()).with("Allow", "GET, HEAD, OPTIONS"), out);
        }
        let files = self.deps.files.as_ref();
        let Ok(e) = files.stat(&fs) else {
            self.log_error(site, Level::Info, Some(&client), &format!("File does not exist: {fs}"));
            return (self.error_page(404, site, &d, ctx, depth), out);
        };
        if method == "POST" {
            return (self.error_page(405, site, &d, ctx, depth).with("Allow", "GET, HEAD, OPTIONS"), out);
        }
        if !e.dir {
            out.file.clone_from(&fs);
            return (statics::file(files, &fs, &e, req, &d), out);
        }
        if !url.ends_with('/') {
            let q = if req.query.is_empty() { String::new() } else { format!("?{}", req.query) };
            return (Response::redirect(&format!("{url}/{q}")).with_status(301), out);
        }
        for i in &d.index {
            let p = format!("{}/{i}", fs.trim_end_matches('/'));
            if let Ok(ie) = files.stat(&p)
                && !ie.dir
            {
                // The index file's own sections apply too.
                let di = settings(site, &p, &url, method);
                out.file.clone_from(&p);
                return (statics::file(files, &p, &ie, req, &di), out);
            }
        }
        if d.indexes {
            return match files.list(&fs) {
                Ok(list) => (statics::listing(&url, &list), out),
                Err(_) => (self.error_page(403, site, &d, ctx, depth), out),
            };
        }
        self.log_error(site, Level::Error, Some(&client), &format!("Directory index forbidden by Options: {fs}"));
        (self.error_page(403, site, &d, ctx, depth), out)
    }
}

/// A response's status replaced.
trait WithStatus {
    fn with_status(self, s: u16) -> Self;
}

impl WithStatus for Response {
    fn with_status(mut self, s: u16) -> Self {
        self.status = s;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_and_strips() {
        assert_eq!(clean("/a//b/./c/").as_deref(), Some("/a/b/c/"));
        assert_eq!(clean("/").as_deref(), Some("/"));
        assert_eq!(clean("/a/../b"), None);
        assert_eq!(clean("a"), None);
        assert_eq!(strip("/icons/x.png", "/icons/"), Some("/x.png"));
        assert_eq!(strip("/icons", "/icons/"), None);
        assert_eq!(strip("/docs", "/docs"), Some(""));
        assert_eq!(strip("/docsx", "/docs"), None);
        assert_eq!(join("/www/a/icons/", "/x.png"), "/www/a/icons/x.png");
    }
}
