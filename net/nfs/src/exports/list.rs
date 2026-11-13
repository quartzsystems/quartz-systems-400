// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An option list, `name[=value],name[=value]...`, split into items
//! with the position each starts at (blanks around a comma are
//! allowed, as IBM's examples write them).

use super::error::OptError;

/// One item of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<'a> {
    /// The name, lower case.
    pub name: String,
    /// The value after `=`, if any.
    pub value: Option<&'a str>,
    /// Where the item starts, from 1.
    pub pos: usize,
}

/// Split `text` into items; an empty item, an empty name or a name that
/// is not letters is `CPFA1B0` at its position. A name given twice is
/// `CPFA1B1`.
pub fn split(text: &str) -> Result<Vec<Item<'_>>, OptError> {
    let mut out: Vec<Item<'_>> = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    loop {
        let end = text[start..].find(',').map_or(text.len(), |i| start + i);
        let raw = &text[start..end];
        let lead = raw.len() - raw.trim_start().len();
        let item = raw.trim();
        let pos = text[..start + lead].chars().count() + 1;
        if item.is_empty() {
            // An empty list is no items; an empty item between commas
            // is not written correctly.
            if !(out.is_empty() && end == text.len()) {
                return Err(OptError::Unknown { pos });
            }
        } else {
            let (name, value) = match item.split_once('=') {
                Some((n, v)) => (n.trim(), Some(v.trim())),
                None => (item, None),
            };
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphabetic()) {
                return Err(OptError::Unknown { pos });
            }
            let name = name.to_ascii_lowercase();
            // `root` alone (the pseudo root) and `root=hosts` are two
            // options of the export list.
            let same = |i: &Item<'_>| {
                i.name == name && (name != "root" || i.value.is_some() == value.is_some())
            };
            if out.iter().any(same) {
                return Err(OptError::Duplicate { name });
            }
            out.push(Item { name, value, pos });
        }
        if end >= bytes.len() {
            break;
        }
        start = end + 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_and_positions() {
        let v = split("ro, anon=0,rw=a:b").unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].name.as_str(), v[0].value, v[0].pos), ("ro", None, 1));
        assert_eq!(
            (v[1].name.as_str(), v[1].value, v[1].pos),
            ("anon", Some("0"), 5)
        );
        assert_eq!(v[2].value, Some("a:b"));
        assert!(split("").unwrap().is_empty());
        assert!(split("   ").unwrap().is_empty());
    }

    #[test]
    fn refusals() {
        assert_eq!(split("ro,,rw"), Err(OptError::Unknown { pos: 4 }));
        assert_eq!(split("ro,"), Err(OptError::Unknown { pos: 4 }));
        assert_eq!(split("r0"), Err(OptError::Unknown { pos: 1 }));
        assert_eq!(split("ro, =3"), Err(OptError::Unknown { pos: 5 }));
        assert_eq!(
            split("RO,ro"),
            Err(OptError::Duplicate { name: "ro".into() })
        );
    }
}
