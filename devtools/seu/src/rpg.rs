// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG IV source as SEU checks it: the member read and parsed by the
//! compiler's front end, and the first error that falls on a record the
//! last key added or changed, as its message identifier and text.

use rpgfront::reader::Options;
use rpgfront::source::Record;
use rpgfront::{Member, NoIncludes};

/// The lowest severity SEU reports; below it a line is not in error.
const ERROR: u8 = 20;

/// Diagnostics about other members: the checker reads only this one.
const ELSEWHERE: &[&str] = &["RNF0273", "RNF0024"];

/// The first error on a touched record: its index and the message, or
/// `None` when the touched records are clean.
#[must_use]
pub fn check(records: &[(u32, u32, String)], touched: &[usize]) -> Option<(usize, String)> {
    // Each record's index is its sequence number here, so a diagnostic
    // leads back to the record exactly.
    let member = Member {
        lib: "*LIBL".into(),
        file: "QRPGLESRC".into(),
        name: "SEU".into(),
        ccsid: 37,
        records: records
            .iter()
            .enumerate()
            .map(|(i, (_, date, text))| Record {
                seq: format!("{i:06}"),
                date: format!("{date:06}"),
                text: text.clone(),
            })
            .collect(),
    };
    let opts = Options {
        defines: rpgc::compile::predefined(&rpgc::Options::default()),
        ..Options::default()
    };
    let (read, _, diags) = rpgfront::front(member, &mut NoIncludes, &opts);
    diags
        .iter()
        .filter(|d| d.severity >= ERROR && !ELSEWHERE.contains(&d.msgid))
        .find_map(|d| {
            let line = read
                .lines
                .iter()
                .find(|l| l.no == d.at.line && l.member == 0 && l.depth == 0)?;
            let i: usize = line.seq.parse().ok()?;
            touched
                .contains(&i)
                .then(|| (i, format!("{} {}", d.msgid, rpgc::msgs::text(d))))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(lines: &[&str]) -> Vec<(u32, u32, String)> {
        lines
            .iter()
            .enumerate()
            .map(|(i, l)| (u32::try_from(i + 1).unwrap() * 100, 0, (*l).to_string()))
            .collect()
    }

    #[test]
    fn a_clean_member_has_no_error() {
        let r = rec(&["**FREE", "dcl-s n int(10);", "n = 1;", "*inlr = *on;"]);
        assert_eq!(check(&r, &[0, 1, 2, 3]), None);
    }

    #[test]
    fn the_error_is_reported_on_the_line_typed() {
        let r = rec(&["**FREE", "dcl-s n int(10);", "n = = 1;", "*inlr = *on"]);
        let (at, msg) = check(&r, &[2]).expect("an error");
        assert_eq!(at, 2);
        assert!(msg.starts_with("RNF5358 "), "{msg}");
        // The last line lacks its semicolon.
        let (at, msg) = check(&r, &[3]).expect("an error");
        assert_eq!(at, 3);
        assert_eq!(
            msg,
            "RNF5507 The free-form statement has no ending semicolon."
        );
        // Errors on records not touched are not this key's.
        assert_eq!(check(&r, &[0, 1]), None);
    }

    #[test]
    fn copy_members_are_not_errors_here() {
        let r = rec(&["**FREE", "/copy qrpglesrc,protos", "*inlr = *on;"]);
        assert_eq!(check(&r, &[1]), None);
    }

    #[test]
    fn fixed_form_operation_codes_are_checked() {
        let r = rec(&["     C                   BADOP                                        "]);
        let (at, msg) = check(&r, &[0]).expect("an error");
        assert_eq!(at, 0);
        assert!(msg.starts_with("RNF5014"), "{msg}");
    }
}
