// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing UIM help text: escaping and wrapping.

/// Text as UIM source: `&`, `:` and a leading `.` escaped.
#[must_use]
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp."),
            ':' => out.push_str("&colon."),
            c => out.push(c),
        }
    }
    out
}

/// A tag followed by escaped text, wrapped at 72 columns; no line of
/// the continuation starts with `.` or `:`.
#[must_use]
pub fn tagged(tag: &str, text: &str) -> String {
    let body = esc(text.trim());
    let mut out = String::new();
    let mut line = String::from(tag);
    for w in body.split_whitespace() {
        let w = if let Some(rest) = w.strip_prefix('.') {
            format!("&period.{rest}")
        } else {
            w.to_string()
        };
        if line.len() + 1 + w.len() > 72 && line.len() > tag.len() {
            out.push_str(&line);
            out.push('\n');
            line.clear();
        } else if line.len() > tag.len() {
            line.push(' ');
        }
        line.push_str(&w);
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// Paragraphs (split at blank lines) as `:P.` blocks.
#[must_use]
pub fn paras(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| tagged(":P.", &p.replace('\n', " ")))
        .collect()
}

/// A help module name as an attribute value.
#[must_use]
pub fn name_attr(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric()) {
        name.to_string()
    } else {
        format!("'{name}'")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_wraps() {
        assert_eq!(esc("a & b: c"), "a &amp. b&colon. c");
        let t = tagged(":P.", &"word ".repeat(30));
        assert!(t.lines().all(|l| l.len() <= 72));
        assert!(t.starts_with(":P.word word"));
        assert_eq!(tagged(":P.", "see .profile"), ":P.see &period.profile\n");
    }
}
