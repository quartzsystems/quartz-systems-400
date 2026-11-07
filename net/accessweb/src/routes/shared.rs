// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The generic pages: help windows, second-level message text, not
//! found, not available.

use qsweb::frame::{Page, render};
use qsweb::http::Response;
use qsweb::msg::{Kind, Message};
use qsweb::pages::{Sent, second_level};

use super::Rq;
use crate::env::Env;
use crate::{PRODUCT, help, msgs};

/// `/help/<key>`: the help window.
pub fn help(_rq: &Rq<'_>, key: &str) -> Response {
    let (topic, body) = help::topic(key);
    Response::html(200, qsweb::frame::help(&PRODUCT, &topic, &body))
}

/// Message `id` as a row, its first-level text from the system (the
/// product's table when the system has none).
pub fn message(env: &Env, kind: Kind, id: &str, data: &[&str]) -> Message {
    let text = env
        .system
        .message(id, data)
        .or_else(|| msgs::second_level(id, data))
        .map(|s| s.text)
        .unwrap_or_default();
    Message::new(kind, id, &text).with_data(data)
}

/// `/msg/<ID>`: a message's second-level text. `?back=` is where
/// `< Back` returns; the buttons post here.
pub fn msg(rq: &Rq<'_>, id: &str) -> Response {
    let q = rq.query();
    let back = q
        .get("back")
        .filter(|b| b.starts_with(PRODUCT.prefix))
        .map(ToString::to_string);
    if rq.post() {
        let f = rq.form();
        return match back {
            Some(b) if !f.has("Close") => Response::redirect(&b),
            _ => rq.redirect("/home"),
        };
    }
    let data: Vec<String> = q.all("d").into_iter().map(ToString::to_string).collect();
    let data: Vec<&str> = data.iter().map(String::as_str).collect();
    let id = id.to_ascii_uppercase();
    let Some(sl) = rq
        .env()
        .system
        .message(&id, &data)
        .or_else(|| msgs::second_level(&id, &data))
    else {
        return rq.not_found(&format!("message {id}"));
    };
    let mut page = second_level(&rq.ctx, &sl, None::<&Sent>, "Message");
    page.title.clone_from(&sl.id);
    page.h1 = format!("Message {}", sl.id);
    page.desc = None;
    page.generic = false;
    rq.page(page)
}

/// The page for a URL or object that is not there.
pub fn not_found(rq: &Rq<'_>, what: &str) -> Response {
    let page = Page {
        title: "Not found".into(),
        crumb: vec!["Not found".into()],
        h1: "Not found".into(),
        desc: Some(format!("There is no {what} on this system.")),
        body: format!(
            "<p><a href=\"{}\">My home page</a></p>",
            qsweb::html::esc(&PRODUCT.url("/home"))
        ),
        help: "contents".into(),
        ..Page::default()
    };
    Response::html(404, render(&rq.ctx, &page))
}

/// The page of an area whose pages are not part of this level yet:
/// `501` with `XWA0099`, in tree group `group`.
pub fn not_available(rq: &Rq<'_>, area: &str, group: &'static str) -> Response {
    let page = Page {
        title: area.into(),
        crumb: vec![area.into()],
        h1: area.into(),
        desc: Some("This part of Access for Web is not available on this system.".into()),
        messages: vec![rq.message(Kind::Error, msgs::NOT_AVAILABLE, &[area])],
        body: format!(
            "<p><a href=\"{}\">My home page</a></p>",
            qsweb::html::esc(&PRODUCT.url("/home"))
        ),
        help: "notavail".into(),
        nav: Some((group, None)),
        ..Page::default()
    };
    rq.page_status(501, page)
}

/// A bare 404 for paths outside the product.
pub fn plain_not_found() -> Response {
    qsweb::http::plain(404, "There is no such page on Access for Web.")
}
