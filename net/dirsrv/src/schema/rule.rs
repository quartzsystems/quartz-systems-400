// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Matching rules (RFC 4517 §4.2, RFC 4530 and IBM's): each one's
//! normalizer, so equality is equality of normalized bytes and ordering
//! is their byte order; substring assertions and the word rules match
//! directly.

use std::cmp::Ordering;

use super::prep::{self, Case, Part};
use super::syntax;
use super::time;

/// What a matching rule is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An EQUALITY rule.
    Equality,
    /// An ORDERING rule.
    Ordering,
    /// A SUBSTR rule.
    Substring,
}

/// How a rule compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imp {
    /// RFC 4518 with case folding.
    CaseIgnore,
    /// RFC 4518 without case folding.
    CaseExact,
    /// Case-ignore over the lines of a postal address.
    CaseIgnoreList,
    /// Numeric strings: spaces removed.
    Numeric,
    /// Telephone numbers: spaces and hyphens removed, case folded.
    Telephone,
    /// Bytes as they are.
    Octet,
    /// INTEGER values.
    Integer,
    /// TRUE and FALSE.
    Boolean,
    /// Bit strings, exact.
    BitString,
    /// Generalized Time.
    GeneralizedTime,
    /// UTC Time.
    UtcTime,
    /// OIDs: descriptors resolved to numeric OIDs.
    Oid,
    /// DNs: each AVA by its type's equality rule.
    Dn,
    /// Name and Optional UID.
    UniqueMember,
    /// The first INTEGER of a description.
    IntegerFirst,
    /// The first OID of a description.
    OidFirst,
    /// The first Directory String of a value, case-ignore.
    DirStringFirst,
    /// UUIDs, hex case folded.
    Uuid,
    /// An assertion equal to one word of the value (keywordMatch,
    /// wordMatch).
    Word,
}

/// A matching rule QS/400 knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// The OID.
    pub oid: &'static str,
    /// The name.
    pub name: &'static str,
    /// The assertion syntax.
    pub syntax: &'static str,
    /// The kind.
    pub kind: Kind,
    /// How it compares.
    pub imp: Imp,
}

/// What a rule needs from the schema: descriptors to OIDs and DNs
/// normalized.
pub trait Context {
    /// The numeric OID a descriptor names (an attribute type, class,
    /// rule or syntax), case-insensitive.
    fn oid_of(&self, descr: &str) -> Option<String>;
    /// A DN's normalized form, or `None` when it does not parse.
    fn normalize_dn(&self, dn: &str) -> Option<Vec<u8>>;
}

