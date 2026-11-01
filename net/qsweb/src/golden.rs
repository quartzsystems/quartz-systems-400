// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Comparing a served page with its golden rendering: both are reduced
//! to a token stream (tags with sorted attributes, text with entities
//! decoded and whitespace collapsed), the CSRF field dropped, and the
//! served page's URLs mapped to the prototype's file names by the
//! caller. Also the links and form actions a page holds, for crawling.

use std::collections::BTreeMap;

/// A token of a page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// `<name attr=value ...>`, attributes sorted, `class` normalized.
    Open(String, BTreeMap<String, String>),
    /// `</name>`.
    Close(String),
    /// Text between tags, whitespace collapsed, entities decoded.
    Text(String),
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tok::Open(n, a) => {
                write!(f, "<{n}")?;
                for (k, v) in a {
                    write!(f, " {k}=\"{v}\"")?;
                }
                f.write_str(">")
            }
            Tok::Close(n) => write!(f, "</{n}>"),
            Tok::Text(t) => write!(f, "{t:?}"),
        }
    }
}

/// Attributes holding URLs.
const URL_ATTRS: [&str; 3] = ["href", "action", "src"];

/// Decode the entities the pages use.
pub fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let c = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" | "#x27" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            "middot" => Some('\u{b7}'),
            "times" => Some('\u{d7}'),
            "rarr" => Some('\u{2192}'),
            "larr" => Some('\u{2190}'),
            "hellip" => Some('\u{2026}'),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "copy" => Some('\u{a9}'),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        if let Some(c) = c {
            out.push(c);
            rest = &rest[end + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn collapse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        // A no-break space is content, not layout.
        if c.is_whitespace() && c != '\u{a0}' {
            space = true;
        } else {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        }
    }
    out
}

#[allow(clippy::many_single_char_names)]
fn parse_attrs(s: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        if start == i {
            break;
        }
        let name = s[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                let vs = i + 1;
                i = vs;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                value = decode_entities(&s[vs..i]);
                i += 1;
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = decode_entities(&s[vs..i]);
            }
        }
        if name == "class" {
            value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        m.insert(name, value);
    }
    m
}

/// The tokens of `html`. `<!DOCTYPE>` and comments are dropped; text
/// inside `<pre>` and `<textarea>` keeps its whitespace.
pub fn tokens(html: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut rest = html;
    let mut verbatim = 0usize;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
            continue;
        }
        if rest.starts_with("<!") {
            rest = rest.find('>').map_or("", |e| &rest[e + 1..]);
            continue;
        }
        if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/') {
            let end = rest.find('>').unwrap_or(rest.len() - 1);
            let inner = &rest[1..end];
            rest = &rest[(end + 1).min(rest.len())..];
            if let Some(name) = inner.strip_prefix('/') {
                let name = name.trim().to_ascii_lowercase();
                if name == "pre" || name == "textarea" {
                    verbatim = verbatim.saturating_sub(1);
                }
                out.push(Tok::Close(name));
            } else {
                let split = inner.find(|c: char| c.is_ascii_whitespace() || c == '/').unwrap_or(inner.len());
                let name = inner[..split].to_ascii_lowercase();
                if name == "pre" || name == "textarea" {
                    verbatim += 1;
                }
                out.push(Tok::Open(name, parse_attrs(&inner[split..])));
            }
            continue;
        }
        let end = rest[1..].find('<').map_or(rest.len(), |e| e + 1);
        let raw = &rest[..end];
        rest = &rest[end..];
        let text = if verbatim > 0 {
            decode_entities(raw).replace("\r\n", "\n")
        } else {
            collapse(&decode_entities(raw))
        };
        if text.trim_matches(|c: char| c.is_whitespace() && c != '\u{a0}').is_empty() && verbatim == 0 {
            continue;
        }
        match out.last_mut() {
            Some(Tok::Text(t)) => {
                t.push(' ');
                t.push_str(&text);
            }
            _ => out.push(Tok::Text(text)),
        }
    }
    out
}

/// How a comparison treats the served page.
pub struct Normalize<'a> {
    /// Maps a served URL to the prototype's (`/qs400/httpadmin/instances`
    /// to `instances.html`); `None` leaves it.
    pub url: &'a dyn Fn(&str) -> Option<String>,
}

fn normalize(toks: Vec<Tok>, n: Option<&Normalize<'_>>) -> Vec<Tok> {
    let mut out = Vec::with_capacity(toks.len());
    for t in toks {
        match t {
            Tok::Open(name, attrs)
                if name == "input" && attrs.get("name").map(String::as_str) == Some(crate::session::CSRF_FIELD) => {}
            Tok::Open(name, mut attrs) => {
                if let Some(n) = n {
                    for a in URL_ATTRS {
                        if let Some(v) = attrs.get_mut(a)
                            && let Some(m) = (n.url)(v)
                        {
                            *v = m;
                        }
                    }
                }
                out.push(Tok::Open(name, attrs));
            }
            t => out.push(t),
        }
    }
    out
}

/// The first difference between `served` and `golden`, as text, or
/// `None` when they are the same page.
pub fn diff(served: &str, golden: &str, n: &Normalize<'_>) -> Option<String> {
    let a = normalize(tokens(served), Some(n));
    let b = normalize(tokens(golden), None);
    let at = a.iter().zip(&b).position(|(x, y)| x != y).or_else(|| (a.len() != b.len()).then(|| a.len().min(b.len())))?;
    let ctx = |v: &[Tok]| {
        v[at.saturating_sub(3)..(at + 3).min(v.len())]
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n    ")
    };
    Some(format!(
        "token {at}:\n  served: {}\n  golden: {}\n  served context:\n    {}\n  golden context:\n    {}",
        a.get(at).map_or("(end)".into(), ToString::to_string),
        b.get(at).map_or("(end)".into(), ToString::to_string),
        ctx(&a),
        ctx(&b)
    ))
}

/// Every link, form action and stylesheet URL in `html` (with the
/// form's method for actions: `GET` or `POST`).
pub fn links(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for t in tokens(html) {
        if let Tok::Open(name, attrs) = t {
            match name.as_str() {
                "a" | "link" => {
                    if let Some(h) = attrs.get("href") {
                        out.push(("GET".into(), h.clone()));
                    }
                }
                "form" => {
                    let method = attrs.get("method").map_or("GET".into(), |m| m.to_ascii_uppercase());
                    out.push((method, attrs.get("action").cloned().unwrap_or_default()));
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_page_modulo_whitespace_csrf_and_urls() {
        let golden = "<p class=\"a  b\">x&nbsp;y\n  z</p>\n<a href=\"instances.html\">I</a><form method=\"post\"><pre>a\n  b</pre></form>";
        let served = "<p class=\"a b\">x\u{a0}y z</p><a href=\"/p/instances\">I</a><form method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"t\"><pre>a\n  b</pre></form>";
        let map = |u: &str| u.strip_prefix("/p/").map(|r| format!("{r}.html"));
        let n = Normalize { url: &map };
        assert_eq!(diff(served, golden, &n), None);
        let d = diff("<p>x</p>", "<p>y</p>", &n).unwrap();
        assert!(d.contains("served: \"x\""), "{d}");
        assert!(diff("<pre>a b</pre>", "<pre>a  b</pre>", &n).is_some());
        assert_eq!(
            links("<a href=\"/x\">x</a><form method=\"post\" action=\"/y\"></form><form method=\"get\">"),
            vec![("GET".into(), "/x".into()), ("POST".into(), "/y".into()), ("GET".into(), String::new())]
        );
    }
}
