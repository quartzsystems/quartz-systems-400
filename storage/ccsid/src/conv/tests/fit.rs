// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Records of mixed, double-byte and single-byte text.

use crate::conv::*;

/// Mixed text with blanks, kana, ASCII and kanji.
const TEXT: &str = "QS/400 は日本語を変換します。ｶﾀｶﾅ ABC 東京都　千代田区x";
/// Mixed text with no blanks, so a record's trailing blanks are its pad.
const DENSE: &str = "日本語をQS四〇〇変換します。ABC漢字かな";

/// Whether a mixed record's shifts balance with no pair cut.
fn balanced(r: &[u8]) -> bool {
    let mut dbl = false;
    let mut i = 0;
    while i < r.len() {
        match r[i] {
            SO if !dbl => dbl = true,
            SI if dbl => dbl = false,
            SO | SI => return false,
            _ if dbl => {
                if i + 1 >= r.len() || matches!(r[i + 1], SO | SI) {
                    return false;
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    !dbl
}

/// The longest prefix of `text` whose encoding fits `len`, encoded.
fn longest(c: u16, text: &str, len: usize) -> (Vec<u8>, bool) {
    let chars: Vec<char> = text.chars().collect();
    let mut best = Vec::new();
    for k in 0..=chars.len() {
        let s: String = chars[..k].iter().collect();
        let b = from_str(c, &s).unwrap();
        if b.len() > len {
            return (best, true);
        }
        best = b;
    }
    (best, false)
}

#[test]
fn every_cut_of_mixed_text() {
    for c in [930, 939, 5026, 5035] {
        let all = from_str(c, TEXT).unwrap();
        for len in 0..=all.len() + 3 {
            let f = fit(c, &all, len, 0x40);
            assert_eq!(f.bytes.len(), len, "{c} {len}");
            assert!(balanced(&f.bytes), "{c} {len} {:02X?}", f.bytes);
            let (want, cut_) = longest(c, TEXT, len);
            assert_eq!(f.cut, cut_, "{c} {len}");
            assert_eq!(&f.bytes[..want.len()], &want[..], "{c} {len}");
            assert!(f.bytes[want.len()..].iter().all(|&b| b == 0x40), "{c} {len}");
            let u = cut(c, &all, len);
            assert_eq!((u.bytes, u.cut), (want, cut_));
        }
    }
}

#[test]
fn every_wrap_of_mixed_text() {
    for c in [930, 939] {
        let all = from_str(c, DENSE).unwrap();
        for len in 4..=all.len() + 2 {
            let recs = wrap(c, &all, len, 0x40);
            let mut text = String::new();
            for r in &recs {
                assert_eq!(r.len(), len);
                assert!(balanced(r), "{c} {len} {r:02X?}");
                let t = to_string(c, r).unwrap();
                // A record that opens with a double-byte character opens with SO.
                if let Some(ch) = t.chars().next()
                    && from_str(c, &ch.to_string()).unwrap()[0] == SO
                {
                    assert_eq!(r[0], SO, "{c} {len}");
                }
                text.push_str(t.trim_end());
            }
            assert_eq!(text, DENSE, "{c} {len}");
        }
    }
}

#[test]
fn tiny_records_drop_what_cannot_fit() {
    let all = from_str(939, "aあb").unwrap();
    assert_eq!(wrap(939, &all, 3, 0x40), [vec![0x81, 0x40, 0x40], vec![0x82, 0x40, 0x40]]);
    assert_eq!(fit(939, &all, 3, 0x40).bytes, [0x81, 0x40, 0x40]);
}

#[test]
fn balanced_input_that_fits_is_kept() {
    // An empty SO SI pair stays as written.
    let r = [0xC1, SO, SI, 0xC2];
    assert_eq!(fit(939, &r, 6, 0x40).bytes, [0xC1, SO, SI, 0xC2, 0x40, 0x40]);
    // An unclosed run gets its SI.
    let open = [SO, 0x44, 0x81];
    assert_eq!(fit(939, &open, 6, 0x40).bytes, [SO, 0x44, 0x81, SI, 0x40, 0x40]);
    assert_eq!(fit(939, &open, 3, 0x40), Fitted { bytes: vec![0x40; 3], cut: true });
}

#[test]
fn double_byte_cuts_between_pairs() {
    let all = from_str(300, "東京都千代田区").unwrap();
    for len in 0..=all.len() + 3 {
        let f = fit(300, &all, len, 0);
        assert_eq!(f.bytes.len(), len);
        let n = len.min(all.len()) & !1;
        assert_eq!(&f.bytes[..n], &all[..n]);
        assert!(f.bytes[n..].iter().all(|&b| b == 0x40), "double-byte spaces");
        assert_eq!(f.cut, len < all.len());
    }
    let recs = wrap(300, &all, 5, 0);
    assert_eq!(recs.len(), 4);
    let data: Vec<u8> = recs.concat().into_iter().filter(|&b| b != 0x40).collect();
    assert_eq!(to_string(300, &data).unwrap(), "東京都千代田区");
}

#[test]
fn single_byte_is_unchanged() {
    let all = from_str(37, "PGM PARM(&A)").unwrap();
    for len in 0..=all.len() + 2 {
        let f = fit(37, &all, len, 0x40);
        let mut want = all[..len.min(all.len())].to_vec();
        want.resize(len, 0x40);
        assert_eq!(f.bytes, want);
        assert_eq!(f.cut, len < all.len());
        let w: Vec<Vec<u8>> = all
            .chunks(len.max(1))
            .map(|c| {
                let mut r = c.to_vec();
                r.resize(len.max(1), 0);
                r
            })
            .collect();
        assert_eq!(wrap(37, &all, len.max(1), 0), w);
    }
    assert_eq!(wrap(819, b"abcdef", 4, 0x20), [b"abcd".to_vec(), b"ef  ".to_vec()]);
}
