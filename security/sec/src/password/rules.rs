// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Password composition rules: the `QPWD*` system values, or the
//! `QPWDRULES` tokens that replace them. A failure is `CPF4AA7` with
//! IBM's reason code; the table is in `docs/SECURITY.md`.

/// One composition rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `*MINLENn`, `QPWDMINLEN`.
    MinLen(usize),
    /// `*MAXLENn`, `QPWDMAXLEN`.
    MaxLen(usize),
    /// `*CHRLMTAJC`, `QPWDLMTREP 2`: no character twice in a row.
    ChrLmtAjc,
    /// `*CHRLMTREP`, `QPWDLMTREP 1`: no character twice anywhere.
    ChrLmtRep,
    /// `*DGTLMTAJC`, `QPWDLMTAJC`: no two digits side by side.
    DgtLmtAjc,
    /// `*DGTLMTFST`.
    DgtLmtFst,
    /// `*DGTLMTLST`.
    DgtLmtLst,
    /// `*DGTMINn`, `QPWDRQDDGT` (1).
    DgtMin(usize),
    /// `*DGTMAXn`.
    DgtMax(usize),
    /// `*LTRLMTAJC`.
    LtrLmtAjc,
    /// `*LTRLMTFST`.
    LtrLmtFst,
    /// `*LTRLMTLST`.
    LtrLmtLst,
    /// `*LTRMINn`.
    LtrMin(usize),
    /// `*LTRMAXn`.
    LtrMax(usize),
    /// `*LMTSAMPOS`, `QPWDPOSDIF`.
    LmtSamPos,
    /// `*LMTPRFNAME`.
    LmtPrfName,
    /// `*MIXCASEn`: at least n upper and n lower case letters.
    MixCase(usize),
    /// `*REQANY3`: three of upper, lower, digit, special.
    ReqAny3,
    /// `*SPCCHRLMTAJC`.
    SpcLmtAjc,
    /// `*SPCCHRLMTFST`.
    SpcLmtFst,
    /// `*SPCCHRLMTLST`.
    SpcLmtLst,
    /// `*SPCCHRMINn`.
    SpcMin(usize),
    /// `*SPCCHRMAXn`.
    SpcMax(usize),
    /// `*UPRMINn`.
    UprMin(usize),
    /// `*UPRMAXn`.
    UprMax(usize),
    /// `*LWRMINn`.
    LwrMin(usize),
    /// `*LWRMAXn`.
    LwrMax(usize),
}

/// Why a password was refused: `CPF4AA7` and IBM's reason code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    /// The reason code.
    pub reason: u8,
}

impl Violation {
    /// The message identifier.
    pub const MSGID: &'static str = "CPF4AA7";

    /// Quartz's text for the reason.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self.reason {
            1 => "The password needs more letters.",
            2 => "The password needs more digits.",
            3 => "The password needs more special characters.",
            4 => "The password has too many letters.",
            5 => "The password has too many digits.",
            6 => "The password has too many special characters.",
            7 => "The password may not start with a letter.",
            8 => "The password may not start with a digit.",
            9 => "The password may not start with a special character.",
            10 => "The password may not end with a letter.",
            11 => "The password may not end with a digit.",
            12 => "The password may not end with a special character.",
            13 => "The password may not have two letters side by side.",
            14 => "The password may not have two digits side by side.",
            15 => "The password may not have two special characters side by side.",
            16 => "The password needs more upper and lower case letters.",
            17 => {
                "The password needs characters of three kinds: upper case, lower case, digits, special."
            }
            18 => "The password may not contain the user profile name.",
            19 => "The password is too short.",
            20 => "The password is too long.",
            21 => "The password may not use a character more than once.",
            22 => "The password may not repeat a character twice in a row.",
            23 => "The password may not have a character in the same position as the old one.",
            24 => "The password contains a character that is not allowed.",
            _ => "The password does not follow the password rules.",
        }
    }
}

/// The rules in force, from the system values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rules {
    /// The composition rules.
    pub rules: Vec<Rule>,
    /// `QPWDLMTCHR`, when `QPWDRULES` is `*PWDSYSVAL`.
    pub restricted: String,
    /// `*ALLCRTCHG`: the rules also bind `CRTUSRPRF` and `CHGUSRPRF`.
    pub all_crtchg: bool,
}

