// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The format level identifier. See `docs/DB_FORMAT.md` §5; the
//! algorithm does not change within V1R.

use std::fmt::Write as _;

use dds::FieldDef;
use sha2::{Digest, Sha256};

/// Characters in a level identifier.
pub const LEVEL_ID_LEN: usize = 13;

/// The 13-character level identifier of a format with these fields.
#[must_use]
pub fn fmt_level_id(fields: &[FieldDef]) -> String {
    let mut h = Sha256::new();
    for f in fields {
        h.update(f.name.as_bytes());
        h.update([f.data_type.letter() as u8]);
        h.update(f.length.to_be_bytes());
        h.update([f.decimals]);
        h.update(f.ccsid.unwrap_or(0).to_be_bytes());
        h.update([u8::from(f.allow_null), u8::from(f.varlen.is_some())]);
    }
    let digest = h.finalize();
    let mut s = String::with_capacity(LEVEL_ID_LEN + 1);
    for b in &digest[..7] {
        let _ = write!(s, "{b:02X}");
    }
    s.truncate(LEVEL_ID_LEN);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use dds::DataType;
    use qsname::Name;

    fn f(name: &str, t: DataType, len: u32, dec: u8) -> FieldDef {
        FieldDef::new(Name::parse(name).unwrap(), t, len, dec)
    }

    #[test]
    fn stable_and_sensitive() {
        let base = vec![
            f("CUSNO", DataType::Packed, 7, 0),
            f("NAME", DataType::Char, 30, 0),
        ];
        let id = fmt_level_id(&base);
        assert_eq!(id.len(), 13);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase())
        );
        // The value is fixed for V1R: a change here breaks every file.
        assert_eq!(id, "F138D74F083F9");
        let mut longer = base.clone();
        longer[1].length = 31;
        assert_ne!(fmt_level_id(&longer), id);
        let mut nullable = base.clone();
        nullable[0].allow_null = true;
        assert_ne!(fmt_level_id(&nullable), id);
        let mut swapped = base.clone();
        swapped.swap(0, 1);
        assert_ne!(fmt_level_id(&swapped), id);
        let mut texted = base.clone();
        texted[0].text = Some("text does not matter".into());
        assert_eq!(fmt_level_id(&texted), id);
        assert_eq!(fmt_level_id(&[]).len(), 13);
    }
}
