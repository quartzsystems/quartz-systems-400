// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The built-in symbols: `&amp.` and friends. Any other `&name.` in
//! text refers to a dialog variable.

/// The character a built-in symbol stands for.
#[must_use]
pub fn builtin(name: &str) -> Option<char> {
    Some(match name.to_ascii_uppercase().as_str() {
        "AMP" => '&',
        "COLON" => ':',
        "PERIOD" | "DOT" => '.',
        "LBRACKET" => '[',
        "RBRACKET" => ']',
        "LBRACE" => '{',
        "RBRACE" => '}',
        "COMMA" => ',',
        "APOS" => '\'',
        "SEMI" => ';',
        "SLR" => '/',
        "BSL" => '\\',
        "SPLAT" => '*',
        _ => return None,
    })
}

/// Every built-in symbol name, for the reference.
pub const NAMES: &[&str] = &[
    "amp", "colon", "period", "dot", "lbracket", "rbracket", "lbrace", "rbrace", "comma", "apos",
    "semi", "slr", "bsl", "splat",
];
