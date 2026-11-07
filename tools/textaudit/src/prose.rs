// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Whether a literal is text a person reads (words in a language), as
//! opposed to an interface token: an IBM name, a keyword, a special
//! value, a variable name, a path, a format string of placeholders.

/// The literal with `{...}` placeholders and `&N` substitutions removed.
fn strip(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// The literal's words: runs of letters and apostrophes.
fn words(s: &str) -> Vec<&str> {
    s.split(|c: char| !(c.is_alphabetic() || c == '\''))
        .filter(|w| w.chars().any(char::is_alphabetic))
        .collect()
}

/// A single token a program uses as a key or a name: no spaces, and
/// lower case, snake case, a path, a file name or an option.
fn identifier_like(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return false;
    }
    let lowerish = t.chars().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:=@%+#$&*?<>[]()|\\,'\"~^!".contains(c)
    });
    let path = t.starts_with('/') || t.starts_with("./") || t.contains("::");
    let camel = t.chars().next().is_some_and(char::is_uppercase)
        && t.chars().skip(1).any(char::is_uppercase)
        && t.chars().any(char::is_lowercase)
        && t.chars().all(char::is_alphanumeric);
    let dotted = t.contains('.')
        && !t.ends_with('.')
        && t.chars().all(|c| c.is_alphanumeric() || "._-".contains(c));
    lowerish || path || camel || dotted || t.contains('_')
}

/// Whether `s` is prose: a word of two letters or more with a lower
/// case letter, outside an identifier-like token.
#[must_use]
pub fn is_prose(s: &str) -> bool {
    let t = strip(s);
    if identifier_like(&t) {
        return false;
    }
    words(&t).iter().any(|w| {
        w.chars().filter(|c| c.is_alphabetic()).count() >= 2 && w.chars().any(char::is_lowercase)
    })
}

#[cfg(test)]
mod tests {
    use super::is_prose;

    #[test]
    fn tokens_are_not_prose() {
        for s in [
            "QSYS",
            "*OUTFILE",
            "RSCNAME",
            "CPF9898",
            "slic",
            "/run/qs400",
            "{}/{}",
            "{name:<10}",
            "%Y-%m-%d",
            "x86_64",
            "ObjectId",
            "qs400.svc",
            "true",
            "  ",
            "*N",
            "Y",
            "{e}",
            "QUIHDWRSC",
        ] {
            assert!(!is_prose(s), "{s}");
        }
    }

    #[test]
    fn words_are_prose() {
        for s in [
            "Operational",
            "{ty} is not a type of resource.",
            "Opt  Resource",
            "not found",
            "Ethernet Port",
        ] {
            assert!(is_prose(s), "{s}");
        }
    }
}
