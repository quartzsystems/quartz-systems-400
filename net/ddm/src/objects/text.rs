// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `KEYWORD(value)` text: values with blanks, parentheses or quotes are
//! quoted, a quote doubled.

/// One `KEYWORD(value)`.
pub fn kv(k: &str, v: &str) -> String {
    let plain = !v.is_empty() && !v.contains([' ', '(', ')', '\'']);
    if plain {
        format!("{k}({v})")
    } else {
        format!("{k}('{}')", v.replace('\'', "''"))
    }
}

/// The pairs of a line, in order.
pub fn parse(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let c: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < c.len() {
        while i < c.len() && c[i] == ' ' {
            i += 1;
        }
        let start = i;
        while i < c.len() && c[i] != '(' {
            i += 1;
        }
        if i >= c.len() {
            break;
        }
        let key: String = c[start..i].iter().collect();
        i += 1;
        let mut val = String::new();
        if c.get(i) == Some(&'\'') {
            i += 1;
            while i < c.len() {
                if c[i] == '\'' {
                    if c.get(i + 1) == Some(&'\'') {
                        val.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                val.push(c[i]);
                i += 1;
            }
            while i < c.len() && c[i] != ')' {
                i += 1;
            }
        } else {
            while i < c.len() && c[i] != ')' {
                val.push(c[i]);
                i += 1;
            }
        }
        i += 1;
        out.push((key.trim().to_string(), val));
    }
    out
}

/// The value of `k` among the pairs.
pub fn get<'a>(pairs: &'a [(String, String)], k: &str) -> Option<&'a str> {
    pairs.iter().find(|(x, _)| x == k).map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        let line = [kv("A", "x"), kv("TEXT", "it's (here)"), kv("E", "")].join(" ");
        let p = parse(&line);
        assert_eq!(get(&p, "A"), Some("x"));
        assert_eq!(get(&p, "TEXT"), Some("it's (here)"));
        assert_eq!(get(&p, "E"), Some(""));
    }
}
