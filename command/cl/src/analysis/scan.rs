// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a statement's variables are written: every `&NAME` outside
//! quoted strings and comments.

use compdiag::front::Lines;

/// The `&NAME`s in lines `first` to `last`: line, column and the name
/// as written, `&` included.
pub(super) fn variables(lines: &Lines, first: u32, last: u32) -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut comment = false;
    let name_char = |c: char| c.is_ascii_alphanumeric() || "_#@$".contains(c);
    for l in first..=last {
        let chars: Vec<char> = lines.text(l).chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if comment {
                if c == '*' && chars.get(i + 1) == Some(&'/') {
                    comment = false;
                    i += 1;
                }
            } else if quoted {
                if c == '\'' {
                    quoted = false;
                }
            } else if c == '\'' {
                quoted = true;
            } else if c == '/' && chars.get(i + 1) == Some(&'*') {
                comment = true;
                i += 1;
            } else if c == '&'
                && chars
                    .get(i + 1)
                    .is_some_and(|n| n.is_ascii_alphabetic() || "#@$".contains(*n))
            {
                let end = (i + 1..chars.len())
                    .find(|&j| !name_char(chars[j]))
                    .unwrap_or(chars.len());
                let col = u32::try_from(i + 1).unwrap_or(u32::MAX);
                out.push((l, col, chars[i..end].iter().collect()));
                i = end;
                continue;
            }
            i += 1;
        }
    }
    out
}
