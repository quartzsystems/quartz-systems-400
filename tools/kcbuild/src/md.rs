// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Markdown the documents use, to HTML: headings, paragraphs, fenced
//! code, lists, pipe tables, inline code, bold, italics and links (a
//! `.md` link becomes the page's `.html`). Nothing else is needed.

use std::fmt::Write as _;

/// Escapes text for HTML.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

/// One line's inline markup.
pub fn inline(s: &str) -> String {
    let mut out = String::new();
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut bold = false;
    let mut ital = false;
    while i < b.len() {
        let c = b[i];
        if c == '`'
            && let Some(end) = b[i + 1..].iter().position(|&x| x == '`')
        {
            let code: String = b[i + 1..i + 1 + end].iter().collect();
            let _ = write!(out, "<code>{}</code>", esc(&code));
            i += end + 2;
            continue;
        }
        if c == '*' && b.get(i + 1) == Some(&'*') {
            out.push_str(if bold { "</b>" } else { "<b>" });
            bold = !bold;
            i += 2;
            continue;
        }
        if c == '*' && (ital || b.get(i + 1).is_some_and(|n| !n.is_whitespace())) {
            out.push_str(if ital { "</i>" } else { "<i>" });
            ital = !ital;
            i += 1;
            continue;
        }
        if c == '['
            && let Some(close) = b[i..].iter().position(|&x| x == ']')
            && b.get(i + close + 1) == Some(&'(')
            && let Some(paren) = b[i + close + 1..].iter().position(|&x| x == ')')
        {
            let text: String = b[i + 1..i + close].iter().collect();
            let url: String = b[i + close + 2..i + close + 1 + paren].iter().collect();
            let url = link(&url);
            let _ = write!(out, "<a href=\"{}\">{}</a>", esc(&url), inline(&text));
            i += close + 2 + paren;
            continue;
        }
        out.push_str(&esc(&c.to_string()));
        i += 1;
    }
    if bold {
        out.push_str("</b>");
    }
    if ital {
        out.push_str("</i>");
    }
    out
}

/// A link to another document becomes that document's page.
fn link(url: &str) -> String {
    if url.contains("://") {
        return url.to_string();
    }
    let (path, frag) = url.split_once('#').map_or((url, ""), |(p, f)| (p, f));
    let name = path.rsplit('/').next().unwrap_or(path);
    let page = name
        .strip_suffix(".md")
        .map_or_else(|| path.to_string(), |n| format!("{n}.html"));
    if frag.is_empty() {
        page
    } else {
        format!("{page}#{frag}")
    }
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim().trim_start_matches('|').trim_end_matches('|');
    // A `|` inside backticks belongs to the cell.
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut code = false;
    for c in t.chars() {
        match c {
            '`' => {
                code = !code;
                cur.push(c);
            }
            '|' if !code => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

fn is_sep(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// A slug for a heading's anchor.
pub fn slug(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if c.is_ascii_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if c == ' ' || c == '-' {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // one pass over the lines
/// A whole document; the first `#` heading is returned apart as the title.
pub fn to_html(md: &str) -> (String, String) {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = String::new();
    let mut title = String::new();
    let mut i = 0;
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, out: &mut String| {
        if !para.is_empty() {
            out.push_str("<p>");
            out.push_str(&inline(&para.join(" ")));
            out.push_str("</p>\n");
            para.clear();
        }
    };
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim_end();
        if t.trim_start().starts_with("```") {
            flush(&mut para, &mut out);
            out.push_str("<pre>");
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                out.push_str(&esc(lines[i]));
                out.push('\n');
                i += 1;
            }
            out.push_str("</pre>\n");
            i += 1;
            continue;
        }
        if let Some(h) = t.strip_prefix('#') {
            flush(&mut para, &mut out);
            let level = 1 + h.chars().take_while(|&c| c == '#').count();
            let text = h.trim_start_matches('#').trim();
            if level == 1 && title.is_empty() {
                title = text.to_string();
            } else {
                let n = level.min(4);
                let _ = writeln!(out, "<h{n} id=\"{}\">{}</h{n}>\n", slug(text), inline(text));
            }
            i += 1;
            continue;
        }
        if t.trim_start().starts_with('|') && lines.get(i + 1).is_some_and(|n| is_sep(n)) {
            flush(&mut para, &mut out);
            out.push_str("<table class=\"list\" cellspacing=\"0\"><tr>");
            for c in cells(t) {
                let _ = write!(out, "<th>{}</th>", inline(&c));
            }
            out.push_str("</tr>\n");
            i += 2;
            let mut alt = false;
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                out.push_str(if alt { "<tr class=\"alt\">" } else { "<tr>" });
                for c in cells(lines[i]) {
                    let _ = write!(out, "<td>{}</td>", inline(&c));
                }
                out.push_str("</tr>\n");
                alt = !alt;
                i += 1;
            }
            out.push_str("</table>\n");
            continue;
        }
        let ts = t.trim_start();
        let ordered = ts
            .split_once(". ")
            .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if ts.starts_with("- ") || ts.starts_with("* ") || ordered {
            flush(&mut para, &mut out);
            let tag = if ordered { "ol" } else { "ul" };
            let _ = writeln!(out, "<{tag}>\n");
            while i < lines.len() {
                let li = lines[i].trim_start();
                let item = if let Some(r) = li.strip_prefix("- ").or_else(|| li.strip_prefix("* "))
                {
                    Some(r)
                } else {
                    li.split_once(". ")
                        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                        .map(|(_, r)| r)
                };
                let Some(first) = item else { break };
                let mut text = first.to_string();
                i += 1;
                // Continuation lines are indented.
                while i < lines.len() && lines[i].starts_with("  ") && !lines[i].trim().is_empty() {
                    let c = lines[i].trim();
                    if c.starts_with("- ") || c.starts_with("* ") {
                        break;
                    }
                    text.push(' ');
                    text.push_str(c);
                    i += 1;
                }
                let _ = writeln!(out, "<li>{}</li>\n", inline(&text));
            }
            let _ = writeln!(out, "</{tag}>\n");
            continue;
        }
        if t.trim().is_empty() {
            flush(&mut para, &mut out);
        } else {
            para.push(t.trim());
        }
        i += 1;
    }
    flush(&mut para, &mut out);
    (title, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_inline() {
        let (t, h) = to_html(
            "# Title\n\nSome `x<y` and **b** [d](DB_FORMAT.md#a).\n\n| A | B |\n|---|---|\n| `a|b` | 2 |\n\n- one\n- two\n",
        );
        assert_eq!(t, "Title");
        assert!(h.contains("<code>x&lt;y</code>"));
        assert!(h.contains("<b>b</b>"));
        assert!(h.contains("href=\"DB_FORMAT.html#a\""));
        assert!(h.contains("<td><code>a|b</code></td>"));
        assert!(h.contains("<li>two</li>"));
    }
}