/// Why a value has no normalized form: the rule is then undefined for
/// it (RFC 4511 §4.5.1.7: Undefined).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MatchError {
    /// The string preparation failed.
    #[error(transparent)]
    Prep(#[from] prep::PrepError),
    /// The value is not of the rule's syntax.
    #[error("value is not valid for {0}")]
    Invalid(&'static str),
    /// The rule does not do this (an ordering asked of an equality-only
    /// rule, a substring of a non-string rule).
    #[error("{0} does not match this way")]
    Inappropriate(&'static str),
}

const DS: &str = "1.3.6.1.4.1.1466.115.121.1.15";
const IA5: &str = "1.3.6.1.4.1.1466.115.121.1.26";
const SUB: &str = "1.3.6.1.4.1.1466.115.121.1.58";

macro_rules! rule {
    ($oid:literal, $name:literal, $syn:expr, $kind:ident, $imp:ident) => {
        Rule {
            oid: $oid,
            name: $name,
            syntax: $syn,
            kind: Kind::$kind,
            imp: Imp::$imp,
        }
    };
}

const fn s(n: &'static str) -> &'static str {
    n
}

/// Every matching rule QS/400 knows. IBM's rules without an RFC
/// equivalent are mapped to the nearest behaviour (docs/DIRSRV.md §6).
pub const RULES: &[Rule] = &[
    rule!(
        "2.5.13.0",
        "objectIdentifierMatch",
        s("1.3.6.1.4.1.1466.115.121.1.38"),
        Equality,
        Oid
    ),
    rule!(
        "2.5.13.1",
        "distinguishedNameMatch",
        s("1.3.6.1.4.1.1466.115.121.1.12"),
        Equality,
        Dn
    ),
    rule!("2.5.13.2", "caseIgnoreMatch", DS, Equality, CaseIgnore),
    rule!(
        "2.5.13.3",
        "caseIgnoreOrderingMatch",
        DS,
        Ordering,
        CaseIgnore
    ),
    rule!(
        "2.5.13.4",
        "caseIgnoreSubstringsMatch",
        SUB,
        Substring,
        CaseIgnore
    ),
    rule!("2.5.13.5", "caseExactMatch", DS, Equality, CaseExact),
    rule!(
        "2.5.13.6",
        "caseExactOrderingMatch",
        DS,
        Ordering,
        CaseExact
    ),
    rule!(
        "2.5.13.7",
        "caseExactSubstringsMatch",
        SUB,
        Substring,
        CaseExact
    ),
    rule!(
        "2.5.13.8",
        "numericStringMatch",
        s("1.3.6.1.4.1.1466.115.121.1.36"),
        Equality,
        Numeric
    ),
    rule!(
        "2.5.13.9",
        "numericStringOrderingMatch",
        s("1.3.6.1.4.1.1466.115.121.1.36"),
        Ordering,
        Numeric
    ),
    rule!(
        "2.5.13.10",
        "numericStringSubstringsMatch",
        SUB,
        Substring,
        Numeric
    ),
    rule!(
        "2.5.13.11",
        "caseIgnoreListMatch",
        s("1.3.6.1.4.1.1466.115.121.1.41"),
        Equality,
        CaseIgnoreList
    ),
    rule!(
        "2.5.13.12",
        "caseIgnoreListSubstringsMatch",
        SUB,
        Substring,
        CaseIgnoreList
    ),
    rule!(
        "2.5.13.13",
        "booleanMatch",
        s("1.3.6.1.4.1.1466.115.121.1.7"),
        Equality,
        Boolean
    ),
    rule!(
        "2.5.13.14",
        "integerMatch",
        s("1.3.6.1.4.1.1466.115.121.1.27"),
        Equality,
        Integer
    ),
    rule!(
        "2.5.13.15",
        "integerOrderingMatch",
        s("1.3.6.1.4.1.1466.115.121.1.27"),
        Ordering,
        Integer
    ),
    rule!(
        "2.5.13.16",
        "bitStringMatch",
        s("1.3.6.1.4.1.1466.115.121.1.6"),
        Equality,
        BitString
    ),
    rule!(
        "2.5.13.17",
        "octetStringMatch",
        s("1.3.6.1.4.1.1466.115.121.1.40"),
        Equality,
        Octet
    ),
    rule!(
        "2.5.13.18",
        "octetStringOrderingMatch",
        s("1.3.6.1.4.1.1466.115.121.1.40"),
        Ordering,
        Octet
    ),
    rule!(
        "2.5.13.20",
        "telephoneNumberMatch",
        s("1.3.6.1.4.1.1466.115.121.1.50"),
        Equality,
        Telephone
    ),
    rule!(
        "2.5.13.21",
        "telephoneNumberSubstringsMatch",
        SUB,
        Substring,
        Telephone
    ),
    rule!(
        "2.5.13.22",
        "presentationAddressMatch",
        s("1.3.6.1.4.1.1466.115.121.1.43"),
        Equality,
        CaseIgnore
    ),
    rule!(
        "2.5.13.23",
        "uniqueMemberMatch",
        s("1.3.6.1.4.1.1466.115.121.1.34"),
        Equality,
        UniqueMember
    ),
    rule!(
        "2.5.13.24",
        "protocolInformationMatch",
        s("1.3.6.1.4.1.1466.115.121.1.42"),
        Equality,
        CaseIgnore
    ),
    rule!(
        "2.5.13.25",
        "uTCTimeMatch",
        s("1.3.6.1.4.1.1466.115.121.1.53"),
        Equality,
        UtcTime
    ),
    rule!(
        "2.5.13.27",
        "generalizedTimeMatch",
        s("1.3.6.1.4.1.1466.115.121.1.24"),
        Equality,
        GeneralizedTime
    ),
    rule!(
        "2.5.13.28",
        "generalizedTimeOrderingMatch",
        s("1.3.6.1.4.1.1466.115.121.1.24"),
        Ordering,
        GeneralizedTime
    ),
    rule!(
        "2.5.13.29",
        "integerFirstComponentMatch",
        s("1.3.6.1.4.1.1466.115.121.1.27"),
        Equality,
        IntegerFirst
    ),
    rule!(
        "2.5.13.30",
        "objectIdentifierFirstComponentMatch",
        s("1.3.6.1.4.1.1466.115.121.1.38"),
        Equality,
        OidFirst
    ),
    rule!(
        "2.5.13.31",
        "directoryStringFirstComponentMatch",
        DS,
        Equality,
        DirStringFirst
    ),
    rule!("2.5.13.32", "wordMatch", DS, Equality, Word),
    rule!("2.5.13.33", "keywordMatch", DS, Equality, Word),
    rule!(
        "1.3.6.1.4.1.1466.109.114.1",
        "caseExactIA5Match",
        IA5,
        Equality,
        CaseExact
    ),
    rule!(
        "1.3.6.1.4.1.1466.109.114.2",
        "caseIgnoreIA5Match",
        IA5,
        Equality,
        CaseIgnore
    ),
    rule!(
        "1.3.6.1.4.1.1466.109.114.3",
        "caseIgnoreIA5SubstringsMatch",
        SUB,
        Substring,
        CaseIgnore
    ),
    rule!(
        "1.3.6.1.1.16.2",
        "uuidMatch",
        s("1.3.6.1.1.16.1"),
        Equality,
        Uuid
    ),
    rule!(
        "1.3.6.1.1.16.3",
        "uuidOrderingMatch",
        s("1.3.6.1.1.16.1"),
        Ordering,
        Uuid
    ),
    rule!("1.3.18.0.2.22.2", "ibm-entryUuidMatch", DS, Equality, Uuid),
    rule!(
        "1.3.18.0.2.4.405",
        "distinguishedNameOrderingMatch",
        s("1.3.6.1.4.1.1466.115.121.1.12"),
        Ordering,
        Dn
    ),
];

/// The rule with this OID or name (case-insensitive).
#[must_use]
pub fn find(id: &str) -> Option<&'static Rule> {
    RULES
        .iter()
        .find(|r| r.oid == id || r.name.eq_ignore_ascii_case(id))
}

/// An INTEGER as a key whose bytes order as the numbers: a sign byte,
/// the digit count (inverted for negatives) and the digits (inverted
/// for negatives).
fn integer_key(v: &[u8]) -> Option<Vec<u8>> {
    let t = std::str::from_utf8(v).ok()?.trim();
    let (neg, d) = match t.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let d = d.trim_start_matches('0');
    if d.is_empty() {
        return Some(vec![1]);
    }
    let len = u32::try_from(d.len()).ok()?.to_be_bytes();
    let mut k = Vec::with_capacity(d.len() + 5);
    if neg {
        k.push(0);
        k.extend(len.iter().map(|b| !b));
        k.extend(d.bytes().map(|c| b'9' - (c - b'0')));
    } else {
        k.push(2);
        k.extend(len);
        k.extend(d.bytes());
    }
    Some(k)
}

/// The canonical decimal form of an INTEGER (`+5`, `005` and `-0` read
/// as `5`, `5` and `0`).
fn integer_text(v: &[u8]) -> Option<Vec<u8>> {
    let t = std::str::from_utf8(v).ok()?.trim();
    let (neg, d) = match t.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let d = d.trim_start_matches('0');
    Some(match (neg, d.is_empty()) {
        (_, true) => b"0".to_vec(),
        (true, false) => format!("-{d}").into_bytes(),
        (false, false) => d.as_bytes().to_vec(),
    })
}

/// The first component of a `( first ... )` value.
fn first_component(v: &[u8]) -> Option<&str> {
    let t = std::str::from_utf8(v).ok()?.trim();
    let inner = t.strip_prefix('(')?.trim_start();
    inner
        .split(|c: char| c.is_whitespace() || c == ')')
        .next()
        .filter(|s| !s.is_empty())
}

fn oid_norm(ctx: &dyn Context, t: &str) -> Option<Vec<u8>> {
    let t = t.trim();
    if syntax::is_numeric_oid(t) {
        Some(t.as_bytes().to_vec())
    } else if syntax::is_descr(t) {
        Some(
            ctx.oid_of(t)
                .unwrap_or_else(|| t.to_ascii_lowercase())
                .into_bytes(),
        )
    } else {
        None
    }
}

impl Rule {
    fn invalid(&self) -> MatchError {
        MatchError::Invalid(self.name)
    }

    /// The normalized form of `v`: equal values have equal forms, and
    /// for ordering rules the forms' bytes order as the values do. It is
    /// the equality and ordering index key.
    ///
    /// # Errors
    /// [`MatchError`] when `v` has no form under this rule.
    pub fn normalize(&self, ctx: &dyn Context, v: &[u8]) -> Result<Vec<u8>, MatchError> {
        let bad = || self.invalid();
        Ok(match self.imp {
            Imp::CaseIgnore | Imp::DirStringFirst | Imp::Word => {
                prep::prepare(v, Case::Fold)?.into_bytes()
            }
            Imp::CaseExact => prep::prepare(v, Case::Exact)?.into_bytes(),
            Imp::CaseIgnoreList => {
                let t = std::str::from_utf8(v).map_err(|_| bad())?;
                let lines: Result<Vec<_>, _> = t
                    .split('$')
                    .map(|l| prep::prepare(l.as_bytes(), Case::Fold))
                    .collect();
                lines?.join("$").into_bytes()
            }
            Imp::Numeric => {
                let t = prep::prepare(v, Case::Exact)?;
                t.chars()
                    .filter(|c| *c != ' ')
                    .collect::<String>()
                    .into_bytes()
            }
            Imp::Telephone => {
                let t = prep::prepare(v, Case::Fold)?;
                t.chars()
                    .filter(|c| *c != ' ' && *c != '-' && *c != '\u{2010}' && *c != '\u{2212}')
                    .collect::<String>()
                    .into_bytes()
            }
            Imp::Octet | Imp::BitString => v.to_vec(),
            Imp::Integer if self.kind == Kind::Ordering => integer_key(v).ok_or_else(bad)?,
            Imp::Integer => integer_text(v).ok_or_else(bad)?,
            Imp::Boolean => {
                if v.eq_ignore_ascii_case(b"TRUE") {
                    b"TRUE".to_vec()
                } else if v.eq_ignore_ascii_case(b"FALSE") {
                    b"FALSE".to_vec()
                } else {
                    return Err(bad());
                }
            }
            Imp::GeneralizedTime => time::parse_generalized(v, true)
                .ok_or_else(bad)?
                .canonical()
                .into_bytes(),
            Imp::UtcTime => time::parse_utc(v, true)
                .ok_or_else(bad)?
                .canonical()
                .into_bytes(),
            Imp::Oid => {
                oid_norm(ctx, std::str::from_utf8(v).map_err(|_| bad())?).ok_or_else(bad)?
            }
            Imp::Dn => ctx
                .normalize_dn(std::str::from_utf8(v).map_err(|_| bad())?)
                .ok_or_else(bad)?,
            Imp::UniqueMember => {
                let t = std::str::from_utf8(v).map_err(|_| bad())?;
                let (name, bits) = syntax::split_uid(t);
                let mut k = ctx.normalize_dn(name).ok_or_else(bad)?;
                if let Some(b) = bits {
                    k.push(b'#');
                    k.extend_from_slice(b.as_bytes());
                }
                k
            }
            Imp::IntegerFirst => {
                integer_text(first_component(v).ok_or_else(bad)?.as_bytes()).ok_or_else(bad)?
            }
            Imp::OidFirst => oid_norm(ctx, first_component(v).ok_or_else(bad)?).ok_or_else(bad)?,
            Imp::Uuid => {
                let t = std::str::from_utf8(v).map_err(|_| bad())?;
                t.trim().to_ascii_lowercase().into_bytes()
            }
        })
    }

    /// The normalized form of an assertion value. For the first-component
    /// rules the assertion is the component alone, not a description.
    ///
    /// # Errors
    /// As [`Rule::normalize`].
    pub fn normalize_assertion(&self, ctx: &dyn Context, a: &[u8]) -> Result<Vec<u8>, MatchError> {
        match self.imp {
            Imp::IntegerFirst => integer_text(a).ok_or_else(|| self.invalid()),
            Imp::OidFirst => oid_norm(ctx, std::str::from_utf8(a).map_err(|_| self.invalid())?)
                .ok_or_else(|| self.invalid()),
            _ => self.normalize(ctx, a),
        }
    }

    /// Whether `value` matches the assertion under this equality rule:
    /// `Some(true)`, `Some(false)`, or `None` for Undefined.
    #[must_use]
    pub fn equals(&self, ctx: &dyn Context, value: &[u8], assertion: &[u8]) -> Option<bool> {
        let a = self.normalize_assertion(ctx, assertion).ok()?;
        if self.imp == Imp::Word {
            let v = self.normalize(ctx, value).ok()?;
            let words = std::str::from_utf8(&v).ok()?;
            let a = std::str::from_utf8(&a).ok()?;
            let matched = if self.oid == "2.5.13.32" {
                words.split(' ').any(|w| w == a)
            } else {
                // keywordMatch: the assertion as a run of whole words.
                let padded = format!(" {words} ");
                padded.contains(&format!(" {a} "))
            };
            return Some(matched);
        }
        let v = match self.imp {
            Imp::DirStringFirst => {
                let t = std::str::from_utf8(value).ok()?;
                let first = first_component(value).unwrap_or(t);
                prep::prepare(first.trim_matches('\'').as_bytes(), Case::Fold)
                    .ok()?
                    .into_bytes()
            }
            _ => self.normalize(ctx, value).ok()?,
        };
        Some(v == a)
    }

    /// How `value` compares with the assertion under this ordering rule,
    /// or `None` for Undefined.
    #[must_use]
    pub fn compare(&self, ctx: &dyn Context, value: &[u8], assertion: &[u8]) -> Option<Ordering> {
        let v = self.normalize(ctx, value).ok()?;
        let a = self.normalize(ctx, assertion).ok()?;
        Some(v.cmp(&a))
    }

    /// The case this rule prepares strings with, for substring matching.
    fn substring_case(&self) -> Result<Case, MatchError> {
        match self.imp {
            Imp::CaseIgnore | Imp::CaseIgnoreList | Imp::Telephone => Ok(Case::Fold),
            Imp::CaseExact | Imp::Numeric => Ok(Case::Exact),
            _ => Err(MatchError::Inappropriate(self.name)),
        }
    }

    /// The prepared value substring assertions are matched against, and
    /// that substring index keys (grams) are cut from.
    ///
    /// # Errors
    /// [`MatchError`] when the rule is not a string rule or the value
    /// does not prepare.
    pub fn substring_target(&self, v: &[u8]) -> Result<String, MatchError> {
        let case = self.substring_case()?;
        let t = prep::substring_target(&self.lines(v), case)?;
        Ok(self.strip(t))
    }

    /// Numeric and telephone rules drop spaces (and hyphens) everywhere.
    fn strip(&self, t: String) -> String {
        match self.imp {
            Imp::Numeric => t.chars().filter(|c| *c != ' ').collect(),
            Imp::Telephone => t.chars().filter(|c| *c != ' ' && *c != '-').collect(),
            _ => t,
        }
    }

    /// A postal address's line breaks read as spaces in substrings.
    fn lines(&self, v: &[u8]) -> Vec<u8> {
        if self.imp == Imp::CaseIgnoreList {
            v.iter()
                .map(|&c| if c == b'$' { b' ' } else { c })
                .collect()
        } else {
            v.to_vec()
        }
    }

    /// A substring component prepared as [`Rule::substring_target`]'s.
    ///
    /// # Errors
    /// As [`Rule::substring_target`].
    pub fn substring_part(&self, v: &[u8], part: Part) -> Result<String, MatchError> {
        let case = self.substring_case()?;
        Ok(self.strip(prep::prepare_part(&self.lines(v), case, part)?))
    }

    /// Whether `value` matches a substring assertion under this SUBSTR
    /// rule, or `None` for Undefined.
    #[must_use]
    pub fn substrings(&self, value: &[u8], f: &ldap::filter::Substrings) -> Option<bool> {
        let target = self.substring_target(value).ok()?;
        let initial = match &f.initial {
            Some(i) => Some(self.substring_part(i, Part::Initial).ok()?),
            None => None,
        };
        let fin = match &f.fin {
            Some(x) => Some(self.substring_part(x, Part::Final).ok()?),
            None => None,
        };
        let any: Option<Vec<String>> = f
            .any
            .iter()
            .map(|a| self.substring_part(a, Part::Any).ok())
            .collect();
        Some(prep::match_parts(
            &target,
            initial.as_deref(),
            &any?,
            fin.as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ldap::filter::Substrings;

    struct Ctx;
    impl Context for Ctx {
        fn oid_of(&self, d: &str) -> Option<String> {
            d.eq_ignore_ascii_case("cn").then(|| "2.5.4.3".into())
        }
        fn normalize_dn(&self, dn: &str) -> Option<Vec<u8>> {
            Some(dn.to_ascii_lowercase().replace(' ', "").into_bytes())
        }
    }

    fn r(n: &str) -> &'static Rule {
        find(n).unwrap()
    }

    fn eq(n: &str, v: &str, a: &str) -> Option<bool> {
        r(n).equals(&Ctx, v.as_bytes(), a.as_bytes())
    }

    #[test]
    fn strings() {
        assert_eq!(eq("caseIgnoreMatch", "  Foo   BAR ", "foo bar"), Some(true));
        assert_eq!(eq("caseExactMatch", "Foo  Bar", "Foo Bar"), Some(true));
        assert_eq!(eq("caseExactMatch", "Foo", "foo"), Some(false));
        assert_eq!(eq("caseIgnoreIA5Match", "Admin", "ADMIN"), Some(true));
        assert_eq!(eq("caseExactIA5Match", "Admin", "ADMIN"), Some(false));
        assert_eq!(
            eq("caseIgnoreListMatch", "1 Main St $ Town", "1 MAIN ST$town"),
            Some(true)
        );
        assert_eq!(eq("caseIgnoreMatch", "a\u{E000}", "a"), None);
    }

    #[test]
    fn others() {
        assert_eq!(eq("integerMatch", "+0042", "42"), Some(true));
        assert_eq!(eq("integerMatch", "-0", "0"), Some(true));
        assert_eq!(eq("integerMatch", "x", "0"), None);
        assert_eq!(eq("booleanMatch", "true", "TRUE"), Some(true));
        assert_eq!(eq("numericStringMatch", "123 456", "123456"), Some(true));
        assert_eq!(
            eq("telephoneNumberMatch", "+1 507-555-0100", "+15075550100"),
            Some(true)
        );
        assert_eq!(eq("objectIdentifierMatch", "CN", "2.5.4.3"), Some(true));
        assert_eq!(
            eq("objectIdentifierMatch", "2.5.4.3", "2.5.4.4"),
            Some(false)
        );
        assert_eq!(
            eq(
                "generalizedTimeMatch",
                "20261113120000Z",
                "20261113130000+0100"
            ),
            Some(true)
        );
        assert_eq!(
            eq("uTCTimeMatch", "2611131200Z", "261113120000Z"),
            Some(true)
        );
        assert_eq!(eq("octetStringMatch", "Ab", "ab"), Some(false));
        assert_eq!(
            eq(
                "uuidMatch",
                "597AE2F6-16A6-1027-98F4-D28B5365DC14",
                "597ae2f6-16a6-1027-98f4-d28b5365dc14"
            ),
            Some(true)
        );
        assert_eq!(
            eq("uniqueMemberMatch", "CN=A, o=X#'01'B", "cn=a,o=x#'01'B"),
            Some(true)
        );
        assert_eq!(
            eq("uniqueMemberMatch", "CN=A, o=X#'01'B", "cn=a,o=x"),
            Some(false)
        );
        assert_eq!(
            eq("integerFirstComponentMatch", "( 5 NAME 'x' )", "5"),
            Some(true)
        );
        assert_eq!(
            eq(
                "objectIdentifierFirstComponentMatch",
                "( 2.5.4.3 NAME 'cn' )",
                "cn"
            ),
            Some(true)
        );
        assert_eq!(eq("wordMatch", "The quick fox", "QUICK"), Some(true));
        assert_eq!(eq("wordMatch", "The quick fox", "qui"), Some(false));
        assert_eq!(
            eq("keywordMatch", "The quick brown fox", "quick brown"),
            Some(true)
        );
    }

    #[test]
    fn ordering() {
        let o = |n: &str, a: &str, b: &str| r(n).compare(&Ctx, a.as_bytes(), b.as_bytes());
        assert_eq!(o("integerOrderingMatch", "-10", "-9"), Some(Ordering::Less));
        assert_eq!(
            o("integerOrderingMatch", "10", "9"),
            Some(Ordering::Greater)
        );
        assert_eq!(o("integerOrderingMatch", "-1", "0"), Some(Ordering::Less));
        assert_eq!(o("integerOrderingMatch", "0", "1"), Some(Ordering::Less));
        assert_eq!(
            o("integerOrderingMatch", "-100", "-99"),
            Some(Ordering::Less)
        );
        assert_eq!(
            o("caseIgnoreOrderingMatch", "abc", "ABD"),
            Some(Ordering::Less)
        );
        assert_eq!(
            o(
                "generalizedTimeOrderingMatch",
                "20261113120000Z",
                "20261113125959+0100"
            ),
            Some(Ordering::Greater)
        );
    }

    fn sub(i: Option<&str>, any: &[&str], f: Option<&str>) -> Substrings {
        Substrings {
            initial: i.map(|s| s.as_bytes().to_vec()),
            any: any.iter().map(|s| s.as_bytes().to_vec()).collect(),
            fin: f.map(|s| s.as_bytes().to_vec()),
        }
    }

    #[test]
    fn substrings() {
        let ci = r("caseIgnoreSubstringsMatch");
        assert_eq!(
            ci.substrings(b"Barbara  Jensen", &sub(Some("bar"), &["a j"], Some("SEN"))),
            Some(true)
        );
        assert_eq!(
            ci.substrings(b"Barbara Jensen", &sub(Some("jen"), &[], None)),
            Some(false)
        );
        let ce = r("caseExactSubstringsMatch");
        assert_eq!(
            ce.substrings(b"Barbara", &sub(Some("bar"), &[], None)),
            Some(false)
        );
        let tel = r("telephoneNumberSubstringsMatch");
        assert_eq!(
            tel.substrings(b"+1 507-555-0100", &sub(None, &["5550"], None)),
            Some(true)
        );
        let num = r("numericStringSubstringsMatch");
        assert_eq!(
            num.substrings(b"12 34 56", &sub(Some("1234"), &[], Some("56"))),
            Some(true)
        );
        assert_eq!(
            r("integerMatch").substrings(b"1", &sub(Some("1"), &[], None)),
            None
        );
    }
}
