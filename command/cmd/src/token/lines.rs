// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Source lines into logical lines: `/* */` comments go, a line ending
//! in `+` continues on the next line's first non-blank, one ending in
//! `-` continues with the next line's blanks kept. Quoted text keeps
//! everything.

/// A logical line and the source lines it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Joined {
    /// The text, comments removed, continuations joined.
    pub text: String,
    /// First source line (1-based).
    pub first: usize,
    /// Last source line (1-based).
    pub last: usize,
}

/// Join `lines` (each without its line end) into logical lines. Blank
/// logical lines are dropped.
#[must_use]
pub fn join(lines: &[&str]) -> Vec<Joined> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut first = 0;
    let mut in_comment = false;
    let mut continued: Option<char> = None;
    for (i, raw) in lines.iter().enumerate() {
        let line = strip_comments(raw, &mut in_comment);
        let piece = match continued {
            Some('+') => line.trim_start(),
            Some(_) => line.as_str(),
            None => {
                first = i + 1;
                line.as_str()
            }
        };
        let trimmed = piece.trim_end();
        let (body, cont) = continuation(trimmed);
        if continued.is_none() {
            cur.clear();
        }
        cur.push_str(body);
        continued = cont;
        if continued.is_none() {
            if !cur.trim().is_empty() {
                out.push(Joined {
                    text: cur.trim().to_string(),
                    first,
                    last: i + 1,
                });
            }
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(Joined {
            text: cur.trim().to_string(),
            first,
            last: lines.len(),
        });
    }
    out
}

/// Whether `line` (trimmed at the end) ends in a continuation character,
/// inside a quoted string or not: the body before it and the character.
fn continuation(line: &str) -> (&str, Option<char>) {
    let Some(last) = line.chars().last() else {
        return (line, None);
    };
    if last != '+' && last != '-' {
        return (line, None);
    }
    let body = &line[..line.len() - 1];
    (body, Some(last))
}

/// `line` without `/* */` comments; `in_comment` carries a comment that
/// spans lines.
fn strip_comments(line: &str, in_comment: &mut bool) -> String {
    let mut out = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        if *in_comment {
            if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                *in_comment = false;
                i += 2;
                out.push(' ');
            } else {
                i += 1;
            }
            continue;
        }
        let c = chars[i];
        if c == '\'' {
            quoted = !quoted;
        } else if !quoted && c == '/' && chars.get(i + 1) == Some(&'*') {
            *in_comment = true;
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_and_strips() {
        let src = [
            "/* header */",
            "CRTPF FILE(X) +",
            "      SRCFILE(Y) /* src */ TEXT('a +",
            "b') -",
            "   MBR(Z)",
            "",
            "DLTF X /* multi",
            "line */ MBR(Q)",
        ];
        let j = join(&src);
        assert_eq!(j.len(), 3, "{j:#?}");
        assert_eq!(
            j[0].text,
            "CRTPF FILE(X) SRCFILE(Y)   TEXT('a b')    MBR(Z)"
        );
        assert_eq!((j[0].first, j[0].last), (2, 5));
        assert_eq!(j[1].text, "DLTF X");
        assert_eq!(j[2].text, "MBR(Q)");
    }

    #[test]
    fn quoted_text_continues_too() {
        let j = join(&["SNDPGMMSG MSG('abc -", "   def')", "DLTF Y"]);
        assert_eq!(j.len(), 2);
        assert_eq!(j[0].text, "SNDPGMMSG MSG('abc    def')");
    }
}
