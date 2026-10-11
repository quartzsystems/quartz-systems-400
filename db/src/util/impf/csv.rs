// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Delimited records: splitting with string delimiters, joining.

/// Split one record into fields. A field inside `strdlm` may hold the
/// field delimiter; a doubled string delimiter is one character.
#[must_use]
pub fn split(line: &str, flddlm: char, strdlm: Option<char>) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if Some(c) == strdlm {
                if chars.peek() == Some(&c) {
                    cur.push(c);
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if Some(c) == strdlm && cur.trim().is_empty() {
            cur.clear();
            quoted = true;
        } else if c == flddlm {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    fields
}

/// Quote a text field: the string delimiter doubled inside.
#[must_use]
pub fn quote(s: &str, strdlm: Option<char>) -> String {
    match strdlm {
        None => s.to_string(),
        Some(q) => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push(q);
            for c in s.chars() {
                if c == q {
                    out.push(q);
                }
                out.push(c);
            }
            out.push(q);
            out
        }
    }
}

/// Split the text into records on the delimiter; `Eor` accepts a bare
/// line feed too. A trailing empty record is not one.
#[must_use]
pub fn records(text: &str, dlm: super::RcdDlm) -> Vec<&str> {
    use super::RcdDlm;
    let mut out: Vec<&str> = match dlm {
        RcdDlm::Crlf => text.split("\r\n").collect(),
        RcdDlm::Cr => text.split('\r').collect(),
        RcdDlm::Lf | RcdDlm::Eor => text
            .split('\n')
            .map(|r| r.strip_suffix('\r').unwrap_or(r))
            .collect(),
    };
    if out.last().is_some_and(|r| r.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting() {
        assert_eq!(
            split(r#"1,"Smith, ""Al""",,3.5"#, ',', Some('"')),
            ["1", "Smith, \"Al\"", "", "3.5"]
        );
        assert_eq!(split("a;b", ';', None), ["a", "b"]);
        assert_eq!(quote("say \"hi\"", Some('"')), "\"say \"\"hi\"\"\"");
        assert_eq!(records("a\r\nb\n", super::super::RcdDlm::Eor), ["a", "b"]);
        assert_eq!(records("a\rb", super::super::RcdDlm::Cr), ["a", "b"]);
    }
}
