// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDAP syntaxes (RFC 4517 §3.3 and the others the shipped attributes
//! name), each with a validator. A syntax QS/400 does not know accepts
//! any value and compares octet by octet.

use super::time;

/// The prefix of RFC 4517's syntax OIDs.
pub const PREFIX: &str = "1.3.6.1.4.1.1466.115.121.1.";

/// How values of a syntax are checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// Any bytes.
    Any,
    /// Non-empty UTF-8.
    Utf8,
    /// Non-empty IA5 (ASCII).
    Ia5,
    /// Non-empty Printable String.
    Printable,
    /// `TRUE` or `FALSE` (any case, as IBM's server takes them).
    Boolean,
    /// An INTEGER.
    Integer,
    /// A Bit String (`'0101'B`).
    BitString,
    /// A Country String: two printable characters.
    Country,
    /// A DN.
    Dn,
    /// A Name and Optional UID.
    NameUid,
    /// A Numeric String.
    Numeric,
    /// An OID: a numeric OID or a descriptor.
    Oid,
    /// A Generalized Time.
    GeneralizedTime,
    /// A UTC Time.
    UtcTime,
    /// A Telephone Number (printable).
    Telephone,
    /// A Postal Address: lines separated by `$`.
    PostalAddress,
    /// A Delivery Method.
    DeliveryMethod,
    /// A schema description: a parenthesized form.
    Description,
    /// A UUID (RFC 4530).
    Uuid,
}

/// A syntax QS/400 knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax {
    /// The OID.
    pub oid: &'static str,
    /// Quartz's description.
    pub desc: &'static str,
    /// The validator.
    pub check: Check,
    /// Not human-readable: transferred with `;binary` where RFC 4522
    /// requires it.
    pub binary: bool,
}

macro_rules! syn {
    ($n:literal, $desc:literal, $check:ident) => {
        Syntax {
            oid: concat!("1.3.6.1.4.1.1466.115.121.1.", $n),
            desc: $desc,
            check: Check::$check,
            binary: false,
        }
    };
    ($n:literal, $desc:literal, $check:ident, bin) => {
        Syntax {
            oid: concat!("1.3.6.1.4.1.1466.115.121.1.", $n),
            desc: $desc,
            check: Check::$check,
            binary: true,
        }
    };
}

/// Every syntax QS/400 knows: RFC 4517's, the older ones IBM's schema
/// lists, RFC 4530's UUID and IBM's own.
pub const SYNTAXES: &[Syntax] = &[
    syn!("3", "Attribute type description", Description),
    syn!("4", "Audio", Any, bin),
    syn!("5", "Binary data", Any, bin),
    syn!("6", "Bit string", BitString),
    syn!("7", "Boolean", Boolean),
    syn!("8", "X.509 certificate", Any, bin),
    syn!("9", "X.509 certificate list", Any, bin),
    syn!("10", "X.509 certificate pair", Any, bin),
    syn!("11", "Two-letter country code", Country),
    syn!("12", "Distinguished name", Dn),
    syn!("14", "Delivery method", DeliveryMethod),
    syn!("15", "Directory string (UTF-8)", Utf8),
    syn!("16", "DIT content rule description", Description),
    syn!("17", "DIT structure rule description", Description),
    syn!("21", "Enhanced guide", Utf8),
    syn!("22", "Facsimile telephone number", Utf8),
    syn!("23", "Facsimile image", Any, bin),
    syn!("24", "Generalized time", GeneralizedTime),
    syn!("25", "Guide", Utf8),
    syn!("26", "IA5 string", Ia5),
    syn!("27", "Integer", Integer),
    syn!("28", "JPEG image", Any, bin),
    syn!("30", "Matching rule description", Description),
    syn!("31", "Matching rule use description", Description),
    syn!("33", "MHS O/R address", Utf8),
    syn!("34", "Name and optional unique ID", NameUid),
    syn!("35", "Name form description", Description),
    syn!("36", "Numeric string", Numeric),
    syn!("37", "Object class description", Description),
    syn!("38", "Object identifier", Oid),
    syn!("39", "Other mailbox", Utf8),
    syn!("40", "Octet string", Any),
    syn!("41", "Postal address", PostalAddress),
    syn!("42", "Protocol information", Utf8),
    syn!("43", "Presentation address", Utf8),
    syn!("44", "Printable string", Printable),
    syn!("49", "Supported algorithm", Any, bin),
    syn!("50", "Telephone number", Telephone),
    syn!("51", "Teletex terminal identifier", Utf8),
    syn!("52", "Telex number", Utf8),
    syn!("53", "UTC time", UtcTime),
    syn!("54", "LDAP syntax description", Description),
    syn!("58", "Substring assertion", Any),
    Syntax {
        oid: "1.3.6.1.1.16.1",
        desc: "UUID",
        check: Check::Uuid,
        binary: false,
    },
    Syntax {
        oid: "1.3.18.0.2.8.1",
        desc: "IBM directory string with an order",
        check: Check::Utf8,
        binary: false,
    },
];

/// The syntax with this OID.
#[must_use]
pub fn find(oid: &str) -> Option<&'static Syntax> {
    SYNTAXES.iter().find(|s| s.oid == oid)
}

/// Whether `c` is a `PrintableCharacter` (RFC 4517 §3.2).
fn printable(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b" '()+,-./:?=".contains(&c)
}

/// Whether `s` is a numeric OID.
#[must_use]
pub fn is_numeric_oid(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').count() >= 2
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.bytes().all(|c| c.is_ascii_digit())
                && (p == "0" || !p.starts_with('0'))
        })
}

