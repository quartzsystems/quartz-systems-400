// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sshd_config` patterns: `*` and `?` wildcards, comma lists with `!`
//! negation, and the `USER@HOST` form of `AllowUsers`.

/// Whether `text` matches the wildcard pattern `pat`, ignoring case.
#[must_use]
pub fn wildcard(text: &str, pat: &str) -> bool {
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let p: Vec<char> = pat.to_lowercase().chars().collect();
    let (mut ti, mut pi) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            ti += 1;
            pi += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// A comma list with `!` negation: `Some(true)` matched, `Some(false)`
/// matched a negated entry, `None` matched nothing.
#[must_use]
pub fn list(text: &str, patterns: &str) -> Option<bool> {
    let mut hit = None;
    for p in patterns.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(neg) = p.strip_prefix('!') {
            if wildcard(text, neg) {
                return Some(false);
            }
        } else if wildcard(text, p) {
            hit = Some(true);
        }
    }
    hit
}

/// One `AllowUsers`/`DenyUsers` entry: `USER` or `USER@HOST`, the host
/// matched against the client's address.
#[must_use]
pub fn user_entry(user: &str, peer: &str, entry: &str) -> bool {
    match entry.split_once('@') {
        Some((u, h)) => wildcard(user, u) && wildcard(peer, h),
        None => wildcard(user, entry),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard("QSTEST", "qs*"));
        assert!(wildcard("abc", "a?c"));
        assert!(wildcard("", "*"));
        assert!(!wildcard("abc", "a?"));
        assert!(wildcard("10.0.0.5", "10.0.*"));
        assert_eq!(list("bob", "a*,!bob,b*"), Some(false));
        assert_eq!(list("bill", "a*,!bob,b*"), Some(true));
        assert_eq!(list("carl", "a*,b*"), None);
        assert!(user_entry("QSTEST", "10.1.2.3", "qstest@10.1.*"));
        assert!(!user_entry("QSTEST", "10.2.2.3", "qstest@10.1.*"));
    }
}
