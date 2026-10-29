// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The text utilities: `cut expand grep head iconv od paste sed sort
//! split tail tee tr uniq wc`.

pub mod cut;
pub mod expand;
pub mod grep;
pub mod head;
pub mod iconv;
pub mod od;
pub mod paste;
pub mod sed;
pub mod sort;
pub mod split;
pub mod tail;
pub mod tee;
pub mod tr;
pub mod uniq;
pub mod wc;

/// A `cut`-style list of positions: `1,3-5,7-,-2`, 1-based.
#[derive(Clone, Debug, Default)]
pub struct Positions {
    ranges: Vec<(usize, Option<usize>)>,
}

impl Positions {
    /// Parse a list; `None` when it is not one.
    pub fn parse(s: &str) -> Option<Self> {
        let mut ranges = Vec::new();
        for item in s.split([',', ' ']) {
            if item.is_empty() {
                continue;
            }
            let (lo, hi) = match item.split_once('-') {
                None => {
                    let n: usize = item.parse().ok()?;
                    (n, Some(n))
                }
                Some((a, b)) => {
                    let lo = if a.is_empty() { 1 } else { a.parse().ok()? };
                    let hi = if b.is_empty() {
                        None
                    } else {
                        Some(b.parse().ok()?)
                    };
                    (lo, hi)
                }
            };
            if lo == 0 || hi.is_some_and(|h| h == 0 || h < lo) {
                return None;
            }
            ranges.push((lo, hi));
        }
        if ranges.is_empty() {
            return None;
        }
        Some(Self { ranges })
    }

    /// Is a 1-based position selected?
    pub fn has(&self, n: usize) -> bool {
        self.ranges
            .iter()
            .any(|(lo, hi)| n >= *lo && hi.is_none_or(|h| n <= h))
    }
}
