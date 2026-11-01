// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign on and Sign off. The system checks the password with the 5250
//! rules (and writes `PW` entries); this product then wants `*IOSYSCFG`
//! or membership in the administrators group of Global settings.

use qsweb::frame;
use qsweb::http::{Peer, Request, Response};
use qsweb::msg::{Kind, Message};
use qsweb::session;

use crate::env::Audit;
use crate::settings::Settings;
use crate::{Admin, PRODUCT, msgs};

fn page(admin: &Admin, user: &str, messages: &[Message]) -> Response {
    let html = frame::signon(&PRODUCT, &admin.env.system.name(), user, messages, None);
    Response::html(200, html)
}

/// `GET /signon` shows the page; `POST /signon` signs on.
pub fn signon(admin: &Admin, req: &Request, peer: &Peer) -> Response {
    if req.method != "POST" {
        return page(admin, "", &[]);
    }
    let form = req.form();
    let typed = form.str("user").trim().to_ascii_uppercase();
    let sys = &admin.env.system;
    let user = match sys.sign_on(&typed, form.str("password"), &peer.addr.ip().to_string()) {
        Ok(u) => u,
        Err(m) => return page(admin, &typed, &[m]),
    };
    let s = Settings::load(admin.env.files.as_ref());
    let special = sys.special(&user);
    let ok = special.iter().any(|a| a == "*IOSYSCFG" || a == "*ALLOBJ")
        || (s.admin_group != "*NONE" && sys.groups(&user).iter().any(|g| g.eq_ignore_ascii_case(&s.admin_group)));
    if !ok {
        sys.audit(Audit::Refused {
            user: user.clone(),
            object: "QHTTPSVR/ADMIN".into(),
            need: "*IOSYSCFG".into(),
        });
        let m = msgs::message(Kind::Error, "HTP8111", &[&user, &s.admin_group]);
        return page(admin, &user, &[m]);
    }
    admin.sessions.set_idle(std::time::Duration::from_secs(u64::from(s.session_minutes.max(1)) * 60));
    let sess = admin.sessions.create(&user);
    Response::redirect(&PRODUCT.url("/instances")).with(
        "Set-Cookie",
        &session::set_cookie(&sess.id, PRODUCT.prefix, peer.secure),
    )
}

/// `/signoff`: end the session, back to the sign-on page.
pub fn signoff(admin: &Admin, req: &Request, _peer: &Peer) -> Response {
    if let Some(id) = req.cookie(session::COOKIE) {
        admin.sessions.remove(id);
    }
    Response::redirect(&PRODUCT.url("/signon")).with("Set-Cookie", &session::clear_cookie(PRODUCT.prefix))
}
