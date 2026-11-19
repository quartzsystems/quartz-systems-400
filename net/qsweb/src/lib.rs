// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The layer a family of web products shares: `docs/WEB_STYLE.md` is
//! the standard. The family is the caller's ([`brand::Brand`]): its
//! name, release, paths and cookie; each product brings its own name,
//! routes, navigation tree, messages and help ([`frame::Product`]).
//!
//! - [`brand`]: what the family supplies.
//! - [`http`]: HTTP/1.1 with keep-alive, forms, uploads, responses.
//! - [`session`]: the session table, CSRF tokens, one-shot messages.
//! - [`frame`]: the page frame, the sign-on page, the help window.
//! - [`partials`]: message rows, lists, paging, properties, wizards.
//! - [`pages`]: second-level text, Not authorized, confirmations.
//! - [`golden`]: a served page against its golden rendering; links.
//! - [`ws`]: WebSocket, for the one page with script (the 5250 display).
//!
//! Server-rendered HTML only: no script, one stylesheet ([`CSS`]).

pub mod brand;
pub mod frame;
pub mod golden;
pub mod html;
pub mod http;
pub mod msg;
pub mod pages;
pub mod partials;
pub mod session;
pub mod ws;

/// The stylesheet, byte for byte, served at [`brand::Brand::css_path`].
pub const CSS: &[u8] = include_bytes!("../assets/qs400.css");

/// In-place navigation for a product that turns it on, served at
/// [`brand::Brand::nav_path`].
pub const NAV_JS: &[u8] = include_bytes!("../assets/qsnav.js");

/// The navigation script as a response.
pub fn nav_response() -> http::Response {
    http::Response::new(200, "text/javascript; charset=utf-8", NAV_JS.to_vec())
        .with("Cache-Control", "max-age=3600")
}

/// The stylesheet as a response.
pub fn css_response() -> http::Response {
    http::Response::new(200, "text/css", CSS.to_vec()).with("Cache-Control", "max-age=3600")
}
