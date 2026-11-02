// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routing: every URL under `/qs400/httpadmin/`, the session and CSRF
//! gate in front of them, and [`Rq`], what a page handler works with.
//! URLs mirror the prototype's file names (`instance-ports.html` is
//! `GET /instance/<name>/ports`).

pub mod create;
pub mod instance;
pub mod certs;
pub mod global;
pub mod instances;
pub mod shared;
pub mod signon;

use qsweb::frame::{Ctx, Page, render};
use qsweb::http::{Form, Peer, Request, Response};
use qsweb::msg::{Kind, Message};
use qsweb::pages::{Refusal, not_authorized};
use qsweb::session::{self, Session};

use crate::env::{Audit, Env, Need};
use crate::{Admin, PRODUCT};

/// One request, signed on.
pub struct Rq<'a> {
    /// The product.
    pub admin: &'a Admin,
    /// The request.
    pub req: &'a Request,
    /// The client.
    pub peer: &'a Peer,
    /// The session.
    pub session: Session,
    /// The frame's view.
    pub ctx: Ctx,
}

impl Rq<'_> {
    /// The environment.
    pub fn env(&self) -> &Env {
        &self.admin.env
    }

    /// The signed-on user.
    pub fn user(&self) -> &str {
        &self.session.user
    }

    /// Whether this is a POST.
    pub fn post(&self) -> bool {
        self.req.method == "POST"
    }

    /// The query string.
    pub fn query(&self) -> Form {
        self.req.query()
    }

    /// The posted fields.
    pub fn form(&self) -> Form {
        self.req.form()
    }

    /// A page in the frame, the waiting message first. Every page
    /// carries the product's wordmark, the shared patterns too.
    pub fn page(&self, mut page: Page) -> Response {
        page.generic = false;
        if let Some(m) = self.admin.sessions.take_flash(&self.session.id) {
            page.messages.insert(0, m);
        }
        let mut ctx = self.ctx.clone();
        if let Some((group, _)) = page.nav {
            ctx.open = Some(session::nav_visit(&self.admin.sessions, &self.session.id, group));
        }
        Response::html(200, render(&ctx, &page))
    }

    /// Leave `m` for the next page.
    pub fn flash(&self, m: Message) {
        self.admin.sessions.flash(&self.session.id, m);
    }

    /// `303` to `route` under the prefix.
    pub fn redirect(&self, route: &str) -> Response {
        Response::redirect(&PRODUCT.url(route))
    }

    /// `303` to `route` with `m` waiting there.
    pub fn done(&self, route: &str, m: Message) -> Response {
        self.flash(m);
        self.redirect(route)
    }

    /// A session value.
    pub fn get(&self, key: &str) -> String {
        self.admin.sessions.get(&self.session.id).map(|s| s.get(key).to_string()).unwrap_or_default()
    }

    /// Set session values.
    pub fn set(&self, pairs: &[(&str, &str)]) {
        self.admin.sessions.update(&self.session.id, |s| {
            for (k, v) in pairs {
                s.data.insert((*k).to_string(), (*v).to_string());
            }
        });
    }

    /// Remove the session values starting with `prefix`.
    pub fn clear(&self, prefix: &str) {
        self.admin.sessions.update(&self.session.id, |s| s.data.retain(|k, _| !k.starts_with(prefix)));
    }

    /// Whether the user has special authority `need` (`*ALLOBJ` has
    /// every one).
    pub fn has(&self, need: Need) -> bool {
        let sp = self.env().system.special(self.user());
        sp.iter().any(|s| s == need.name() || s == "*ALLOBJ")
    }

    /// Check `need` for `function` on `object`: on failure the Not
    /// authorized page, and an `AF` audit entry.
    pub fn require(&self, need: Need, function: &str, object: &str, crumb: &[&str]) -> Result<(), Response> {
        if self.has(need) {
            return Ok(());
        }
        Err(self.refuse(need, function, object, crumb))
    }

    /// Check that the user holds every one of `needs` itself (`*ALLOBJ`
    /// stands in for none of them), as the certificate commands do.
    pub fn require_each(&self, needs: &[Need], function: &str, object: &str, crumb: &[&str]) -> Result<(), Response> {
        let sp = self.env().system.special(self.user());
        match needs.iter().find(|n| !sp.iter().any(|s| s == n.name())) {
            Some(n) => Err(self.refuse(*n, function, object, crumb)),
            None => Ok(()),
        }
    }

    /// The Not authorized page for a missing `need`, and its `AF` entry.
    pub fn refuse(&self, need: Need, function: &str, object: &str, crumb: &[&str]) -> Response {
        let sys = &self.env().system;
        sys.audit(Audit::Refused {
            user: self.user().to_string(),
            object: object.to_string(),
            need: need.name().to_string(),
        });
        let class = sys.user_class(self.user());
        let r = Refusal {
            message: crate::msgs::message(Kind::Error, "HTP8110", &[self.user(), need.name()]),
            explain: format!(
                "You are not authorized to use this function. Your user profile <span class=\"mono\">{}</span> has user class <span class=\"mono\">{}</span> and does not have the <span class=\"mono\">{}</span> special authority this function requires.",
                qsweb::html::esc(self.user()),
                qsweb::html::esc(&class),
                need.name()
            ),
            function: function.to_string(),
            object: object.to_string(),
            required: need.name().to_string(),
            user: self.user().to_string(),
            policy: None,
            crumb: crumb.iter().map(ToString::to_string).collect(),
        };
        let mut page = not_authorized(&self.ctx, &r);
        page.generic = false;
        Response::html(403, render(&self.ctx, &page))
    }

    /// The page for a missing object.
    pub fn not_found(&self, what: &str) -> Response {
        shared::not_found(self, what)
    }

    /// `(date, time)` of `secs` as the user sees them.
    pub fn stamp(&self, secs: u64) -> (String, String) {
        self.env().system.stamp(secs)
    }

    /// `MM/DD/YY HH:MM:SS` of `secs`.
    pub fn when(&self, secs: u64) -> String {
        let (d, t) = self.stamp(secs);
        format!("{d} {t}")
    }

    /// Write a `ZC` entry for a change when Global settings ask for it.
    pub fn audit_change(&self, object: &str, what: &str) {
        if crate::settings::Settings::load(self.env().files.as_ref()).audit {
            self.env().system.audit(Audit::Change {
                user: self.user().to_string(),
                object: object.to_string(),
                what: what.to_string(),
            });
        }
    }
}

