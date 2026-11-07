// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/5250/script.js`: the session page's script (`assets/s5250.js`)
//! after the console font, as the server draws with it, and the keypad's
//! functions.

use std::fmt::Write as _;
use std::sync::OnceLock;

use qsweb::http::Response;

use super::session::KEYPAD;
use crate::s5250::look::font;

/// The script itself.
const SOURCE: &str = include_str!("../../../assets/s5250.js");

/// The whole script.
pub fn text() -> &'static str {
    static S: OnceLock<String> = OnceLock::new();
    S.get_or_init(|| {
        let mut s = String::from("var QS_FONT = {");
        for (c, rows) in font::table() {
            let r: Vec<String> = rows.iter().map(ToString::to_string).collect();
            let _ = write!(s, "{}:[{}],", *c as u32, r.join(","));
        }
        let boxed: Vec<String> = font::glyph('\u{10FFFF}')
            .iter()
            .map(ToString::to_string)
            .collect();
        let _ = writeln!(s, "box:[{}]}};", boxed.join(","));
        s.push_str("var KEYPAD = {");
        let pairs: Vec<String> = KEYPAD
            .iter()
            .map(|(l, f)| format!("\"{l}\":\"{f}\""))
            .collect();
        s.push_str(&pairs.join(","));
        s.push_str("};\n");
        s.push_str(SOURCE);
        s
    })
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
