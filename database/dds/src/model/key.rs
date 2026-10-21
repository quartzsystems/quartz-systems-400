// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A key field of an access path.

use qsname::Name;

use super::DataType;

/// How the key bytes are formed from the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyTransform {
    /// `SIGNED`: numeric order, the default for numeric fields.
    Signed,
    /// `UNSIGNED`: byte order, the default for character fields.
    Unsigned,
    /// `ABSVAL`: the sign is ignored.
    Absval,
    /// `ZONE`: only the zone half of each byte.
    Zone,
    /// `DIGIT`: only the digit half of each byte.
    Digit,
}

impl KeyTransform {
    /// The default for a field type.
    #[must_use]
    pub fn default_for(t: DataType) -> Self {
        if t.is_numeric() {
            Self::Signed
        } else {
            Self::Unsigned
        }
    }

    /// The keyword.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Signed => "SIGNED",
            Self::Unsigned => "UNSIGNED",
            Self::Absval => "ABSVAL",
            Self::Zone => "ZONE",
            Self::Digit => "DIGIT",
        }
    }
}

/// One key field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyField {
    /// The field name.
    pub name: Name,
    /// Index of the field in the record format.
    pub field: usize,
    /// `DESCEND`.
    pub descend: bool,
    /// How the key bytes are formed.
    pub transform: KeyTransform,
    /// `NOALTSEQ`.
    pub noaltseq: bool,
    /// Source line index of the `K` specification.
    pub line: usize,
}

impl KeyField {
    /// `Descending, signed` and the like, for listings.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut s = String::from(if self.descend {
            "Descending"
        } else {
            "Ascending"
        });
        s.push_str(", ");
        s.push_str(&self.transform.name().to_ascii_lowercase());
        if self.noaltseq {
            s.push_str(", no ALTSEQ");
        }
        s
    }
}
