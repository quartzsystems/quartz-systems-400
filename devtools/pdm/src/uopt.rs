// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User-defined options: one- or two-character codes with a command,
//! kept in an option file (`QGPL/QAUOOPT` by default), one record per
//! option: the code in columns 1-2, the command from column 4.

use std::fmt::Write;

use crate::PdmError;
use crate::option::{ListKind, find};

/// One user-defined option.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserOption {
    /// The code typed on a list.
    pub code: String,
    /// The command, with substitution letters.
    pub command: String,
}

/// Whether a code can be a user-defined option: one or two letters
/// or digits, not one of PDM's own options on any list.
///
/// # Errors
///
/// The code is blank, too long, has other characters or is taken.
pub fn check_code(code: &str) -> Result<String, PdmError> {
    let c = code.trim().to_ascii_uppercase();
    let ok = !c.is_empty()
        && c.len() <= 2
        && c.chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "$#@".contains(ch));
    let taken = [ListKind::Library, ListKind::Object, ListKind::Member]
        .iter()
        .any(|l| find(*l, &c).is_some());
    if ok && !taken {
        Ok(c)
    } else {
        Err(PdmError::BadCode(c))
    }
}

/// The options in a member's text, by code; blank and malformed lines
/// skipped, a repeated code keeping its first command.
#[must_use]
pub fn parse(text: &str) -> Vec<UserOption> {
    let mut out: Vec<UserOption> = Vec::new();
    for line in text.lines() {
        let code = line.get(..2).unwrap_or(line).trim().to_ascii_uppercase();
        let command = line.get(2..).unwrap_or("").trim().to_string();
        if code.is_empty() || command.is_empty() || out.iter().any(|o| o.code == code) {
            continue;
        }
        out.push(UserOption { code, command });
    }
    out.sort_by(|a, b| a.code.cmp(&b.code));
    out
}

/// The member's text for a set of options.
#[must_use]
pub fn to_text(options: &[UserOption]) -> String {
    let mut sorted = options.to_vec();
    sorted.sort_by(|a, b| a.code.cmp(&b.code));
    sorted.iter().fold(String::new(), |mut s, o| {
        let _ = writeln!(s, "{:<2} {}", o.code, o.command);
        s
    })
}

/// The options a new option file starts with.
#[must_use]
pub fn shipped() -> Vec<UserOption> {
    [
        ("C", "CALL PGM(&L/&N)"),
        ("CC", "CHGCURLIB CURLIB(&L)"),
        ("DM", "DSPMSG"),
        ("JL", "DSPJOBLOG"),
        ("SP", "WRKSPLF"),
        ("WJ", "WRKJOB"),
        ("WO", "WRKOBJ OBJ(&L/&N)"),
        ("WS", "WRKUSRJOB"),
    ]
    .iter()
    .map(|(c, cmd)| UserOption {
        code: (*c).to_string(),
        command: (*cmd).to_string(),
    })
    .collect()
}

/// The option with a code.
#[must_use]
pub fn lookup<'a>(options: &'a [UserOption], code: &str) -> Option<&'a UserOption> {
    options
        .iter()
        .find(|o| o.code.eq_ignore_ascii_case(code.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert_eq!(check_code("c"), Ok("C".into()));
        assert_eq!(check_code("WO"), Ok("WO".into()));
        assert!(check_code("").is_err());
        assert!(check_code("ABC").is_err());
        assert!(check_code("5").is_err());
        assert!(check_code("14").is_err());
        assert!(check_code("9X").is_ok());
        assert!(check_code("a-").is_err());
    }

    #[test]
    fn text_round_trip() {
        let opts = shipped();
        let back = parse(&to_text(&opts));
        assert_eq!(back, opts);
        let p = parse("c  CALL X\n\nZZ\nC  CALL Y\nA DSPLIBL\n");
        assert_eq!(p.len(), 2);
        assert_eq!(lookup(&p, "c").map(|o| o.command.as_str()), Some("CALL X"));
        assert_eq!(p[0].code, "A");
    }
}
