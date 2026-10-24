// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Case folding for `CASE(*MIXED)` file systems (root, case-insensitive
//! UDFS): names compare by their folded form and keep the case they
//! were created with. A character folds to its uppercase when that is
//! one character (so `ß` stays `ß`), which keeps the fold a function of
//! single characters, stable across releases.

/// The folded form of a name.
#[must_use]
pub fn fold(name: &str) -> String {
    name.chars().map(fold_char).collect()
}

/// One character folded.
#[must_use]
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_uppercase();
    }
    let mut up = c.to_uppercase();
    match (up.next(), up.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// Whether two names are the same name under folding.
#[must_use]
pub fn same(a: &str, b: &str) -> bool {
    (a.len() == b.len() && a.eq_ignore_ascii_case(b)) || fold(a) == fold(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding() {
        assert_eq!(fold("Readme.txt"), "README.TXT");
        assert!(same("QOpenSys", "qopensys"));
        assert!(same("Grüße", "GRÜßE"));
        assert!(!same("a", "b"));
        assert_eq!(fold_char('ß'), 'ß');
        assert_eq!(fold("éa"), "ÉA");
    }
}
