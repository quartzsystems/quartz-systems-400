// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The layer the QS/400 web products share (HTTP Server Administration,
//! Navigator, Access for Web): `docs/WEB_STYLE.md` is the standard.
//!
//! - [`http`]: HTTP/1.1 with keep-alive, forms, uploads, responses.
//! - [`session`]: the session table, CSRF tokens, one-shot messages.
//! - [`frame`]: the page frame, the sign-on page, the help window.
//! - [`partials`]: message rows, lists, paging, properties, wizards.
//! - [`pages`]: second-level text, Not authorized, confirmations.
//! - [`golden`]: a served page against its golden rendering; links.
//!
//! Server-rendered HTML only: no script, one stylesheet ([`CSS`]).

pub mod frame;
pub mod golden;
pub mod html;
pub mod http;
pub mod msg;
pub mod pages;
pub mod partials;
pub mod session;

/// `shared/qs400.css`, byte for byte.
pub const CSS: &[u8] = include_bytes!("../assets/qs400.css");

/// The stylesheet as a response.
pub fn css_response() -> http::Response {
    http::Response::new(200, "text/css", CSS.to_vec()).with("Cache-Control", "max-age=3600")
}