fn num(token: &str, prefix: &str) -> Option<usize> {
    token.strip_prefix(prefix).and_then(|n| n.parse().ok())
}

impl Rules {
    /// From the system values; `get` answers a value's text.
    pub fn from_sysvals(get: &dyn Fn(&str) -> String) -> Self {
        let tokens: Vec<String> = get("QPWDRULES")
            .split_whitespace()
            .map(str::to_ascii_uppercase)
            .collect();
        if tokens.is_empty() || tokens.iter().any(|t| t == "*PWDSYSVAL") {
            let n = |name: &str, d: usize| get(name).trim().parse().unwrap_or(d);
            let mut rules = vec![
                Rule::MinLen(n("QPWDMINLEN", 6)),
                Rule::MaxLen(n("QPWDMAXLEN", 10)),
            ];
            if get("QPWDLMTAJC").trim() == "1" {
                rules.push(Rule::DgtLmtAjc);
            }
            match get("QPWDLMTREP").trim() {
                "1" => rules.push(Rule::ChrLmtRep),
                "2" => rules.push(Rule::ChrLmtAjc),
                _ => {}
            }
            if get("QPWDPOSDIF").trim() == "1" {
                rules.push(Rule::LmtSamPos);
            }
            if get("QPWDRQDDGT").trim() == "1" {
                rules.push(Rule::DgtMin(1));
            }
            let lmtchr = get("QPWDLMTCHR");
            let restricted = if lmtchr.trim().eq_ignore_ascii_case("*NONE") {
                String::new()
            } else {
                lmtchr.trim().to_string()
            };
            return Self {
                rules,
                restricted,
                all_crtchg: false,
            };
        }
        let mut r = Self::default();
        for t in &tokens {
            let t = t.as_str();
            let rule = match t {
                "*ALLCRTCHG" => {
                    r.all_crtchg = true;
                    continue;
                }
                "*CHRLMTAJC" => Rule::ChrLmtAjc,
                "*CHRLMTREP" => Rule::ChrLmtRep,
                "*DGTLMTAJC" => Rule::DgtLmtAjc,
                "*DGTLMTFST" => Rule::DgtLmtFst,
                "*DGTLMTLST" => Rule::DgtLmtLst,
                "*LTRLMTAJC" => Rule::LtrLmtAjc,
                "*LTRLMTFST" => Rule::LtrLmtFst,
                "*LTRLMTLST" => Rule::LtrLmtLst,
                "*LMTSAMPOS" => Rule::LmtSamPos,
                "*LMTPRFNAME" => Rule::LmtPrfName,
                "*REQANY3" => Rule::ReqAny3,
                "*SPCCHRLMTAJC" => Rule::SpcLmtAjc,
                "*SPCCHRLMTFST" => Rule::SpcLmtFst,
                "*SPCCHRLMTLST" => Rule::SpcLmtLst,
                _ => {
                    let parsed = [
                        ("*MINLEN", Rule::MinLen as fn(usize) -> Rule),
                        ("*MAXLEN", Rule::MaxLen),
                        ("*DGTMIN", Rule::DgtMin),
                        ("*DGTMAX", Rule::DgtMax),
                        ("*LTRMIN", Rule::LtrMin),
                        ("*LTRMAX", Rule::LtrMax),
                        ("*MIXCASE", Rule::MixCase),
                        ("*SPCCHRMIN", Rule::SpcMin),
                        ("*SPCCHRMAX", Rule::SpcMax),
                        ("*UPRMIN", Rule::UprMin),
                        ("*UPRMAX", Rule::UprMax),
                        ("*LWRMIN", Rule::LwrMin),
                        ("*LWRMAX", Rule::LwrMax),
                    ]
                    .into_iter()
                    .find_map(|(p, f)| num(t, p).map(f));
                    match parsed {
                        Some(rule) => rule,
                        None => continue,
                    }
                }
            };
            r.rules.push(rule);
        }
        if !r.rules.iter().any(|x| matches!(x, Rule::MaxLen(_))) {
            r.rules.push(Rule::MaxLen(128));
        }
        r
    }

