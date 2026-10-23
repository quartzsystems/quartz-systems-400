// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Which commands a `CMDxxx` menu holds (`docs/MENUS.md`, "The
//! generator"): a verb menu takes the names that start with its verb;
//! a subject menu takes the names that carry its code as a whole
//! token after the verb.

use std::collections::BTreeSet;

/// The verbs: `CMD<verb>` is a verb menu, and a name's leading verb is
/// stripped before a subject is looked for.
pub const VERBS: &[&str] = &[
    "ADD", "ALC", "ANS", "ANZ", "APY", "ASK", "CALL", "CFG", "CHG", "CHK", "CLO", "CLR", "CMP",
    "CNL", "CPY", "CRT", "CVT", "DCL", "DLC", "DLT", "DLY", "DMP", "DO", "DSC", "DSP", "DUP",
    "EDT", "EJT", "EML", "END", "EXT", "FMT", "FND", "GEN", "GO", "GRT", "HLD", "INS", "INZ",
    "LOD", "MGR", "MON", "MOV", "MRG", "OPN", "OVR", "POS", "PRT", "PWR", "RCL", "RCV", "RGZ",
    "RLS", "RMV", "RNM", "RPL", "RQS", "RRT", "RSM", "RST", "RTN", "RTV", "RUN", "RVK", "SAV",
    "SBM", "SET", "SIGN", "SLT", "SND", "STR", "TFR", "TRC", "UPD", "VFY", "VRY", "WAIT", "WRK",
];

/// Menus IBM lists on `VERB` whose code starts no command: filled by
/// the subject rule.
pub const VERB_LISTED: &[&str] = &["AUD", "CNR"];

/// Verb-and-subject menus: `(code, verb, subject)`.
pub const COMPOSITES: &[(&str, &str, &str)] = &[("CRTF", "CRT", "F"), ("CRTPGM", "CRT", "PGM")];

/// The groups of `MAJOR` whose contents IBM curates by hand; written
/// later, not generated.
pub const HAND_WRITTEN: &[&str] = &[
    "CMDOBJMGT",
    "CMDFILE",
    "CMDSAVRST",
    "CMDWRKMGT",
    "CMDDTAMGT",
    "CMDSYSCTL",
    "CMDCMNMGT",
    "CMDDSTSRV",
    "CMDPRBMGT",
    "CMDMSGHDL",
    "CMDS36",
    "CMDS38",
    "CMDOFC",
];

/// One-letter tokens that end a name's words: attributes, description,
/// entry, file, list, member, queue.
pub const SUFFIXES: &[&str] = &["A", "D", "E", "F", "L", "M", "Q"];

/// The tokens the `F` (file) subject matches: every file type.
pub const FILE_TOKENS: &[&str] = &[
    "F", "PF", "LF", "DSPF", "PRTF", "SAVF", "SRCF", "SRCPF", "DBF", "DDMF", "ICFF", "BSCF",
    "CMNF", "NETF", "TAPF", "DKTF", "QRYF", "TIEF", "MXDF", "SPLF", "IMPF", "STMF",
];

/// Tokens of command names that are no menu code.
pub const EXTRA_TOKENS: &[&str] = &[
    "PAGS", "OVL", "SRCPF", "DKTF", "MXDF", "IMPF", "STMF", "TYP", "INT", "ABN", "CPYD", "PFU",
];

/// Members no token rule finds: the command-definition statements and
/// the CL control statements, by menu.
pub const CURATED: &[(&str, &[&str])] = &[
    (
        "CMDCMDDEF",
        &["CMD", "DEP", "ELEM", "PARM", "PMTCTL", "QUAL"],
    ),
    (
        "CMDCL",
        &[
            "CALLSUBR",
            "CHGVAR",
            "DCL",
            "DCLF",
            "DCLPRCOPT",
            "DO",
            "DOFOR",
            "DOUNTIL",
            "DOWHILE",
            "ELSE",
            "ENDDO",
            "ENDSELECT",
            "ENDSUBR",
            "GOTO",
            "IF",
            "ITERATE",
            "LEAVE",
            "MONMSG",
            "OTHERWISE",
            "RETURN",
            "SELECT",
            "SUBR",
            "WHEN",
        ],
    ),
    ("CMDRTN", &["RETURN"]),
];

/// Knows the tokens command names are made of.
#[derive(Clone, Debug)]
pub struct Classifier {
    tokens: BTreeSet<String>,
    multi: BTreeSet<String>,
}

impl Classifier {
    /// Over the menu codes (`CMDxxx` without `CMD`), the verbs, the
    /// extra tokens and the suffixes.
    pub fn new(codes: impl IntoIterator<Item = String>) -> Self {
        let mut tokens: BTreeSet<String> = codes.into_iter().collect();
        tokens.extend(VERBS.iter().map(|s| (*s).to_string()));
        tokens.extend(EXTRA_TOKENS.iter().map(|s| (*s).to_string()));
        tokens.extend(FILE_TOKENS.iter().map(|s| (*s).to_string()));
        let multi = tokens.iter().filter(|t| t.len() > 1).cloned().collect();
        tokens.extend(SUFFIXES.iter().map(|s| (*s).to_string()));
        Self { tokens, multi }
    }

