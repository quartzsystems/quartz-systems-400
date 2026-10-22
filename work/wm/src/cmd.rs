// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Command lines as the bridged console words take them: positional
//! words, then IBM-style `KEYWORD(value)` parameters. The analyzer in
//! front of every line spells a bridged command this way.

/// Positional words and `KEYWORD(value)` parameters of a command line.
#[derive(Debug, Default)]
pub struct Params {
    /// Words before the first keyword.
    pub pos: Vec<String>,
    /// Keywords, upper case, with their raw value text.
    pub kw: Vec<(String, String)>,
}

impl Params {
    /// Parse the line after its command word.
    #[must_use]
    pub fn parse(line: &str) -> Self {
        let rest = line.trim_start();
        let rest = rest
            .find(char::is_whitespace)
            .map_or("", |i| &rest[i..])
            .trim_start();
        let mut p = Self::default();
        let chars: Vec<char> = rest.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '(' {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if i < chars.len() && chars[i] == '(' {
                // The value runs to the matching parenthesis, quotes respected.
                i += 1;
                let mut depth = 1;
                let mut quoted = false;
                let vstart = i;
                while i < chars.len() {
                    let c = chars[i];
                    if c == '\'' {
                        quoted = !quoted;
                    } else if !quoted && c == '(' {
                        depth += 1;
                    } else if !quoted && c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
                let value: String = chars[vstart..i.min(chars.len())].iter().collect();
                i += 1;
                p.kw.push((word.to_ascii_uppercase(), value.trim().to_string()));
            } else if p.kw.is_empty() {
                p.pos.push(word);
            } else {
                // A bare word after keywords is a flag like *YES.
                p.kw.push((word.to_ascii_uppercase(), String::new()));
            }
        }
        p
    }

    /// The value of a keyword.
    #[must_use]
    pub fn kw(&self, name: &str) -> Option<&str> {
        self.kw
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// A keyword's value, upper case, with `*` kept.
    #[must_use]
    pub fn upper(&self, name: &str) -> Option<String> {
        self.kw(name).map(str::to_ascii_uppercase)
    }

    /// Whether a keyword says `*YES`.
    #[must_use]
    pub fn yes(&self, name: &str) -> bool {
        self.upper(name).is_some_and(|v| v == "*YES")
    }

    /// A numeric keyword.
    pub fn num<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        match self.kw(name) {
            None => Ok(None),
            Some(v) => v
                .parse()
                .map(Some)
                .map_err(|_| format!("{name}({v}): a number is needed")),
        }
    }

    /// The positional word `i`, or the keyword, or an error.
    pub fn pos_or(&self, i: usize, name: &str) -> Result<String, String> {
        self.pos
            .get(i)
            .cloned()
            .or_else(|| self.kw(name).map(str::to_string))
            .ok_or_else(|| format!("{name} is needed"))
    }

    /// A quoted `TEXT('...')` value without its quotes.
    #[must_use]
    pub fn text(&self, name: &str) -> String {
        self.kw(name)
            .map(|t| t.trim_matches('\'').replace("''", "'"))
            .unwrap_or_default()
    }

    /// The list inside a keyword value: `(A B (C D))` gives `A`, `B`
    /// and `C D`, quotes respected.
    #[must_use]
    pub fn list(value: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut depth = 0;
        let mut quoted = false;
        let mut cur = String::new();
        for c in value.chars() {
            match c {
                '\'' => {
                    quoted = !quoted;
                    cur.push(c);
                }
                '(' if !quoted => {
                    if depth > 0 {
                        cur.push(c);
                    }
                    depth += 1;
                }
                ')' if !quoted => {
                    depth -= 1;
                    if depth > 0 {
                        cur.push(c);
                    } else if !cur.trim().is_empty() {
                        out.push(cur.trim().to_string());
                        cur.clear();
                    }
                }
                c if c.is_whitespace() && depth == 0 && !quoted => {
                    if !cur.trim().is_empty() {
                        out.push(cur.trim().to_string());
                    }
                    cur.clear();
                }
                c => cur.push(c),
            }
        }
        if !cur.trim().is_empty() {
            out.push(cur.trim().to_string());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let p = Params::parse("crtpf QGPL/F SRCFILE(QGPL/QDDSSRC) TEXT('a (b) c') *YES");
        assert_eq!(p.pos, vec!["QGPL/F"]);
        assert_eq!(p.kw("SRCFILE"), Some("QGPL/QDDSSRC"));
        assert_eq!(p.text("TEXT"), "a (b) c");
        assert_eq!(p.kw("*YES"), Some(""));
        assert_eq!(p.pos_or(1, "X").unwrap_err(), "X is needed");
    }

    #[test]
    fn list() {
        assert_eq!(Params::list("A B"), vec!["A", "B"]);
        assert_eq!(
            Params::list("(1 *BASE) (2 *INTERACT)"),
            vec!["1 *BASE", "2 *INTERACT"]
        );
        assert_eq!(Params::list("'a b' c"), vec!["'a b'", "c"]);
    }
}
