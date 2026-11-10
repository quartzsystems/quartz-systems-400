// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory search patterns ([MS-FSA] 2.1.4.4): `*`, `?`, and the DOS
//! forms `<` (`DOS_STAR`), `>` (`DOS_QM`) and `"` (`DOS_DOT`), matched without
//! regard to case.

/// Whether `name` matches `pattern`.
#[must_use]
pub fn matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let n: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    if p == ['*'] {
        return true;
    }
    step(&p, &n)
}

fn step(p: &[char], n: &[char]) -> bool {
    let Some((&c, rest)) = p.split_first() else {
        return n.is_empty();
    };
    match c {
        '*' => (0..=n.len()).any(|i| step(rest, &n[i..])),
        '<' => {
            // Any run of characters that leaves the name's last dot after it.
            let last_dot = n.iter().rposition(|&x| x == '.');
            (0..=n.len()).any(|i| last_dot.is_none_or(|d| i <= d) && step(rest, &n[i..]))
        }
        '?' => !n.is_empty() && step(rest, &n[1..]),
        '>' => match n.first() {
            None | Some('.') => step(rest, n),
            Some(_) => step(rest, &n[1..]),
        },
        '"' => match n.first() {
            None => step(rest, n),
            Some('.') => step(rest, &n[1..]),
            Some(_) => false,
        },
        c => n.first() == Some(&c) && step(rest, &n[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn wildcards() {
        assert!(matches("*", "anything.txt"));
        assert!(matches("*.TXT", "notes.txt"));
        assert!(!matches("*.txt", "notes.txt.bak"));
        assert!(matches("a?c", "ABC"));
        assert!(matches("<.txt", "my.notes.txt"));
        assert!(matches("<", "file.txt") || matches("<", "file"));
        assert!(matches("file\"txt", "file.txt"));
        assert!(matches("file\"", "file"));
        assert!(matches("ab>>>", "ab"));
        assert!(!matches("x", "y"));
    }
}
