// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routing: every URL under `/qs400/accessweb/`, the session and CSRF
//! gate in front of them, and [`Rq`], what a page handler works with.
//! URLs mirror the prototype's file names; each area routes its own
//! first path segments in its own folder.

pub mod cmd;
pub mod customize;
pub mod db;
pub mod download;
pub mod files;
pub mod home;
pub mod jobs;
pub mod msgs;
pub mod print;
pub mod s5250;
pub mod shared;
pub mod signon;

use qsweb::frame::{Ctx, Page, render, render_with};
use qsweb::html::esc;
use qsweb::http::{Form, Peer, Request, Response};
use qsweb::msg::{Kind, Message};
use qsweb::pages::{Refusal, not_authorized};
use qsweb::session::{self, Session};

use crate::env::{Audit, Env, Need};
use crate::policy::{self, SetBy};
use crate::prefs::Prefs;
use crate::{AccessWeb, PRODUCT};

/// The session value that marks a sign-on with an expired password:
/// only Change password is open to it.
pub const EXPIRED: &str = "expired";

/// One request, signed on.
pub struct Rq<'a> {
    /// The product.
    pub app: &'a AccessWeb,
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
        &self.app.env
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

    /// The user's preferences.
    pub fn prefs(&self) -> Prefs {
        Prefs::load(self.env(), self.user())
    }

    /// A page in the frame, the waiting message first.
    pub fn page(&self, page: Page) -> Response {
        self.page_status(200, page)
    }

    /// [`Rq::page`] with another status.
    pub fn page_status(&self, status: u16, mut page: Page) -> Response {
        if let Some(m) = self.app.sessions.take_flash(&self.session.id) {
            page.messages.insert(0, m);
        }
        let mut ctx = self.ctx.clone();
        if let Some((group, _)) = page.nav {
            ctx.open = Some(session::nav_visit(&self.app.sessions, &self.session.id, group));
        }
        let note = msgs::watch::note(self);
        Response::html(status, render_with(&ctx, &page, note.as_deref()))
    }

    /// Leave `m` for the next page.
    pub fn flash(&self, m: Message) {
        self.app.sessions.flash(&self.session.id, m);
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
        self.app.sessions.get(&self.session.id).map(|s| s.get(key).to_string()).unwrap_or_default()
    }

    /// Set session values.
    pub fn set(&self, pairs: &[(&str, &str)]) {
        self.app.sessions.update(&self.session.id, |s| {
            for (k, v) in pairs {
                s.data.insert((*k).to_string(), (*v).to_string());
            }
        });
    }

    /// Whether the user has special authority `need` (`*ALLOBJ` has
    /// every one but `*SECADM`'s policy role).
    pub fn has(&self, need: Need) -> bool {
        let sp = self.env().system.special(self.user());
        sp.iter().any(|s| s == need.name() || s == "*ALLOBJ")
    }

    /// Whether a policy allows the user `function` (to show an action).
    pub fn allowed(&self, function: &str) -> bool {
        policy::allowed(self.env(), self.user(), function)
    }

    /// Check the policy of `function` before running or showing it: on
    /// Deny the Not authorized page with `XWA0001`, and an `AF` entry.
    pub fn policy(&self, function: &str, crumb: &[&str]) -> Result<(), Response> {
        let e = policy::policy(self.env(), self.user(), function);
        if e.allowed() {
            return Ok(());
        }
        let title = policy::title(function);
        self.env().system.audit(Audit::Refused {
            user: self.user().to_string(),
            object: title.clone(),
            need: format!("policy {function}"),
        });
        let level = match &e.by {
            SetBy::User(u) => format!("for user profile <span class=\"mono\">{}</span>", esc(u)),
            SetBy::Group(g) => format!("for group <span class=\"mono\">{}</span>", esc(g)),
            SetBy::Public => "for <span class=\"mono\">*PUBLIC</span>".into(),
            SetBy::System => "by the system: it needs special authority <span class=\"mono\">*SECADM</span>".into(),
            SetBy::Default => String::new(),
        };
        let r = Refusal {
            message: self.message(Kind::Error, crate::msgs::DENIED, &[&title, self.user()]),
            explain: format!(
                "An Access for Web policy denies this function to your user profile <span class=\"mono\">{}</span>.",
                esc(self.user())
            ),
            function: title.clone(),
            object: "Access for Web".into(),
            required: "Allow".into(),
            user: self.user().to_string(),
            policy: Some(format!(
                "Access for Web policy \"{}\" is set to <b>Deny</b> {level}.",
                esc(&title)
            )),
            crumb: crumb.iter().map(ToString::to_string).collect(),
        };
        let mut page = not_authorized(&self.ctx, &r);
        page.generic = false;
        Err(Response::html(403, render(&self.ctx, &page)))
    }

    /// Check special authority `need` for `function` on `object`: on
    /// failure the Not authorized page with `CPF2207`, and an `AF`
    /// entry.
    pub fn require(&self, need: Need, function: &str, object: (&str, &str, &str), crumb: &[&str]) -> Result<(), Response> {
        if self.has(need) {
            return Ok(());
        }
        let (lib, name, typ) = object;
        let sys = &self.env().system;
        sys.audit(Audit::Refused {
            user: self.user().to_string(),
            object: format!("{lib}/{name}"),
            need: need.name().to_string(),
        });
        let class = sys.user_class(self.user());
        let r = Refusal {
            message: self.message(Kind::Error, "CPF2207", &[name, lib, typ]),
            explain: format!(
                "You are not authorized to use this function. Your user profile <span class=\"mono\">{}</span> has user class <span class=\"mono\">{}</span> and does not have the <span class=\"mono\">{}</span> special authority this function requires.",
                esc(self.user()),
                esc(&class),
                need.name()
            ),
            function: function.to_string(),
            object: format!("{lib}/{name}"),
            required: need.name().to_string(),
            user: self.user().to_string(),
            policy: None,
            crumb: crumb.iter().map(ToString::to_string).collect(),
        };
        let mut page = not_authorized(&self.ctx, &r);
        page.generic = false;
        Err(Response::html(403, render(&self.ctx, &page)))
    }

    /// Message `id` as a row, its text from the system's message files
    /// (the product's own table when the system has none).
    pub fn message(&self, kind: Kind, id: &str, data: &[&str]) -> Message {
        shared::message(self.env(), kind, id, data)
    }

    /// The page for a missing object.
    pub fn not_found(&self, what: &str) -> Response {
        shared::not_found(self, what)
    }

    /// `(date, time)` of `secs` as the user's preferences show them.
    pub fn stamp(&self, secs: u64) -> (String, String) {
        self.prefs().when(secs, self.env().system.offset(secs))
    }

    /// `date time` of `secs`.
    pub fn when(&self, secs: u64) -> String {
        let (d, t) = self.stamp(secs);
        format!("{d} {t}")
    }
}

