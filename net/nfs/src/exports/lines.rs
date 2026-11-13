// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The logical lines of `/etc/exports` and `/etc/netgroup`: a `#`
//! outside double quotes starts a comment, a `\` that ends a line joins
//! the next one to it.

/// The logical lines with the number of the line each starts on (from
/// 1); comments are removed, blank lines kept as empty.
#[must_use]
pub fn logical(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut start = 0;
    for (i, raw) in text.lines().enumerate() {
        if cur.is_empty() {
            start = i + 1;
        }
        let line = strip_comment(raw.trim_end_matches('\r'));
        let trimmed = line.trim_end();
        if let Some(body) = trimmed.strip_suffix('\\') {
            cur.push_str(body);
            cur.push(' ');
            continue;
        }
        cur.push_str(trimmed);
        out.push((start, std::mem::take(&mut cur)));
    }
    if !cur.is_empty() {
        out.push((start, cur));
    }
    out
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_continuations() {
        let l = logical("# head\n/a -ro \\\n  ,anon=0 # tail\n\"/b#c\" -ro\n/d \\");
        assert_eq!(l[0], (1, String::new()));
        assert_eq!(l[1].0, 2);
        assert_eq!(
            l[1].1.split_whitespace().collect::<Vec<_>>(),
            ["/a", "-ro", ",anon=0"]
        );
        assert_eq!(l[2], (4, "\"/b#c\" -ro".into()));
        assert_eq!(l[3].0, 5);
    }
}
