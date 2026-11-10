// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parameters of a call as the job sends them: one byte vector per
//! API parameter (the error code left out), in order. An optional
//! parameter not passed is missing or empty.

use crate::fmt::text;

/// A call's parameters.
#[derive(Clone, Copy, Debug)]
pub struct Parms<'a>(pub &'a [Vec<u8>]);

impl Parms<'_> {
    /// Parameter `i`'s bytes, when passed.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&[u8]> {
        self.0.get(i).map(Vec::as_slice).filter(|b| !b.is_empty())
    }

    /// Whether parameter `i` was passed.
    #[must_use]
    pub fn has(&self, i: usize) -> bool {
        self.get(i).is_some()
    }

    /// Parameter `i` as text, trailing blanks dropped.
    #[must_use]
    pub fn text(&self, i: usize) -> String {
        self.get(i).map(text::text).unwrap_or_default()
    }

    /// Parameter `i`'s raw bytes, `n` long (blank padded).
    #[must_use]
    pub fn raw(&self, i: usize, n: usize) -> Vec<u8> {
        text::raw_at(self.get(i).unwrap_or(&[]), 0, n)
    }

    /// Parameter `i` as a 4-byte integer.
    #[must_use]
    pub fn bin4(&self, i: usize) -> i32 {
        self.get(i).map_or(0, |b| text::bin4(b, 0))
    }

    /// Parameter `i` as an 8-byte integer, when passed.
    #[must_use]
    pub fn bin8(&self, i: usize) -> Option<i64> {
        self.get(i).map(|b| text::bin8(b, 0))
    }

    /// Parameter `i`'s first character, when passed.
    #[must_use]
    pub fn ch(&self, i: usize) -> Option<char> {
        self.get(i).map(|b| text::ch(b, 0))
    }
}
