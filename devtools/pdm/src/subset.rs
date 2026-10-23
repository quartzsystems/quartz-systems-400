// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Subsetting a list (F17) and positioning it: generic names
//! (`ABC*`), `*ALL`, types, attributes, text and change dates.

use crate::PdmError;
use crate::subst::Item;

/// Whether `value` matches a name pattern: `*ALL` or blank, a
/// generic name ending in `*`, or the name itself; case ignored.
#[must_use]
pub fn matches(pattern: &str, value: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    let v = value.trim().to_ascii_uppercase();
    if p.is_empty() || p == "*ALL" {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => v.starts_with(prefix),
        None => v == p,
    }
}

/// A subset of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subset {
    /// The name pattern.
    pub name: String,
    /// The type pattern (`*ALL`, `*PGM`, `CLLE`, `PROD`).
    pub ty: String,
    /// The attribute pattern (objects only).
    pub attr: String,
    /// Text the entry's text must contain; `*ALL` for any.
    pub text: String,
    /// Members changed on or after, `YYYYMMDD`; `None` for any.
    pub from_date: Option<u32>,
    /// Members changed on or before, `YYYYMMDD`.
    pub to_date: Option<u32>,
}

impl Default for Subset {
    fn default() -> Self {
        Self {
            name: "*ALL".into(),
            ty: "*ALL".into(),
            attr: "*ALL".into(),
            text: "*ALL".into(),
            from_date: None,
            to_date: None,
        }
    }
}

impl Subset {
    /// Whether the subset keeps an item, changed on `date` (`YYYYMMDD`,
    /// 0 when unknown).
    #[must_use]
    pub fn keeps(&self, item: &Item, date: u32) -> bool {
        let text = self.text.trim();
        matches(&self.name, &item.name)
            && matches(&self.ty, &item.ty)
            && matches(&self.attr, &item.attr)
            && (text.is_empty()
                || text.eq_ignore_ascii_case("*ALL")
                || item
                    .text
                    .to_ascii_uppercase()
                    .contains(&text.to_ascii_uppercase()))
            && self.from_date.is_none_or(|f| date >= f)
            && self.to_date.is_none_or(|t| date <= t)
    }

    /// Whether this is the whole list.
    #[must_use]
    pub fn is_all(&self) -> bool {
        *self == Self::default()
    }
}

/// `MMDDYY` (as typed on the subset panel) as `YYYYMMDD`; blank is
/// `None`.
///
/// # Errors
///
/// Not six digits, or not a date.
#[allow(clippy::many_single_char_names)] // month, day, year
pub fn date(mmddyy: &str) -> Result<Option<u32>, PdmError> {
    let bad = || PdmError::BadValue {
        field: "date",
        value: mmddyy.trim().to_string(),
    };
    let t: String = mmddyy.chars().filter(char::is_ascii_digit).collect();
    if mmddyy.trim().is_empty() {
        return Ok(None);
    }
    if t.len() != 6 {
        return Err(bad());
    }
    let n: u32 = t.parse().map_err(|_| bad())?;
    let (m, d, y) = (n / 10_000, n / 100 % 100, n % 100);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    let y = if y < 40 { 2000 + y } else { 1900 + y };
    Ok(Some(y * 10_000 + m * 100 + d))
}

/// `CYYMMDD...` (the store's stamps) as `YYYYMMDD`; 0 when not one.
#[must_use]
pub fn stamp_date(s: &str) -> u32 {
    let b = s.as_bytes();
    if b.len() < 7 || !b[..7].iter().all(u8::is_ascii_digit) {
        return 0;
    }
    let n: u32 = s[..7].parse().unwrap_or(0);
    19_000_000 + n
}

/// The index of the first name at or after `target` (case ignored) in
/// a list sorted by name; `None` when all are before it.
#[must_use]
pub fn position<'a>(names: impl IntoIterator<Item = &'a str>, target: &str) -> Option<usize> {
    let t = target.trim().to_ascii_uppercase();
    names
        .into_iter()
        .position(|n| n.trim().to_ascii_uppercase() >= t)
}

/// F13: the first option typed is copied to every entry after it that
/// has none. `options` holds `(entry, option)` sorted by entry.
#[must_use]
pub fn repeat(options: &[(usize, String)], len: usize) -> Vec<(usize, String)> {
    let Some((first, opt)) = options.first().cloned() else {
        return Vec::new();
    };
    let mut out: Vec<(usize, String)> = options.to_vec();
    for i in first + 1..len {
        if !out.iter().any(|(e, _)| *e == i) {
            out.push((i, opt.clone()));
        }
    }
    out.sort_by_key(|(e, _)| *e);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, ty: &str, text: &str) -> Item {
        Item {
            name: name.into(),
            ty: ty.into(),
            text: text.into(),
            ..Item::default()
        }
    }

    #[test]
    fn patterns() {
        assert!(matches("*ALL", "X"));
        assert!(matches("", "X"));
        assert!(matches("ab*", "ABC"));
        assert!(!matches("ab*", "XAB"));
        assert!(matches("abc", "ABC"));
        assert!(!matches("abc", "ABCD"));
    }

    #[test]
    fn subsets() {
        let s = Subset {
            name: "H*".into(),
            ty: "CLLE".into(),
            text: "greet".into(),
            from_date: Some(20_260_101),
            ..Subset::default()
        };
        assert!(s.keeps(&item("HELLO", "CLLE", "Greeting"), 20_261_023));
        assert!(!s.keeps(&item("HELLO", "CLLE", "Greeting"), 20_251_023));
        assert!(!s.keeps(&item("BYE", "CLLE", "Greeting"), 20_261_023));
        assert!(!s.keeps(&item("HELLO", "PF", "Greeting"), 20_261_023));
        assert!(Subset::default().is_all());
        assert_eq!(date("102326"), Ok(Some(20_261_023)));
        assert_eq!(date(""), Ok(None));
        assert!(date("1323").is_err());
        assert_eq!(stamp_date("1261023101112"), 20_261_023);
    }

    #[test]
    fn position_and_repeat() {
        assert_eq!(position(["A", "C", "E"], "b"), Some(1));
        assert_eq!(position(["A", "C", "E"], "F"), None);
        let r = repeat(&[(1, "5".into()), (3, "4".into())], 5);
        assert_eq!(
            r,
            [
                (1, "5".into()),
                (2, "5".into()),
                (3, "4".into()),
                (4, "5".into())
            ]
        );
        assert!(repeat(&[], 3).is_empty());
    }
}
