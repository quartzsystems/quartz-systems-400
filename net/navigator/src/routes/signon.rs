// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign on and Sign off. The system checks the password with the 5250
//! rules (and writes `PW` entries); any profile it accepts signs on, and
//! each page then checks what the command behind it would.

use qsweb::frame;
use qsweb::http::{Peer, Request, Response};
use qsweb::msg::Message;
use qsweb::session;

use crate::{Navigator, PRODUCT};

fn page(nav: &Navigator, user: &str, messages: &[Message]) -> Response {
    Response::html(200, frame::signon(&PRODUCT, &nav.env.system.name(), user, messages, None))
}

/// `GET /signon` shows the page; `POST /signon` signs on.
pub fn signon(nav: &Navigator, req: &Request, peer: &Peer) -> Response {
    if req.method != "POST" {
        return page(nav, "", &[]);
    }
    let form = req.form();
    let typed = form.str("user").trim().to_ascii_uppercase();
    let user = match nav.env.system.sign_on(&typed, form.str("password"), &peer.addr.ip().to_string()) {
        Ok(u) => u,
        Err(m) => return page(nav, &typed, &[m]),
    };
    let sess = nav.sessions.create(&user);
    Response::redirect(&PRODUCT.url("/home")).with("Set-Cookie", &session::set_cookie(&sess.id, PRODUCT.prefix, peer.secure))
}

/// `/signoff`: end the session, back to the sign-on page.
pub fn signoff(nav: &Navigator, req: &Request, _peer: &Peer) -> Response {
    if let Some(id) = req.cookie(session::COOKIE) {
        nav.sessions.remove(id);
    }
    Response::redirect(&PRODUCT.url("/signon")).with("Set-Cookie", &session::clear_cookie(PRODUCT.prefix))
}
