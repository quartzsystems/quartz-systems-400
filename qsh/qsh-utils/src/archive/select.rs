// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Choosing members by pattern: a shell pattern selects the members it
//! matches and everything below a directory it matches.

use qsh::pattern;

/// The patterns of a list or extract.
pub struct Select {
    pats: Vec<String>,
    used: Vec<bool>,
    /// Select what the patterns do not match (`-c`).
    pub complement: bool,
    /// Each pattern selects one member at most (`-n`).
    pub first_only: bool,
    /// Patterns are names, not shell patterns (`tar`).
    pub literal: bool,
}

impl Select {
    /// Select by these patterns (none selects everything).
    pub fn new(pats: &[String]) -> Self {
        Self {
            pats: pats
                .iter()
                .map(|p| {
                    let t = p.trim_end_matches('/');
                    if t.is_empty() { p.clone() } else { t.to_string() }
                })
                .collect(),
            used: vec![false; pats.len()],
            complement: false,
            first_only: false,
            literal: false,
        }
    }

    fn hit(&self, pat: &str, name: &str) -> bool {
        let one = |s: &str| {
            if self.literal {
                s == pat
            } else {
                pattern::matches(&pattern::plain(pat), s, false)
            }
        };
        if one(name) {
            return true;
        }
        name.char_indices()
            .filter(|(_, c)| *c == '/')
            .any(|(i, _)| i > 0 && one(&name[..i]))
    }

    /// Is a member selected?
    pub fn take(&mut self, name: &str) -> bool {
        if self.pats.is_empty() {
            return true;
        }
        let mut any = false;
        for i in 0..self.pats.len() {
            if self.first_only && self.used[i] {
                continue;
            }
            if self.hit(&self.pats[i], name) {
                self.used[i] = true;
                any = true;
                if self.first_only {
                    break;
                }
            }
        }
        any != self.complement
    }

    /// Patterns that selected nothing.
    pub fn unmatched(&self) -> Vec<&str> {
        if self.complement {
            return Vec::new();
        }
        self.pats
            .iter()
            .zip(&self.used)
            .filter(|(_, u)| !**u)
            .map(|(p, _)| p.as_str())
            .collect()
    }
}
