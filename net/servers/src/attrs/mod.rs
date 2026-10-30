// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers' attributes as typed values over the keyword bag the
//! `QUSRSYS` server table keeps for each server (`tcpcfg`'s
//! `ServerAttrs`, file `QATOCSVRA`); the autostart flag beside it is
//! the table's own (`QATOCSTART`).

pub mod http;
pub mod telnet;

/// A bag value by keyword (case does not matter).
pub fn get<'a>(bag: &'a [(String, String)], kwd: &str) -> Option<&'a str> {
    bag.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(kwd))
        .map(|(_, v)| v.as_str())
}

/// `KWD(value)` in request data or a keyword string.
pub fn keyword(text: &str, kwd: &str) -> Option<String> {
    let upper = text.to_ascii_uppercase();
    let key = format!("{}(", kwd.to_ascii_uppercase());
    let mut from = 0;
    while let Some(at) = upper[from..].find(&key) {
        let start = from + at;
        // A whole keyword: start of text or after a blank.
        if start == 0 || upper.as_bytes()[start - 1] == b' ' {
            let rest = &text[start + key.len()..];
            return rest.split(')').next().map(|v| v.trim().to_string());
        }
        from = start + key.len();
    }
    None
}

/// Whether a command value means "leave it": `*SAME`, or the `-1` the
/// analyzer maps it to for a numeric parameter, or nothing.
pub fn is_same(v: &str) -> bool {
    let v = v.trim();
    v.is_empty() || v.eq_ignore_ascii_case("*SAME") || v == "-1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords() {
        assert_eq!(keyword("PORT(2001) TLSPORT(2005)", "PORT").as_deref(), Some("2001"));
        assert_eq!(keyword("TLSPORT(2005) PORT(2001)", "port").as_deref(), Some("2001"));
        assert_eq!(keyword("TLSPORT(2005)", "PORT"), None);
        assert!(is_same("*same") && is_same("-1") && !is_same("5"));
    }
}
