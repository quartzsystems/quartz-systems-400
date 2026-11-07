// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Change password: the user's own, through the system's password
//! change (`QSYCHGPW`), which checks the rules and answers with its own
//! message. After an expired password the session ends and the user
//! signs on with the new one.

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, mono, props};

use super::GROUP;
use crate::routes::signon::PWD_WARN;
use crate::routes::{EXPIRED, Rq};
use crate::{PRODUCT, msgs};

fn show(rq: &Rq<'_>, messages: Vec<Message>) -> Response {
    let mut messages = messages;
    let warn = rq.get(PWD_WARN);
    if !warn.is_empty() {
        let system = rq.env().system.name();
        messages.push(rq.message(Kind::Info, msgs::PASSWORD_EXPIRES, &[rq.user(), &system, &warn]));
    }
    let pw = |n: &str| format!("<input type=\"password\" name=\"{n}\" size=\"30\">");
    let rules = format!(
        "<span class=\"small grey\">{}; the system checks and answers with its own message (CPF2234 and the like).</span>",
        esc(&rq.env().system.password_rules())
    );
    let body = props(
        &rq.ctx,
        &[PropGroup {
            title: None,
            rows: vec![
                Prop::new("User profile", mono(rq.user())),
                Prop::new("Current password", pw("p0")),
                Prop::new("New password", pw("p1")),
                Prop::new("Confirm new password", pw("p2")),
                Prop::new("Rules", rules),
            ],
        }],
        &[Button::new("change", "Change"), Button::new("cancel", "Cancel")],
    );
    rq.page(Page {
        title: "Change password".into(),
        crumb: vec!["Change password".into()],
        h1: "Change password".into(),
        messages,
        body,
        help: "password".into(),
        nav: Some((GROUP, Some("/password"))),
        ..Page::default()
    })
}

/// `GET /password` shows the page; `POST` changes the password.
pub fn page(rq: &Rq<'_>) -> Response {
    if !rq.post() {
        return show(rq, Vec::new());
    }
    let expired = !rq.get(EXPIRED).is_empty();
    let f = rq.form();
    if !f.has("change") {
        if expired {
            return rq.redirect("/signoff");
        }
        return rq.redirect("/home");
    }
    let (current, new, again) = (f.str("p0"), f.str("p1"), f.str("p2"));
    if new != again {
        return show(rq, vec![rq.message(Kind::Error, msgs::PASSWORD_MISMATCH, &[])]);
    }
    let sys = &rq.env().system;
    if let Err(m) = sys.change_password(rq.user(), current, new) {
        return show(rq, vec![m]);
    }
    if expired {
        rq.app.sessions.remove(&rq.session.id);
        let to = format!("{}?changed={}", PRODUCT.url("/signon"), rq.user());
        return Response::redirect(&to).with("Set-Cookie", &qsweb::session::clear_cookie(PRODUCT.prefix));
    }
    rq.set(&[(PWD_WARN, "")]);
    let m = rq.message(Kind::Done, msgs::PASSWORD_CHANGED, &[rq.user(), &sys.name()]);
    rq.done("/home", m)
}
