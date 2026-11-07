// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! HTML text and query strings.

use crate::http::form::encode;

/// Text safe inside HTML and attribute values.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A query string from pairs, empty values left out: `?a=1&b=2`, or
/// `?` alone when nothing is left (a link back to the defaults).
pub fn query(pairs: &[(&str, &str)]) -> String {
    let q: Vec<String> = pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={}", encode(v)))
        .collect();
    format!("?{}", q.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_queries() {
        assert_eq!(
            esc("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
        assert_eq!(query(&[("status", ""), ("sort", "name")]), "?sort=name");
        assert_eq!(query(&[("q", "a b")]), "?q=a%20b");
    }
}
