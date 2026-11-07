// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Just enough XML for the spreadsheet files: escaping, and a reader of
//! elements (name, attributes, empty or not) and text, entities
//! decoded. No DTDs, namespaces kept as written (`table:table-cell`).

/// Text as XML character data or an attribute value.
pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Control characters XML 1.0 cannot hold.
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// A piece of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// `<name a="v">` (`empty` for `<name/>`).
    Open {
        /// The element name.
        name: String,
        /// Attributes in order.
        attrs: Vec<(String, String)>,
        /// Self-closing.
        empty: bool,
    },
    /// `</name>`.
    Close(String),
    /// Text, entities decoded.
    Text(String),
}

impl Node {
    /// An attribute of an `Open`.
    pub fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Node::Open { attrs, .. } => attrs.iter().find(|a| a.0 == key).map(|a| a.1.as_str()),
            _ => None,
        }
    }
}

/// Decode `&amp;` and the like, and character references.
pub fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e < 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let c = match &rest[1..end] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
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

fn attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_string();
        let after = rest[eq + 1..].trim_start();
        let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            break;
        };
        let Some(end) = after[1..].find(q) else {
            break;
        };
        out.push((key, unesc(&after[1..=end])));
        rest = after[end + 2..].trim_start();
    }
    out
}

/// The nodes of `doc`; declarations, comments and processing
/// instructions are left out.
pub fn parse(doc: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let mut rest = doc;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push(Node::Text(unesc(rest)));
            break;
        };
        if lt > 0 {
            out.push(Node::Text(unesc(&rest[..lt])));
        }
        rest = &rest[lt..];
        if let Some(r) = rest.strip_prefix("<!--") {
            rest = r.find("-->").map_or("", |e| &r[e + 3..]);
            continue;
        }
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            out.push(Node::Text(r[..end].to_string()));
            rest = r.get(end + 3..).unwrap_or("");
            continue;
        }
        let Some(gt) = rest.find('>') else {
            break;
        };
        let inner = &rest[1..gt];
        rest = &rest[gt + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            out.push(Node::Close(name.trim().to_string()));
            continue;
        }
        let empty = inner.ends_with('/');
        let inner = inner.trim_end_matches('/');
        let split = inner.find(char::is_whitespace).unwrap_or(inner.len());
        out.push(Node::Open {
            name: inner[..split].to_string(),
            attrs: attrs(&inner[split..]),
            empty,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        let doc = format!("<?xml version=\"1.0\"?><a x=\"1\" y='{}'><b/>{}</a>", esc("<&>"), esc("t & \"u\""));
        let n = parse(&doc);
        assert_eq!(n[0].attr("y"), Some("<&>"));
        assert!(matches!(&n[1], Node::Open { name, empty: true, .. } if name == "b"));
        assert_eq!(n[2], Node::Text("t & \"u\"".into()));
        assert_eq!(n[3], Node::Close("a".into()));
        assert_eq!(unesc("&#65;&#x42;&zz;"), "AB&zz;");
    }
}
