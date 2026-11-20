// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The reference codes of this IPL as `slic` posts them, the history the
//! service console's `srcs` shows: `slic` writes [`PATH`] whole at each
//! code, one line a code (`seq`, Unix milliseconds, the code, its text,
//! tab-separated), and the server reads it. A code's number is the IPL's
//! count in the LIC record shifted 24 bits, plus its place in the IPL
//! from 1: never reused, ascending from one IPL to the next.

use qsi::base::{Refcode, RefcodeList};

/// The file.
pub const PATH: &str = "/run/qs400/srchist";

/// The most codes a page holds.
pub const MAX_PAGE: u32 = 256;

/// The number of the `index`th code (from 0) of IPL `ipl`.
#[must_use]
pub fn seq(ipl: u32, index: usize) -> u64 {
    (u64::from(ipl) << 24) + u64::try_from(index).unwrap_or(0) + 1
}

/// One code's line.
#[must_use]
pub fn line(seq: u64, time_ms: u64, code: &str, text: &str) -> String {
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    format!("{seq}\t{time_ms}\t{}\t{}\n", clean(code), clean(text))
}

/// The codes in the file's text, oldest first; a line that does not
/// read is passed over.
#[must_use]
pub fn parse(text: &str) -> Vec<Refcode> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.splitn(4, '\t');
            let seq = it.next()?.parse().ok()?;
            let time_ms = it.next()?.parse().ok()?;
            let code = it.next()?.to_string();
            let text = it.next().unwrap_or("").to_string();
            Some(Refcode {
                seq,
                code,
                words: Vec::new(),
                text,
                time_ms,
            })
        })
        .collect()
}

/// A page of `codes` (oldest first): those numbered below `before`
/// (from the newest without one), newest first, at most `limit`.
#[must_use]
pub fn page(codes: &[Refcode], before: Option<u64>, limit: u32) -> RefcodeList {
    let limit = usize::try_from(limit.clamp(1, MAX_PAGE)).unwrap_or(1);
    let mut picked: Vec<Refcode> = codes
        .iter()
        .rev()
        .filter(|c| before.is_none_or(|b| c.seq < b))
        .take(limit + 1)
        .cloned()
        .collect();
    let more = if picked.len() > limit {
        picked.truncate(limit);
        picked.last().map(|c| c.seq)
    } else {
        None
    };
    RefcodeList {
        codes: picked,
        more,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_read_back_and_page() {
        let mut text = String::new();
        for i in 0..5 {
            text.push_str(&line(
                seq(7, i),
                1000 + i as u64,
                "C6001090",
                "IPL\tcomplete",
            ));
        }
        text.push_str("junk\n");
        let codes = parse(&text);
        assert_eq!(codes.len(), 5);
        assert_eq!(codes[0].seq, (7 << 24) + 1);
        assert_eq!(codes[0].text, "IPL complete");
        let p = page(&codes, None, 2);
        assert_eq!(p.codes.len(), 2);
        assert_eq!(p.codes[0].seq, (7 << 24) + 5);
        assert_eq!(p.more, Some((7 << 24) + 4));
        let p = page(&codes, p.more, 10);
        assert_eq!(p.codes.len(), 3);
        assert_eq!(p.more, None);
        assert!(seq(8, 0) > seq(7, 1000));
    }
}
