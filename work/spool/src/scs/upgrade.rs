// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data spooled before the published presentation position codes. Until
//! version 2 of the `*SPLF` attributes the encoder wrote absolute
//! vertical as `X'344C'`, which is relative vertical (`RVPP`); it never
//! wrote relative vertical. [`published_pp`] rewrites such data.

use super::{AVPP, CSP, PP, RVPP, TRN};

/// The offsets of the subcommand byte of each `PP` command, walking the
/// stream as the reader does: `TRN` data and the parameters of `X'2B'`
/// commands are skipped, so a `X'34'` byte inside them is not a command.
fn pp_subcommands(data: &[u8]) -> Vec<usize> {
    let at = |i: usize| usize::from(data.get(i).copied().unwrap_or(0));
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            PP => {
                if i < data.len() {
                    out.push(i);
                }
                i += 2;
            }
            TRN => i += 1 + at(i),
            CSP => i += 1 + at(i + 1).max(1),
            _ => {}
        }
    }
    out
}

/// `data` as written before the published codes, rewritten with them:
/// each `X'344C'` (the old absolute vertical) becomes `X'34C4'`. `None`
/// when nothing changes: no `X'344C'`, or a `X'34C4'` anywhere, which
/// the old encoder never wrote, so the data already uses the published
/// codes (written since, or received from another system).
#[must_use]
pub fn published_pp(data: &[u8]) -> Option<Vec<u8>> {
    let subs = pp_subcommands(data);
    let codes = || subs.iter().map(|&i| data[i]);
    if codes().any(|c| c == AVPP) || !codes().any(|c| c == RVPP) {
        return None;
    }
    let mut out = data.to_vec();
    for &i in &subs {
        if out[i] == RVPP {
            out[i] = AVPP;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_absolute_moves_become_published() {
        // Line 5, "A", then line 9, "B".
        let old = [0x34, 0x4C, 5, 0xC1, 0x15, 0x34, 0x4C, 9, 0xC2];
        let new = published_pp(&old).expect("rewritten");
        assert_eq!(new, [0x34, 0xC4, 5, 0xC1, 0x15, 0x34, 0xC4, 9, 0xC2]);
        let (pages, _) = super::super::decode(&new);
        assert_eq!(pages[0].lines.keys().copied().collect::<Vec<_>>(), [5, 9]);
        assert_eq!(published_pp(&new), None, "once only");
    }

    #[test]
    fn transparent_data_and_parameters_stay() {
        // TRN of 3 bytes holding 34 4C 01; SHF whose parameter is X'34';
        // a CSP with length 3 holding 34 4C; then one real old AVPP.
        let old = [0x35, 3, 0x34, 0x4C, 1, 0x2B, 0xC1, 2, 0x34, 0x2B, 0xD2, 3, 0x34, 0x4C, 0x34, 0x4C, 7];
        let new = published_pp(&old).expect("rewritten");
        let mut want = old;
        want[15] = 0xC4;
        assert_eq!(new, want);
    }

    #[test]
    fn published_or_plain_data_is_left() {
        assert_eq!(published_pp(&[0xC1, 0x15, 0xC2]), None);
        // A published AVPP anywhere: the file is already right.
        assert_eq!(published_pp(&[0x34, 0xC4, 5, 0x34, 0x4C, 1]), None);
        // Horizontal moves only.
        assert_eq!(published_pp(&[0x34, 0xC0, 5, 0xC1]), None);
        // A truncated command at the end.
        assert_eq!(published_pp(&[0xC1, 0x34]), None);
    }
}
