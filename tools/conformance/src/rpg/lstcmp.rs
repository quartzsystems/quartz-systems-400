// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compile listings compared with the oracle's. Both sides go through
//! one normalization: the heading, dates, times, user and library are
//! placeholders (ours masked as `rpg-oracle` masks pub400's), the
//! oracle's page padding goes, and every message text (ours are
//! Quartz-written, so they differ by rule) becomes `{text}`, its
//! continuation lines dropped. Everything else must be the same.

use rpg_oracle::listing::{Mask, mask};

/// Our library and user as the kit compiles.
const OUR_LIB: &str = "QGPL";
const OUR_USER: &str = "QSECOFR";

/// Where a message text starts in the source and additional sections.
const DIAG_TEXT: usize = 28;
/// Where it starts in the message summary.
const SUMMARY_TEXT: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sect {
    Other,
    Diags,
    Summary,
    Final,
}

fn is_heading(l: &str) -> bool {
    l.starts_with("1{heading}")
}

fn text_of(l: &str) -> &str {
    l.get(1..).unwrap_or("")
}

/// A diagnostic record: ` *RNFnnnn ss`.
fn is_diag(l: &str) -> bool {
    let b = l.as_bytes();
    b.len() >= 12
        && b[0] == b' '
        && b[1] == b'*'
        && (b[2..5] == *b"RNF" || b[2..5] == *b"RNS")
        && b[5..9].iter().all(u8::is_ascii_digit)
        && b[9] == b' '
        && b[10..12].iter().all(u8::is_ascii_digit)
}

fn is_continuation(l: &str, at: usize) -> bool {
    l.len() > at && l.as_bytes()[..at].iter().all(|b| *b == b' ')
}

/// Normalize a listing; `ours` masks our own names first.
#[must_use]
pub fn normalize(lines: &[String], ours: bool) -> Vec<String> {
    let lines: Vec<String> = if ours {
        let m = Mask {
            libs: vec![OUR_LIB.into()],
            users: vec![OUR_USER.into()],
            systems: Vec::new(),
        };
        mask(lines, &m)
    } else {
        lines.iter().map(|l| l.trim_end().to_string()).collect()
    };
    // The oracle pads each page with blank lines up to its footer.
    let mut kept: Vec<String> = Vec::new();
    for l in lines {
        if is_heading(&l) {
            while kept.last().is_some_and(|k| text_of(k).trim().is_empty()) {
                kept.pop();
            }
        }
        kept.push(l);
    }
    while kept.last().is_some_and(|k| text_of(k).trim().is_empty()) {
        kept.pop();
    }
    let mut out = Vec::new();
    let mut sect = Sect::Other;
    let mut in_text: Option<usize> = None;
    for l in kept {
        let t = text_of(&l).trim_start();
        if t.starts_with("S o u r c e   L i s t i n g") || t.starts_with("A d d i t i o n a l   D i a g") {
            sect = Sect::Diags;
        } else if t.starts_with("M e s s a g e   S u m m a r y") {
            sect = Sect::Summary;
        } else if t.starts_with("F i n a l   S u m m a r y") {
            sect = Sect::Final;
        } else if t.starts_with("* * * * *   E N D   O F   F I N A L") {
            out.push(l);
            in_text = None;
            continue;
        }
        // A text continued on the next page continues after its heading.
        if is_heading(&l) || l[1..].starts_with("Line   <") || l[1..].starts_with("Number ") {
            out.push(l);
            continue;
        }
        if let Some(at) = in_text {
            if is_continuation(&l, at) {
                continue;
            }
            in_text = None;
        }
        let at = match sect {
            Sect::Diags => DIAG_TEXT,
            Sect::Summary => SUMMARY_TEXT,
            _ => 0,
        };
        if at > 0 && is_diag(&l) {
            let head: String = l.chars().take(at).collect();
            out.push(format!("{head}{{text}}"));
            in_text = Some(at);
            continue;
        }
        if sect == Sect::Final && out.last().is_some_and(|p: &String| p.contains("E N D   O F   F I N A L")) {
            out.push(format!("{}{{completion {}}}", &l[..1], severity_in(&l)));
            continue;
        }
        if l.contains("{lib}") || l.contains("{user}") {
            out.push(collapse(&l));
            continue;
        }
        out.push(l);
    }
    out
}

/// Runs of blanks after the indentation as one: names of other
/// lengths shift what follows.
fn collapse(l: &str) -> String {
    let lead = l.len() - text_of(l).trim_start().len();
    let words: Vec<&str> = l[lead..].split(' ').filter(|w| !w.is_empty()).collect();
    format!("{}{}", &l[..lead], words.join(" "))
}

/// The first two-digit word of a completion line: its severity.
fn severity_in(l: &str) -> String {
    text_of(l)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|w| w.len() == 2 && w.bytes().all(|b| b.is_ascii_digit()))
        .unwrap_or("??")
        .to_string()
}

/// The first difference: line number (from 1) and both lines.
#[must_use]
pub fn first_difference(ours: &[String], ibm: &[String]) -> Option<(usize, String, String)> {
    let n = ours.len().max(ibm.len());
    (0..n).find_map(|i| {
        let a = ours.get(i).map_or("<end>", String::as_str);
        let b = ibm.get(i).map_or("<end>", String::as_str);
        (a != b).then(|| (i + 1, a.to_string(), b.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn masks_texts_and_padding() {
        let ibm = v(&[
            "0                          S o u r c e   L i s t i n g",
            " *RNF2318 00      2 000200  First words",
            "                            more words.",
            "      3 x",
            "-",
            "-",
            "1{heading} page 2",
        ]);
        assert_eq!(
            normalize(&ibm, false),
            v(&[
                "0                          S o u r c e   L i s t i n g",
                " *RNF2318 00      2 000200  {text}",
                "      3 x",
                "1{heading} page 2",
            ])
        );
    }
}
