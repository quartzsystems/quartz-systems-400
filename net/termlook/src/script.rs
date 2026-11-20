// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The terminal page's script: [`SOURCE`] as it is, [`text`] what a
//! server sends, after the font it draws with and the keypad's keys.

use std::fmt::Write as _;

use crate::look::font;

/// The script itself (`assets/term.js`).
pub const SOURCE: &str = include_str!("../assets/term.js");

/// The script as served: `QS_FONT`, every glyph of the console font by
/// code point (`box` the hollow box), `KEYPAD` the keypad's labels and
/// keys, then [`SOURCE`].
pub fn text(keypad: &[(&str, &str)]) -> String {
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
    let pairs: Vec<String> = keypad
        .iter()
        .map(|(l, f)| format!("\"{l}\":\"{f}\""))
        .collect();
    s.push_str(&pairs.join(","));
    s.push_str("};\n");
    s.push_str(SOURCE);
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn font_keypad_then_the_script() {
        let t = super::text(crate::exchange::keys::KEYPAD);
        assert!(t.starts_with("var QS_FONT = {32:[0,0,"));
        assert!(t.contains("var KEYPAD = {\"F1\":\"F:1\""));
        assert!(t.ends_with(super::SOURCE) && t.contains("data-socket"));
    }
}