    /// Check `new` for profile `profile`; `old` is the current password
    /// when the user typed it (`CHGPWD`), for `*LMTSAMPOS`.
    pub fn check(
        &self,
        new: &str,
        old: Option<&str>,
        profile: &str,
        pwdlvl: u8,
    ) -> Result<(), Violation> {
        let chars: Vec<char> = new.chars().collect();
        let v = |reason| Err(Violation { reason });
        let kind = |c: char| -> u8 {
            if c.is_ascii_digit() {
                1
            } else if c.is_alphabetic() {
                0
            } else {
                2
            }
        };
        let count = |k: u8| chars.iter().filter(|c| kind(**c) == k).count();
        let (letters, digits, specials) = (count(0), count(1), count(2));
        let upper = chars.iter().filter(|c| c.is_uppercase()).count();
        let lower = chars.iter().filter(|c| c.is_lowercase()).count();
        let adjacent = |k: u8| chars.windows(2).any(|w| kind(w[0]) == k && kind(w[1]) == k);
        let first = chars.first().map(|c| kind(*c));
        let last = chars.last().map(|c| kind(*c));
        if chars
            .iter()
            .any(|c| self.restricted.contains(c.to_ascii_uppercase()))
        {
            return v(24);
        }
        if pwdlvl <= 1 && chars.len() > super::SHORT_MAX {
            return v(20);
        }
        for rule in &self.rules {
            let bad = match *rule {
                Rule::MinLen(n) => (chars.len() < n).then_some(19),
                Rule::MaxLen(n) => (chars.len() > n).then_some(20),
                Rule::ChrLmtAjc => chars.windows(2).any(|w| w[0] == w[1]).then_some(22),
                Rule::ChrLmtRep => {
                    let mut seen = std::collections::HashSet::new();
                    chars.iter().any(|c| !seen.insert(*c)).then_some(21)
                }
                Rule::DgtLmtAjc => adjacent(1).then_some(14),
                Rule::LtrLmtAjc => adjacent(0).then_some(13),
                Rule::SpcLmtAjc => adjacent(2).then_some(15),
                Rule::DgtLmtFst => (first == Some(1)).then_some(8),
                Rule::LtrLmtFst => (first == Some(0)).then_some(7),
                Rule::SpcLmtFst => (first == Some(2)).then_some(9),
                Rule::DgtLmtLst => (last == Some(1)).then_some(11),
                Rule::LtrLmtLst => (last == Some(0)).then_some(10),
                Rule::SpcLmtLst => (last == Some(2)).then_some(12),
                Rule::DgtMin(n) => (digits < n).then_some(2),
                Rule::DgtMax(n) => (digits > n).then_some(5),
                Rule::LtrMin(n) => (letters < n).then_some(1),
                Rule::LtrMax(n) => (letters > n).then_some(4),
                Rule::SpcMin(n) => (specials < n).then_some(3),
                Rule::SpcMax(n) => (specials > n).then_some(6),
                Rule::UprMin(n) | Rule::LwrMin(n) | Rule::MixCase(n) => {
                    let need_up = !matches!(rule, Rule::LwrMin(_));
                    let need_low = !matches!(rule, Rule::UprMin(_));
                    ((need_up && upper < n) || (need_low && lower < n)).then_some(16)
                }
                Rule::UprMax(n) => (upper > n).then_some(4),
                Rule::LwrMax(n) => (lower > n).then_some(4),
                Rule::ReqAny3 => {
                    let kinds = [upper > 0, lower > 0, digits > 0, specials > 0];
                    (kinds.iter().filter(|k| **k).count() < 3).then_some(17)
                }
                Rule::LmtSamPos => old
                    .is_some_and(|o| {
                        let o = o.to_uppercase();
                        new.to_uppercase()
                            .chars()
                            .zip(o.chars())
                            .any(|(a, b)| a == b)
                    })
                    .then_some(23),
                Rule::LmtPrfName => (!profile.is_empty()
                    && new.to_uppercase().contains(&profile.to_uppercase()))
                .then_some(18),
            };
            if let Some(reason) = bad {
                return v(reason);
            }
        }
        Ok(())
    }
}

