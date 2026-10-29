// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Field splitting by `IFS`.

use super::{Field, Q, S};

/// Split a field at unquoted expansion characters found in `IFS`
/// (unset `IFS` is space, tab, newline). Whitespace runs are one
/// delimiter and vanish at the ends; each other `IFS` character
/// delimits a field, with the whitespace around it.
pub(crate) fn split(field: &Field, ifs: Option<&str>) -> Vec<Field> {
    let ifs = ifs.unwrap_or(" \t\n");
    if ifs.is_empty() || !field.iter().any(|(_, f)| f & S != 0 && f & Q == 0) {
        return if field.is_empty() {
            Vec::new()
        } else {
            vec![field.clone()]
        };
    }
    let splits = |i: usize| -> bool {
        let (c, f) = field[i];
        f & S != 0 && f & Q == 0 && ifs.contains(c)
    };
    let ws = |i: usize| -> bool { splits(i) && matches!(field[i].0, ' ' | '\t' | '\n') };
    let n = field.len();
    let mut out = Vec::new();
    let mut cur = Field::new();
    let mut i = 0;
    while i < n && ws(i) {
        i += 1;
    }
    while i < n {
        if splits(i) {
            out.push(std::mem::take(&mut cur));
            let mut j = i;
            while j < n && ws(j) {
                j += 1;
            }
            if j < n && splits(j) && !ws(j) {
                j += 1;
                while j < n && ws(j) {
                    j += 1;
                }
            }
            i = j;
        } else {
            cur.push(field[i]);
            i += 1;
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(s: &str) -> Field {
        s.chars().map(|c| (c, S)).collect()
    }

    fn texts(v: Vec<Field>) -> Vec<String> {
        v.into_iter()
            .map(|f| f.into_iter().map(|(c, _)| c).collect())
            .collect()
    }

    #[test]
    fn posix_rules() {
        assert_eq!(texts(split(&f("  a  b  "), None)), ["a", "b"]);
        assert_eq!(texts(split(&f("a::b"), Some(":"))), ["a", "", "b"]);
        assert_eq!(texts(split(&f(":a:"), Some(":"))), ["", "a"]);
        assert_eq!(texts(split(&f("a : b"), Some(" :"))), ["a", "b"]);
        assert_eq!(texts(split(&f("a :: b"), Some(" :"))), ["a", "", "b"]);
        assert!(split(&f(""), None).is_empty());
    }
}
