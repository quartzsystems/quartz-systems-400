// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign on and Sign off. The system checks the password with the 5250
//! rules (and writes `PW` entries); any profile it accepts signs on
//! unless policy "Sign on to Access for Web" denies it. A password in
//! its warning period leaves `QAD4031` for the first page; an expired
//! one opens Change password only.

use qsweb::frame;
use qsweb::http::{Peer, Request, Response};
use qsweb::msg::{Kind, Message};
use qsweb::session;

use super::{EXPIRED, shared};
use crate::env::Audit;
use crate::prefs::{Prefs, SIGNON_FROM};
use crate::{AccessWeb, PRODUCT, msgs, policy};

/// Session values sign-on leaves: the previous sign-on's time and
/// address, the days left on the password.
pub const PREV_AT: &str = "prev.at";
/// The previous sign-on's address.
pub const PREV_FROM: &str = "prev.from";
/// Days until the password expires, when the system warns.
pub const PWD_WARN: &str = "pwd.warn";

fn page(app: &AccessWeb, user: &str, messages: &[Message]) -> Response {
    Response::html(
        200,
        frame::signon(&PRODUCT, &app.env.system.name(), user, messages, None),
    )
}

/// `GET /signon` shows the page (`?changed=USER` after an expired
/// password was changed); `POST /signon` signs on.
pub fn signon(app: &AccessWeb, req: &Request, peer: &Peer) -> Response {
    let env = &app.env;
    if req.method != "POST" {
        let q = req.query();
        let shown: Vec<Message> = q
            .get("changed")
            .map(|u| u.trim().to_ascii_uppercase())
            .filter(|u| {
                !u.is_empty()
                    && u.len() <= 10
                    && u.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "#$@_".contains(c))
            })
            .map(|u| {
                shared::message(
                    env,
                    Kind::Done,
                    msgs::PASSWORD_CHANGED,
                    &[&u, &env.system.name()],
                )
            })
            .into_iter()
            .collect();
        return page(app, "", &shown);
    }
    let form = req.form();
    let typed = form.str("user").trim().to_ascii_uppercase();
    let from = peer.addr.ip().to_string();
    let s = match env.system.sign_on(&typed, form.str("password"), &from) {
        Ok(s) => s,
        Err(m) => return page(app, &typed, &[m]),
    };
    let user = s.user.clone();
    if !s.expired && !policy::allowed(env, &user, policy::SIGNON) {
        env.system.sign_off(&user);
        let title = policy::title(policy::SIGNON);
        env.system.audit(Audit::Refused {
            user: user.clone(),
            object: title.clone(),
            need: format!("policy {}", policy::SIGNON),
        });
        let m = shared::message(env, Kind::Error, msgs::DENIED, &[&title, &user]);
        return page(app, &typed, &[m]);
    }
    let sess = app.sessions.create(&user);
    let mut prefs = Prefs::load(env, &user);
    let prev_from = prefs.get(SIGNON_FROM);
    let at = s.previous.to_string();
    let warn = s
        .expires_in
        .map(|d| d.max(0).to_string())
        .unwrap_or_default();
    app.sessions.update(&sess.id, |x| {
        x.data.insert(PREV_AT.into(), at);
        x.data.insert(PREV_FROM.into(), prev_from);
        x.data.insert(PWD_WARN.into(), warn.clone());
        if s.expired {
            x.data.insert(EXPIRED.into(), "1".into());
        }
    });
    let system = env.system.name();
    let target = if s.expired {
        app.sessions.flash(
            &sess.id,
            shared::message(env, Kind::Error, msgs::PASSWORD_EXPIRED, &[&user, &system]),
        );
        "/password"
    } else {
        prefs.stored.insert(SIGNON_FROM.into(), from);
        let _ = env.prefs.put(&user, &prefs.to_store());
        if !warn.is_empty() {
            app.sessions.flash(
                &sess.id,
                shared::message(
                    env,
                    Kind::Info,
                    msgs::PASSWORD_EXPIRES,
                    &[&user, &system, &warn],
                ),
            );
        }
        "/home"
    };
    Response::redirect(&PRODUCT.url(target)).with(
        "Set-Cookie",
        &session::set_cookie(&sess.id, PRODUCT.prefix, peer.secure),
    )
}

/// `/signoff`: end the session, back to the sign-on page.
pub fn signoff(app: &AccessWeb, req: &Request, _peer: &Peer) -> Response {
    if let Some(id) = req.cookie(session::COOKIE) {
        if let Some(s) = app.sessions.get(id) {
            app.env.system.sign_off(&s.user);
        }
        app.sessions.remove(id);
    }
    Response::redirect(&PRODUCT.url("/signon"))
        .with("Set-Cookie", &session::clear_cookie(PRODUCT.prefix))
}