/// Previous passwords a new one must differ from, for `QPWDRQDDIF`.
#[must_use]
pub fn history_len(rqddif: u8) -> usize {
    match rqddif {
        1 => 32,
        2 => 24,
        3 => 18,
        4 => 12,
        5 => 10,
        6 => 8,
        7 => 6,
        8 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(vals: &[(&str, &str)]) -> Rules {
        let vals: Vec<(String, String)> = vals
            .iter()
            .map(|(a, b)| ((*a).into(), (*b).into()))
            .collect();
        Rules::from_sysvals(&|n: &str| {
            vals.iter()
                .find(|(k, _)| k == n)
                .map_or_else(String::new, |(_, v)| v.clone())
        })
    }

    fn reason(r: &Rules, p: &str) -> u8 {
        r.check(p, Some("OLDPASS1"), "BOB", 3)
            .err()
            .map_or(0, |v| v.reason)
    }

    #[test]
    fn system_values() {
        let r = rules(&[
            ("QPWDRULES", "*PWDSYSVAL"),
            ("QPWDMINLEN", "6"),
            ("QPWDMAXLEN", "10"),
            ("QPWDLMTAJC", "1"),
            ("QPWDLMTREP", "2"),
            ("QPWDRQDDGT", "1"),
            ("QPWDPOSDIF", "1"),
            ("QPWDLMTCHR", "AEIOU"),
        ]);
        assert_eq!(reason(&r, "XYZ1"), 19);
        assert_eq!(reason(&r, "XYZWQRSTL1Z"), 20);
        assert_eq!(reason(&r, "XYZW12"), 14);
        assert_eq!(reason(&r, "XYYW1Z"), 22);
        assert_eq!(reason(&r, "XYZWQR"), 2);
        assert_eq!(reason(&r, "XLZW1R"), 23);
        assert_eq!(reason(&r, "XAZW1R"), 24);
        assert_eq!(reason(&r, "ZYXW1Q"), 0);
    }

    #[test]
    fn every_token() {
        let cases: &[(&str, &str, u8)] = &[
            ("*MINLEN8", "Ab1!", 19),
            ("*MAXLEN5", "Ab1!xyz", 20),
            ("*CHRLMTAJC", "aab1", 22),
            ("*CHRLMTREP", "abca1", 21),
            ("*DGTLMTAJC", "ab12", 14),
            ("*DGTLMTFST", "1abc", 8),
            ("*DGTLMTLST", "abc1", 11),
            ("*DGTMIN2", "abc1", 2),
            ("*DGTMAX1", "a12b", 5),
            ("*LTRLMTAJC", "1ab2", 13),
            ("*LTRLMTFST", "a123", 7),
            ("*LTRLMTLST", "123a", 10),
            ("*LTRMIN3", "ab12", 1),
            ("*LTRMAX1", "ab12", 4),
            ("*LMTSAMPOS", "OXXXXXX", 23),
            ("*LMTPRFNAME", "xbobx1", 18),
            ("*MIXCASE1", "abc123", 16),
            ("*REQANY3", "abc123", 17),
            ("*SPCCHRLMTAJC", "a!!1", 15),
            ("*SPCCHRLMTFST", "!a1", 9),
            ("*SPCCHRLMTLST", "a1!", 12),
            ("*SPCCHRMIN1", "abc1", 3),
            ("*SPCCHRMAX0", "ab!1", 6),
            ("*UPRMIN1", "abc1", 16),
            ("*UPRMAX0", "Abc1", 4),
            ("*LWRMIN1", "ABC1", 16),
            ("*LWRMAX0", "Abc1", 4),
        ];
        for (token, pw, want) in cases {
            let r = rules(&[("QPWDRULES", token)]);
            assert_eq!(reason(&r, pw), *want, "{token} {pw}");
            assert!(!Violation { reason: *want }.text().is_empty());
        }
        let r = rules(&[("QPWDRULES", "*ALLCRTCHG *REQANY3")]);
        assert!(r.all_crtchg);
        assert_eq!(reason(&r, "Abc123"), 0);
        // Level 0 and 1 never take more than ten characters.
        assert_eq!(
            r.check("Abcdefgh123", None, "BOB", 0)
                .err()
                .map(|v| v.reason),
            Some(20)
        );
    }

    #[test]
    fn history_lengths() {
        let want = [0, 32, 24, 18, 12, 10, 8, 6, 4];
        for (d, n) in want.iter().enumerate() {
            assert_eq!(history_len(u8::try_from(d).unwrap()), *n);
        }
    }
}
