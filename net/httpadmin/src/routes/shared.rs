// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The generic pages: help windows, second-level message text, not
//! found.

use qsweb::frame::{Page, render};
use qsweb::http::Response;
use qsweb::msg::SecondLevel;
use qsweb::pages::{Sent, second_level};

use super::Rq;
use crate::{PRODUCT, help, msgs};

/// `/help/<key>`: the help window.
pub fn help(_rq: &Rq<'_>, key: &str) -> Response {
    let (topic, body) = help::topic(key);
    Response::html(200, qsweb::frame::help(&PRODUCT, &topic, &body))
}

/// The text of `id` from the system, else this product's table.
pub fn text(rq: &Rq<'_>, id: &str, data: &[&str]) -> Option<SecondLevel> {
    rq.env().system.message(id, data).or_else(|| msgs::second_level(id, data))
}

/// `/msg/<ID>`: a message's second-level text. `?back=` is where
/// `< Back` returns; the buttons post here.
pub fn msg(rq: &Rq<'_>, id: &str) -> Response {
    let q = rq.query();
    let back = q.get("back").filter(|b| b.starts_with(PRODUCT.prefix)).map(ToString::to_string);
    if rq.post() {
        let f = rq.form();
        return match back {
            Some(b) if !f.has("Close") => Response::redirect(&b),
            _ => rq.redirect("/instances"),
        };
    }
    let data: Vec<String> = q.all("d").into_iter().map(ToString::to_string).collect();
    let data: Vec<&str> = data.iter().map(String::as_str).collect();
    let Some(sl) = text(rq, &id.to_ascii_uppercase(), &data) else {
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
            "<p><a href=\"{}\">Server instances</a></p>",
            qsweb::html::esc(&PRODUCT.url("/instances"))
        ),
        help: "contents".into(),
        ..Page::default()
    };
    Response::html(404, render(&rq.ctx, &page))
}

/// A bare 404 for paths outside the product.
pub fn plain_not_found() -> Response {
    qsweb::http::plain(404, "There is no such page on the administration server.")
}
