// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The queue-state answer (commands 03 and 04). RFC 1179 leaves its
//! form to the server; this one is ours, a heading and a line per
//! spooled file in the order they print, in the columns `lpq` users
//! know.

use std::fmt::Write as _;

/// One spooled file on the queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Its owner.
    pub owner: String,
    /// Its job number.
    pub job: u32,
    /// Its name.
    pub file: String,
    /// Its spooled file number.
    pub number: u32,
    /// Its status (`RDY`, `HLD`, ...).
    pub status: String,
    /// Its size, bytes.
    pub size: u64,
}

fn rank(n: usize) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// The answer for `queue`: every entry, or those whose owner or job
/// number `list` names.
#[must_use]
pub fn text(queue: &str, entries: &[Entry], long: bool, list: &[String]) -> String {
    let picked: Vec<&Entry> = entries
        .iter()
        .filter(|e| {
            list.is_empty()
                || list
                    .iter()
                    .any(|w| w.eq_ignore_ascii_case(&e.owner) || w.parse::<u32>().is_ok_and(|n| n == e.job))
        })
        .collect();
    let mut out = String::new();
    if picked.is_empty() {
        let _ = writeln!(out, "{queue}: no entries");
        return out;
    }
    let _ = writeln!(out, "{queue}: {} {}", picked.len(), if picked.len() == 1 { "entry" } else { "entries" });
    if long {
        for (i, e) in picked.iter().enumerate() {
            let _ = writeln!(out, "\n{}: {:<10} [job {:06}]", e.owner, rank(i + 1), e.job);
            let _ = writeln!(out, "        {:<10} {:>6} {:>4} {} bytes", e.file, e.number, e.status, e.size);
        }
    } else {
        let _ = writeln!(out, "Rank   Owner      Job    File       Status  Total Size");
        for (i, e) in picked.iter().enumerate() {
            let _ = writeln!(
                out,
                "{:<6} {:<10} {:06} {:<10} {:<7} {} bytes",
                rank(i + 1),
                e.owner,
                e.job,
                e.file,
                e.status,
                e.size
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(owner: &str, job: u32, file: &str) -> Entry {
        Entry {
            owner: owner.into(),
            job,
            file: file.into(),
            number: 1,
            status: "RDY".into(),
            size: 120,
        }
    }

    #[test]
    fn short_and_long() {
        let q = [e("QTMPLPD", 12, "NOTES"), e("ANNE", 14, "INVOICE")];
        let s = text("QGPL/QPRINT", &q, false, &[]);
        assert_eq!(
            s,
            "QGPL/QPRINT: 2 entries\nRank   Owner      Job    File       Status  Total Size\n1st    QTMPLPD    000012 NOTES      RDY     120 bytes\n2nd    ANNE       000014 INVOICE    RDY     120 bytes\n"
        );
        let s = text("Q", &q, true, &["anne".into()]);
        assert!(s.starts_with("Q: 1 entry\n\nANNE: 1st") && s.contains("INVOICE"), "{s}");
        assert_eq!(text("Q", &q, false, &["99".into()]), "Q: no entries\n");
        assert_eq!(rank(11), "11th");
        assert_eq!(rank(22), "22nd");
    }
}
