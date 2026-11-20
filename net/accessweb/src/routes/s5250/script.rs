// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/script.js`: the terminal page's script (`termlook`'s) after
//! the console font, as the server draws with it, and the keypad's
//! functions.

use std::sync::OnceLock;

use qsweb::http::Response;

use super::session::KEYPAD;

/// The whole script.
pub fn text() -> &'static str {
    static S: OnceLock<String> = OnceLock::new();
    S.get_or_init(|| termlook::script::text(KEYPAD))
}

/// `GET /5250/script.js`.
pub fn js() -> Response {
    Response::new(
        200,
        "text/javascript; charset=utf-8",
        text().as_bytes().to_vec(),
    )
    .with("Cache-Control", "max-age=3600")
}
