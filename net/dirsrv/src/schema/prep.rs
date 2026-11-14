// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! String preparation (RFC 4518) for the string matching rules: map,
//! fold, prohibit, insignificant spaces. NFKC normalization is not
//! applied (docs/DIRSRV.md §6).

/// Whether case is folded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    /// Case-exact rules.
    Exact,
    /// Case-ignore rules.
    Fold,
}

/// Why a value cannot be prepared: it is then undefined to every rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrepError {
    /// Not UTF-8.
    #[error("value is not UTF-8")]
    NotUtf8,
    /// A prohibited character (RFC 4518 §2.4).
    #[error("value holds a prohibited character U+{0:04X}")]
    Prohibited(u32),
}

/// RFC 4518 §2.2: characters mapped to nothing.
fn maps_to_nothing(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{1806}' | '\u{034F}' | '\u{180B}'..='\u{180D}'
        | '\u{FE00}'..='\u{FE0F}' | '\u{FFFC}' | '\u{200B}'
        | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}')
        || (c.is_control() && !matches!(c, '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}'))
}

/// RFC 4518 §2.2: characters mapped to a space.
fn maps_to_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | '\u{85}'
            | ' '
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

/// RFC 4518 §2.4: prohibited output (private use, non-characters,
/// the replacement character).
fn prohibited(c: char) -> bool {
    let u = c as u32;
    matches!(u, 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD | 0xFFFD | 0xFDD0..=0xFDEF)
        || (u & 0xFFFE) == 0xFFFE
}

/// Map, fold and check, without the space step: the characters of the
/// value with every space-like character as one ' '.
fn map(value: &[u8], case: Case) -> Result<Vec<char>, PrepError> {
    let s = std::str::from_utf8(value).map_err(|_| PrepError::NotUtf8)?;
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if maps_to_nothing(c) {
            continue;
        }
        if maps_to_space(c) {
            out.push(' ');
            continue;
        }
        if prohibited(c) {
            return Err(PrepError::Prohibited(c as u32));
        }
        match case {
            Case::Exact => out.push(c),
            Case::Fold => out.extend(c.to_lowercase()),
        }
    }
    Ok(out)
}

/// Runs of spaces collapsed to one, none at either end unless kept.
fn squeeze(chars: &[char], keep_lead: bool, keep_trail: bool) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut space = false;
    for &c in chars {
        if c == ' ' {
            space = true;
            continue;
        }
        if space && (!out.is_empty() || keep_lead) {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    if space && (!out.is_empty() || keep_lead) && keep_trail {
        out.push(' ');
    }
    out
}

/// A value prepared for equality and ordering: leading and trailing
/// spaces dropped, inner runs one space (equivalent, for comparison, to
/// RFC 4518 §2.6.1's form).
///
/// # Errors
/// [`PrepError`] when the value is not UTF-8 or holds a prohibited
/// character.
pub fn prepare(value: &[u8], case: Case) -> Result<String, PrepError> {
    Ok(squeeze(&map(value, case)?, false, false))
}

/// Where a substring component sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// `initial`.
    Initial,
    /// `any`.
    Any,
    /// `final`.
    Final,
}

/// A substring assertion component prepared to match against
/// [`substring_target`]: an initial part gains a leading space, a final
/// part a trailing one, so they meet the value's ends.
///
/// # Errors
/// As [`prepare`].
pub fn prepare_part(value: &[u8], case: Case, part: Part) -> Result<String, PrepError> {
    let chars = map(value, case)?;
    Ok(match part {
        Part::Initial => format!(" {}", squeeze(&chars, false, true)),
        Part::Final => format!("{} ", squeeze(&chars, true, false)),
        Part::Any => {
            let lead = chars.first() == Some(&' ');
            let trail = chars.last() == Some(&' ');
            let inner = squeeze(&chars, false, false);
            let mut s = String::new();
            if lead {
                s.push(' ');
            }
            s.push_str(&inner);
            if trail && !inner.is_empty() {
                s.push(' ');
            }
            s
        }
    })
}

/// The value a substring assertion is matched against: the prepared
/// value with one space at each end.
///
/// # Errors
/// As [`prepare`].
pub fn substring_target(value: &[u8], case: Case) -> Result<String, PrepError> {
    Ok(format!(" {} ", prepare(value, case)?))
}

/// Whether prepared components match a prepared target, in order and
/// without overlap.
#[must_use]
pub fn match_parts(target: &str, initial: Option<&str>, any: &[String], fin: Option<&str>) -> bool {
    let mut rest = target;
    if let Some(i) = initial {
        match rest.strip_prefix(i) {
            Some(r) => rest = r,
            None => return false,
        }
    }
    // The final part is taken from the end first so `any` cannot use it.
    let end = match fin {
        Some(f) => {
            if rest.len() < f.len() || !rest.ends_with(f) {
                return false;
            }
            rest.len() - f.len()
        }
        None => rest.len(),
    };
    let mut middle = &rest[..end];
    for a in any {
        if a.is_empty() {
            continue;
        }
        match middle.find(a.as_str()) {
            Some(p) => middle = &middle[p + a.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> String {
        prepare(s.as_bytes(), Case::Fold).unwrap()
    }

    #[test]
    fn spaces_are_insignificant() {
        assert_eq!(p("  Foo   Bar  "), "foo bar");
        assert_eq!(p("foo\u{00A0}\tbar"), "foo bar");
        assert_eq!(p("   "), "");
        assert_eq!(p(""), "");
    }

    #[test]
    fn mapped_to_nothing() {
        assert_eq!(p("co\u{00AD}operate"), "cooperate");
        assert_eq!(p("a\u{200B}b"), "ab");
        assert_eq!(p("a\u{0001}b"), "ab");
    }

    #[test]
    fn case_folds() {
        assert_eq!(p("ÄÖÜ Straße"), "äöü straße");
        assert_eq!(prepare("ABC".as_bytes(), Case::Exact).unwrap(), "ABC");
    }

    #[test]
    fn prohibited_and_bad_utf8() {
        assert_eq!(
            prepare("a\u{E000}".as_bytes(), Case::Fold),
            Err(PrepError::Prohibited(0xE000))
        );
        assert_eq!(prepare(&[0xFF], Case::Fold), Err(PrepError::NotUtf8));
    }

    #[test]
    fn substrings() {
        let t = substring_target(b"  Foo   Bar ", Case::Fold).unwrap();
        assert_eq!(t, " foo bar ");
        let i = prepare_part(b"FOO ", Case::Fold, Part::Initial).unwrap();
        let f = prepare_part(b"  bar", Case::Fold, Part::Final).unwrap();
        assert!(match_parts(&t, Some(&i), &[], None));
        assert!(match_parts(&t, None, &[], Some(&f)));
        let a = prepare_part(b"o  b", Case::Fold, Part::Any).unwrap();
        assert!(match_parts(&t, None, &[a], None));
        let no = prepare_part(b"fooba", Case::Fold, Part::Initial).unwrap();
        assert!(!match_parts(&t, Some(&no), &[], None));
        // Initial and final cannot overlap.
        let t = substring_target(b"aba", Case::Fold).unwrap();
        let i = prepare_part(b"ab", Case::Fold, Part::Initial).unwrap();
        let f = prepare_part(b"ba", Case::Fold, Part::Final).unwrap();
        assert!(!match_parts(&t, Some(&i), &[], Some(&f)));
    }
}