/// Whether `s` is a descriptor (`keystring`, RFC 4512 §1.4). IBM's
/// names may hold `_` too.
#[must_use]
pub fn is_descr(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_alphabetic()
        && b.iter()
            .all(|&c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

/// Whether `v` is an INTEGER's string form.
#[must_use]
pub fn is_integer(v: &[u8]) -> bool {
    let d = v.strip_prefix(b"-").unwrap_or(v);
    !d.is_empty() && d.iter().all(u8::is_ascii_digit) && (d == b"0" || d[0] != b'0') && v != b"-0"
}

fn is_bitstring(v: &[u8]) -> bool {
    v.len() >= 3
        && v[0] == b'\''
        && v.ends_with(b"'B")
        && v[1..v.len() - 2].iter().all(|&c| c == b'0' || c == b'1')
}

const DELIVERY: &[&str] = &[
    "any",
    "mhs",
    "physical",
    "telex",
    "teletex",
    "g3fax",
    "g4fax",
    "ia5",
    "videotex",
    "telephone",
];

impl Check {
    /// Whether `v` is a valid value. `dn` validates a DN string.
    #[must_use]
    pub fn valid(self, v: &[u8], dn: &dyn Fn(&str) -> bool) -> bool {
        let text = std::str::from_utf8(v);
        match self {
            Self::Any => true,
            Self::Utf8 => text.is_ok() && !v.is_empty(),
            Self::Ia5 => !v.is_empty() && v.is_ascii(),
            Self::Printable => !v.is_empty() && v.iter().all(|&c| printable(c)),
            Self::Telephone => {
                !v.is_empty() && v.iter().all(|&c| printable(c) || c == b'#' || c == b'*')
            }
            Self::Boolean => v.eq_ignore_ascii_case(b"TRUE") || v.eq_ignore_ascii_case(b"FALSE"),
            Self::Integer => is_integer(v),
            Self::BitString => is_bitstring(v),
            Self::Country => v.len() == 2 && v.iter().all(|&c| printable(c)),
            Self::Dn => text.is_ok_and(dn),
            Self::NameUid => text.is_ok_and(|t| match split_uid(t) {
                (name, Some(bits)) => dn(name) && is_bitstring(bits.as_bytes()),
                (name, None) => dn(name),
            }),
            Self::Numeric => !v.is_empty() && v.iter().all(|&c| c.is_ascii_digit() || c == b' '),
            Self::Oid => text.is_ok_and(|t| is_numeric_oid(t) || is_descr(t)),
            Self::GeneralizedTime => time::parse_generalized(v, false).is_some(),
            Self::UtcTime => time::parse_utc(v, false).is_some(),
            Self::PostalAddress => {
                text.is_ok_and(|t| !t.is_empty() && t.split('$').all(|l| !l.is_empty()))
            }
            Self::DeliveryMethod => text.is_ok_and(|t| {
                t.split('$')
                    .all(|p| DELIVERY.contains(&p.trim().to_ascii_lowercase().as_str()))
            }),
            Self::Description => text.is_ok_and(|t| {
                let t = t.trim();
                t.starts_with('(') && t.ends_with(')')
            }),
            Self::Uuid => is_uuid(v),
        }
    }
}

/// A Name and Optional UID split at its `#'bits'B` (RFC 4517 §3.3.21).
#[must_use]
pub fn split_uid(t: &str) -> (&str, Option<&str>) {
    if let Some(p) = t.rfind('#') {
        let bits = &t[p + 1..];
        if is_bitstring(bits.as_bytes()) {
            return (&t[..p], Some(bits));
        }
    }
    (t, None)
}

fn is_uuid(v: &[u8]) -> bool {
    v.len() == 36
        && v.iter().enumerate().all(|(i, &c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(c: Check, v: &str) -> bool {
        c.valid(v.as_bytes(), &|s| s.contains('='))
    }

    #[test]
    fn validators() {
        assert!(ok(Check::Integer, "-12") && ok(Check::Integer, "0"));
        assert!(
            !ok(Check::Integer, "012") && !ok(Check::Integer, "-0") && !ok(Check::Integer, "1a")
        );
        assert!(
            ok(Check::Boolean, "TRUE") && ok(Check::Boolean, "false") && !ok(Check::Boolean, "yes")
        );
        assert!(ok(Check::BitString, "'0101'B") && !ok(Check::BitString, "'012'B"));
        assert!(ok(Check::Ia5, "a@b.c") && !ok(Check::Ia5, "é"));
        assert!(ok(Check::Printable, "Foo (Bar)") && !ok(Check::Printable, "a@b"));
        assert!(ok(Check::Country, "US") && !ok(Check::Country, "USA"));
        assert!(ok(Check::Numeric, "123 45") && !ok(Check::Numeric, "12-3"));
        assert!(
            ok(Check::Oid, "2.5.4.3")
                && ok(Check::Oid, "cn")
                && !ok(Check::Oid, "2.05")
                && !ok(Check::Oid, "1cn")
        );
        assert!(
            ok(Check::GeneralizedTime, "20261113120000Z")
                && !ok(Check::GeneralizedTime, "20261113")
        );
        assert!(ok(Check::NameUid, "cn=a#'01'B") && ok(Check::NameUid, "cn=a#b"));
        assert!(ok(Check::PostalAddress, "1 Main St$Town") && !ok(Check::PostalAddress, "a$$b"));
        assert!(
            ok(Check::DeliveryMethod, "telephone $ mhs") && !ok(Check::DeliveryMethod, "pigeon")
        );
        assert!(
            ok(Check::Uuid, "597ae2f6-16a6-1027-98f4-d28b5365dc14") && !ok(Check::Uuid, "597ae2f6")
        );
        assert!(ok(Check::Description, "( 1.2.3 NAME 'x' )"));
        assert!(!ok(Check::Utf8, ""));
    }
}
