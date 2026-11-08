// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option indicators in positions 7-16, the one check `CRTDSPF`,
//! `CRTPRTF` and the front end share: position 7 `A`, `O` or blank,
//! then up to three slots of `N` or blank and two digits, 01-99.
//! Positions 8-16 starting with `*` (`*DSn`) are the display file's.

use crate::diag::{Diagnostics, ids};
use crate::source::Source;

/// Whether position 7 joins the line to a condition group correctly.
#[must_use]
pub fn joiner_ok(c: char) -> bool {
    matches!(c, ' ' | 'A' | 'O')
}

/// The indicators of positions 8-16 (`text`), each number and whether
/// it must be on; a malformed field is reported and gives what was read
/// before it.
pub fn indicators(text: &str, line: usize, diags: &mut Diagnostics) -> Vec<(u8, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for slot in chars.chunks(3) {
        if slot.iter().all(|c| *c == ' ') {
            continue;
        }
        let on = match slot.first() {
            Some('N') => false,
            Some(' ') => true,
            _ => {
                diags.push(ids::bad_conditioning(line, text));
                return out;
            }
        };
        let digits: String = slot.iter().skip(1).collect();
        match digits.parse::<u8>() {
            Ok(n) if (1..=99).contains(&n) => out.push((n, on)),
            Ok(n) => diags.push(ids::indicator_range(line, n)),
            Err(_) => {
                diags.push(ids::bad_conditioning(line, text));
                return out;
            }
        }
    }
    out
}

/// Every code line's positions 7-16 checked: what the front end reports
/// for a display file before the display-file compiler runs.
pub fn check_lines(source: &Source, diags: &mut Diagnostics) {
    for (i, l) in source.lines().iter().enumerate() {
        if l.blank(1, crate::RECORD_LEN) || l.ch(7) == '*' {
            continue;
        }
        if !joiner_ok(l.ch(7)) {
            diags.push(ids::bad_conditioning(i, &l.ch(7).to_string()));
        }
        let text = l.cols(8, 16);
        if !text.starts_with('*') {
            indicators(&text, i, diags);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids_of(text: &str) -> Vec<&'static str> {
        let mut d = Diagnostics::default();
        indicators(text, 0, &mut d);
        d.as_slice().iter().map(|d| d.id).collect::<Vec<_>>()
    }

    #[test]
    fn slots() {
        let mut d = Diagnostics::default();
        assert_eq!(
            indicators(" 50N51 99", 0, &mut d),
            [(50, true), (51, false), (99, true)]
        );
        assert!(d.as_slice().is_empty());
        assert_eq!(ids_of("51       "), ["DDS6101"]);
        assert_eq!(ids_of(" 5X      "), ["DDS6101"]);
        assert_eq!(ids_of(" 00      "), ["DDS6102"]);
        assert!(ids_of("         ").is_empty());
    }
}