    /// The longest verb that starts the name, and the rest.
    #[must_use]
    pub fn strip_verb<'a>(&self, name: &'a str) -> (Option<&'static str>, &'a str) {
        VERBS
            .iter()
            .filter(|v| name.starts_with(**v))
            .max_by_key(|v| v.len())
            .map_or((None, name), |v| (Some(*v), &name[v.len()..]))
    }

    /// The longest token that starts `tail`.
    fn longest_at<'a>(&self, tail: &'a str) -> Option<&'a str> {
        (1..=tail.len().min(10))
            .rev()
            .map(|l| &tail[..l])
            .find(|t| self.tokens.contains(*t))
    }

    /// Can `s` be split into multi-letter tokens? The empty string can.
    fn segmentable(&self, s: &str) -> bool {
        let n = s.len();
        let mut ok = vec![false; n + 1];
        ok[0] = true;
        for i in 0..n {
            if !ok[i] {
                continue;
            }
            for j in i + 2..=n.min(i + 10) {
                if self.multi.contains(&s[i..j]) {
                    ok[j] = true;
                }
            }
        }
        ok[n]
    }

    /// Does `rest` (a name without its verb) carry `code` as a token?
    fn carries(&self, rest: &str, code: &str) -> bool {
        let file = code == "F";
        (0..rest.len()).any(|i| {
            let Some(tok) = self.longest_at(&rest[i..]) else {
                return false;
            };
            let hit = if file {
                FILE_TOKENS.contains(&tok)
            } else {
                tok == code
            };
            hit && self.segmentable(&rest[..i])
        })
    }

    /// A verb menu's member: the name starts with the verb.
    #[must_use]
    pub fn verb_member(name: &str, verb: &str) -> bool {
        name.starts_with(verb)
    }

    /// A subject menu's member: the code is a token after the verb.
    #[must_use]
    pub fn subject_member(&self, name: &str, code: &str) -> bool {
        let (_, rest) = self.strip_verb(name);
        self.carries(rest, code)
    }

    /// A composite menu's member: the verb, then the subject.
    #[must_use]
    pub fn composite_member(&self, name: &str, verb: &str, subject: &str) -> bool {
        name.strip_prefix(verb)
            .is_some_and(|rest| self.carries(rest, subject))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classifier() -> Classifier {
        Classifier::new(
            [
                "JOB", "JOBD", "JOBQ", "JOBQE", "PF", "PFR", "FIL", "SRC", "JRN", "SCDE", "MLB",
                "CL", "PGM", "SBS", "SPLF", "C", "ACC", "AP", "BCH", "DSPF", "BND",
            ]
            .into_iter()
            .map(String::from),
        )
    }

    #[test]
    fn verbs_strip() {
        let c = classifier();
        assert_eq!(c.strip_verb("SIGNOFF"), (Some("SIGN"), "OFF"));
        assert_eq!(c.strip_verb("CALLPRC"), (Some("CALL"), "PRC"));
        assert_eq!(c.strip_verb("BCHJOB"), (None, "BCHJOB"));
        assert_eq!(c.strip_verb("WAIT"), (Some("WAIT"), ""));
    }

    #[test]
    fn subjects_are_whole_tokens() {
        let c = classifier();
        for n in [
            "CHGJOB",
            "ADDJOBSCDE",
            "CHGJOBMLBA",
            "CHGJOBTYP",
            "BCHJOB",
            "ENDBCHJOB",
            "SBMJOB",
        ] {
            assert!(c.subject_member(n, "JOB"), "{n}");
        }
        for n in ["CHGJOBD", "WRKJOBQ", "ADDJOBQE", "DATA", "WRKACTJOBD"] {
            assert!(!c.subject_member(n, "JOB"), "{n}");
        }
        for n in [
            "CRTPF",
            "CHGPFM",
            "ADDPFCST",
            "CHGSRCPF",
            "STRJRNPF",
            "CVTPFMPAGS",
            "CVTPAGSPFM",
        ] {
            assert!(c.subject_member(n, "PF"), "{n}");
        }
        for n in ["DSPFD", "DSPFFD", "STRPFRMON", "CRTPFRDTA", "WRKSPLF"] {
            assert!(!c.subject_member(n, "PF"), "{n}");
        }
    }

    #[test]
    fn the_file_subject_takes_every_file_type() {
        let c = classifier();
        for n in [
            "CPYF", "CRTPF", "DSPFD", "DSPFFD", "CRTDSPF", "WRKSPLF", "CHGSRCPF",
        ] {
            assert!(c.subject_member(n, "F"), "{n}");
        }
        assert!(!c.subject_member("CHGJOB", "F"));
        assert!(c.composite_member("CRTPF", "CRT", "F"));
        assert!(!c.composite_member("CPYF", "CRT", "F"));
        assert!(c.composite_member("CRTCLPGM", "CRT", "PGM"));
        assert!(!c.composite_member("CRTBNDRPG", "CRT", "PGM"));
    }

    #[test]
    fn one_letter_codes_need_a_boundary() {
        let c = classifier();
        assert!(c.subject_member("CRTBNDC", "C"));
        assert!(c.subject_member("CRTCMOD", "C"));
        assert!(!c.subject_member("CRTSRCPF", "C"));
        assert!(!c.subject_member("ADDACC", "C"));
        assert!(Classifier::verb_member("ADDAJE", "ADD"));
        assert!(!Classifier::verb_member("CHGADD", "ADD"));
    }
}