/// The route of `path` under the prefix, if it is under it.
fn route(path: &str) -> Option<&str> {
    let r = path.strip_prefix(PRODUCT.prefix)?;
    if r.is_empty() || r.starts_with('/') { Some(r) } else { None }
}

/// Answer a request.
pub fn dispatch(app: &AccessWeb, req: &Request, peer: &Peer) -> Response {
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
    match r {
        "" | "/" => return Response::redirect(&PRODUCT.url("/signon")),
        "/signon" => return signon::signon(app, req, peer),
        "/signoff" => return signon::signoff(app, req, peer),
        _ => {}
    }
    let session = req.cookie(session::COOKIE).and_then(|id| app.sessions.get(id));
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
        system: app.env.system.name(),
        user: session.user.clone(),
        csrf: session.csrf.clone(),
        open: Some(session::nav_open(&session)),
        here: qsweb::frame::here(req),
    };
    let expired = !session.get(EXPIRED).is_empty();
    let rq = Rq {
        app,
        req,
        peer,
        session,
        ctx,
    };
    let segs: Vec<&str> = r.trim_matches('/').split('/').collect();
    // A message that arrived on a watched queue opens Message Arrived.
    if let Some(r) = msgs::watch::arrived(&rq, &segs) {
        return r;
    }
    match segs.as_slice() {
        ["password"] => customize::password::page(&rq),
        ["help", key] => shared::help(&rq, key),
        ["msg", id] => shared::msg(&rq, id),
        // An expired password opens nothing else.
        _ if expired => rq.redirect("/password"),
        ["home"] => home::page(&rq),
        ["policies", ..] => customize::policies::route(&rq, &segs),
        ["preferences"] => customize::prefs::page(&rq),
        ["5250", ..] => s5250::route(&rq, &segs),
        ["db" | "sql" | "requests", ..] => db::route(&rq, &segs),
        ["files", ..] => files::route(&rq, &segs),
        ["print", ..] => print::route(&rq, &segs),
        ["messages", ..] => msgs::route(&rq, &segs),
        ["jobs", ..] => jobs::route(&rq, &segs),
        ["cmd", ..] => cmd::route(&rq, &segs),
        ["download", ..] => download::route(&rq, &segs),
        ["nav"] => qsweb::frame::nav_toggle(&PRODUCT, &app.sessions, &rq.session.id, req),
        _ => rq.not_found(r),
    }
}
