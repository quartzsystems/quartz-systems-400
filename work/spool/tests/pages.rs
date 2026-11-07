// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The page index: one start per page `decode` finds, page ranges that
//! join into the whole stream, and ranges that read as the same pages
//! read inside the whole: after a form feed, an overflow, an absolute
//! vertical move, and pitch and line density changes part-way.

use spool::scs::{self, CSP, Encoder, PAGE, SCD, SLD, decode, index, range};

/// A file whose pages start every way a page can: form feeds, lines
/// past the page length, a move up the page; with the pitch and the
/// line density changed on page 3 and a font command on page 4.
fn mixed() -> Vec<u8> {
    let mut e = Encoder::default();
    e.header(132, 20, 6, 10);
    let line = |e: &mut Encoder, s: &str| {
        e.text(&ccsid::to_ebcdic(s).unwrap());
        e.nl();
    };
    for p in 1..=2 {
        for l in 1..=5 {
            line(&mut e, &format!("PAGE {p} LINE {l}"));
        }
        e.ff();
    }
    // Page 3: 15 cpi and 8 lpi from here on.
    e.out.extend_from_slice(&[CSP, PAGE, 4, SCD, 0, 15]);
    e.out.extend_from_slice(&[CSP, SLD, 2, 9]);
    // Lines past the page length run onto page 4.
    for l in 1..=24 {
        line(&mut e, &format!("RUN {l}"));
    }
    // A font command, then a column and a move back up: page 5.
    e.out
        .extend_from_slice(&[CSP, 0xD1, 7, 0x05, 0x00, 0x0B, 0x00, 0x00, 0x00]);
    e.column(40);
    line(&mut e, "INDENTED");
    e.line(3);
    line(&mut e, "AFTER MOVE UP");
    e.out
}

#[test]
fn one_start_per_page() {
    let data = mixed();
    let (pages, _) = decode(&data);
    let idx = index(&data);
    assert_eq!(idx.len(), pages.len());
    assert_eq!(pages.len(), 5, "{pages:?}");
    assert_eq!(idx[0].offset, 0);
    assert!(idx.windows(2).all(|w| w[0].offset < w[1].offset));
}

#[test]
fn ranges_join_into_the_whole() {
    let data = mixed();
    let n = index(&data).len();
    // Page by page, and in batches of two: after each range's state,
    // the bytes join into the file.
    for batch in [1, 2, 3] {
        let mut joined = Vec::new();
        let mut first = 1;
        while first <= n {
            let last = (first + batch - 1).min(n);
            let r = range(&data, first, last).unwrap();
            let skip = index(&data)[first - 1].prefix().len();
            joined.extend_from_slice(&r[skip..]);
            first = last + 1;
        }
        assert_eq!(joined, data, "batches of {batch}");
    }
    assert_eq!(range(&data, 1, 0), None);
    assert_eq!(range(&data, 0, 1), None);
    assert_eq!(range(&data, n + 1, n + 1), None);
    assert_eq!(range(&data, 2, 99), range(&data, 2, n));
}

#[test]
fn a_range_reads_as_the_same_pages_do_in_the_whole() {
    let data = mixed();
    let (whole, _) = decode(&data);
    let idx = index(&data);
    for first in 1..=whole.len() {
        for last in first..=whole.len() {
            let r = range(&data, first, last).unwrap();
            let (pages, fmt) = decode(&r);
            assert_eq!(pages, whole[first - 1..last], "pages {first} to {last}");
            // The format in effect at the range's end is the whole's there.
            let end = idx.get(last).map_or(data.len(), |p| p.offset);
            assert_eq!(fmt, decode(&data[..end]).1, "pages {first} to {last}");
        }
    }
    // Page 3 starts in 15 cpi and 8 lpi; its state says so.
    let (_, fmt) = decode(&range(&data, 4, 4).unwrap());
    assert_eq!((fmt.cpi, fmt.lpi, fmt.len), (15, 8, 20));
    // The font command is in page 5's state.
    assert!(idx[4].formats.windows(2).any(|w| w == [CSP, 0xD1]));
    assert!(whole[4].lines.values().any(|l| l.trim() == "AFTER MOVE UP"));
}

#[test]
fn a_file_without_pages() {
    assert!(index(&[]).is_empty());
    assert_eq!(range(&[], 1, 0), None);
    // Format commands alone print nothing.
    let mut e = Encoder::default();
    e.header(80, 66, 6, 10);
    assert!(index(&e.out).is_empty());
    let _ = scs::FF;
}

#[test]
fn presentation_position_codes_are_the_published_ones() {
    assert_eq!(
        (scs::AHPP, scs::RHPP, scs::AVPP, scs::RVPP),
        (0xC0, 0xC8, 0xC4, 0x4C)
    );
    // X'34C4' 5 is line 5; X'344C' 2 is two lines further down.
    let data = [0x34, 0xC4, 5, 0xC1, 0x15, 0x34, 0x4C, 2, 0xC2];
    let (pages, _) = decode(&data);
    assert_eq!(pages[0].lines.keys().copied().collect::<Vec<_>>(), [5, 8]);
}
