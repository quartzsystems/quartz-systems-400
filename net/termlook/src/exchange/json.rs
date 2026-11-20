// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The little JSON the exchange needs: strings out, and one field of a
//! flat object in.

use std::fmt::Write as _;

/// `s` as a JSON string.
pub fn string(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// The value of field `name` of the flat object `msg`: a string's
/// characters, or a number's digits.
pub fn field(msg: &str, name: &str) -> Option<String> {
    let at = msg.find(&format!("\"{name}\":"))? + name.len() + 3;
    let rest = msg[at..].trim_start();
    if let Some(body) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut it = body.chars();
        while let Some(c) = it.next() {
            match c {
                '\\' => match it.next()? {
                    'n' => out.push('\n'),
                    'u' => {
                        let h: String = it.by_ref().take(4).collect();
                        out.push(char::from_u32(u32::from_str_radix(&h, 16).ok()?)?);
                    }
                    c => out.push(c),
                },
                '"' => return Some(out),
                c => out.push(c),
            }
        }
        None
    } else {
        Some(rest.chars().take_while(char::is_ascii_digit).collect())
    }
}
