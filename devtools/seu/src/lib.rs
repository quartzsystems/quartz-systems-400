// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Source Entry Utility's editing model: a source member's records
//! with their sequence numbers and dates ([`buffer`]), the line
//! commands typed over the sequence numbers and their pending blocks
//! ([`line`]), the `SEU==>` command line ([`cmdline`]), find and change
//! ([`find`]), the source types with their format lines and prompts
//! ([`srctype`], [`prompt`]), the session defaults ([`defaults`]), the
//! Exit display's choices ([`exit`]) and the session that ties them to
//! the display's rows ([`session`]). Nothing here touches the store or
//! a screen: the job's driver reads and writes the member through the
//! storage service and draws the rows through display files.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
// Column tables and spec fields read best with short names.
#![allow(clippy::many_single_char_names, clippy::similar_names)]
#![cfg_attr(test, allow(clippy::unreadable_literal))]

pub mod buffer;
pub mod cl;
pub mod cmdline;
pub mod defaults;
#[cfg(feature = "display")]
pub mod display;
pub mod exit;
pub mod find;
pub mod line;
pub mod prompt;
pub mod session;
pub mod srctype;

pub use buffer::{Buffer, Record};
pub use session::{Mode, Session};

/// A sequence number as SEU shows it: `nnnn.nn` from hundredths.
#[must_use]
pub fn seq_text(seq: u32) -> String {
    format!("{:04}.{:02}", seq / 100 % 10_000, seq % 100)
}

/// A typed sequence number (`12`, `12.5`, `0012.50`) in hundredths.
#[must_use]
pub fn parse_seq(text: &str) -> Option<u32> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
    if whole.len() > 4 || frac.len() > 2 {
        return None;
    }
    if !whole.chars().chain(frac.chars()).all(|c| c.is_ascii_digit()) {
        return None;
    }
    let w: u32 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
    let mut f: u32 = if frac.is_empty() { 0 } else { frac.parse().ok()? };
    if frac.len() == 1 {
        f *= 10;
    }
    Some(w * 100 + f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_numbers() {
        assert_eq!(seq_text(100), "0001.00");
        assert_eq!(seq_text(999_999), "9999.99");
        assert_eq!(parse_seq("12"), Some(1200));
        assert_eq!(parse_seq("12.5"), Some(1250));
        assert_eq!(parse_seq("0012.05"), Some(1205));
        assert_eq!(parse_seq("x"), None);
        assert_eq!(parse_seq("12345"), None);
    }
}