/// The route of `path` under the prefix, if it is under it.
fn route(path: &str) -> Option<&str> {
    let r = path.strip_prefix(PRODUCT.prefix)?;
    if r.is_empty() || r.starts_with('/') { Some(r) } else { None }
}

/// Answer a request.
pub fn dispatch(admin: &Admin, req: &Request, peer: &Peer) -> Response {
    if req.path == qsweb::frame::CSS_PATH {
        return qsweb::css_response();
    }
    if req.path == "/" || req.path.is_empty() {
        return Response::redirect(&PRODUCT.url("/signon"));
    }
    let Some(r) = route(&req.path) else {
        return shared::plain_not_found();
    };
    if !matches!(req.method.as_str(), "GET" | "HEAD" | "POST") {
        return qsweb::http::plain(405, "Method not allowed.").with("Allow", "GET, HEAD, POST");
    }
    let settings = crate::settings::Settings::load(admin.env.files.as_ref());
    if !settings.allows(peer.addr.ip()) {
        return qsweb::http::plain(403, "Administration is not allowed from this address.");
    }
    if settings.admin_tls && !peer.secure && admin.tls_up() {
        let host = req.header("host").unwrap_or("").rsplit_once(':').map_or(req.header("host").unwrap_or(""), |(h, _)| h);
        let q = if req.query.is_empty() { String::new() } else { format!("?{}", req.query) };
        return Response::redirect(&format!("https://{host}:{}{}{q}", settings.admin_tls_port, req.path));
    }
    match r {
        "" | "/" => return Response::redirect(&PRODUCT.url("/signon")),
        "/signon" => return signon::signon(admin, req, peer),
        "/signoff" => return signon::signoff(admin, req, peer),
        _ => {}
    }
    let session = req.cookie(session::COOKIE).and_then(|id| admin.sessions.get(id));
    let Some(session) = session else {
        return Response::redirect(&PRODUCT.url("/signon"));
    };
    if req.method == "POST" {
        let form = req.form();
        if !session::csrf_ok(&session, form.str(session::CSRF_FIELD)) {
            return qsweb::http::plain(403, "The form is out of date. Go back, reload the page and try again.");
        }
    }
    let ctx = Ctx {
        product: &PRODUCT,
        system: admin.env.system.name(),
        user: session.user.clone(),
        csrf: session.csrf.clone(),
        open: Some(session::nav_open(&session)),
        here: qsweb::frame::here(req),
    };
    let rq = Rq {
        admin,
        req,
        peer,
        session,
        ctx,
    };
    let segs: Vec<&str> = r.trim_matches('/').split('/').collect();
    match segs.as_slice() {
        ["instances"] => instances::list(&rq),
        ["instance", name, rest @ ..] => instance::route(&rq, name, rest),
        ["create", "step", n] => create::step(&rq, n),
        ["certs", rest @ ..] => certs::route(&rq, rest),
        ["global"] => global::page(&rq),
        ["nav"] => qsweb::frame::nav_toggle(&PRODUCT, &admin.sessions, &rq.session.id, req),
        ["help", key] => shared::help(&rq, key),
        ["msg", id] => shared::msg(&rq, id),
        ["joblog"] => shared::job_log(&rq),
        _ => rq.not_found(r),
    }
}
