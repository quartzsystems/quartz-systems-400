// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bodies of `VARIABLE` (a global variable's `*SRVPGM`) and `TYPE`
//! (a distinct type's `*SQLUDT`) entries (`docs/DB_FORMAT.md` §8.1,
//! `SqlVariable`, `SqlType`). Types are kept as SQL writes them.

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// Bytes of the fixed part of a `VARIABLE` body.
pub const VARIABLE_FIXED: usize = 16;

/// The `VARIABLE` body layout.
pub static VARIABLE_LAYOUT: Layout = Layout {
    name: "SqlVariable",
    size: None,
    fields: &[
        field("flags", 0, 1),
        field("reserved", 1, 1),
        field("type_len", 2, 2),
        field("default_len", 4, 4),
        field("reserved2", 8, 8),
        Field { name: "parts", offset: Some(16), size: None },
    ],
};

/// Bytes of the fixed part of a `TYPE` body.
pub const TYPE_FIXED: usize = 16;

/// The `TYPE` body layout.
pub static TYPE_LAYOUT: Layout = Layout {
    name: "SqlType",
    size: None,
    fields: &[
        field("metatype", 0, 1),
        field("reserved", 1, 1),
        field("source_len", 2, 2),
        field("reserved2", 4, 12),
        Field { name: "parts", offset: Some(16), size: None },
    ],
};

/// A global variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variable {
    /// Its type, as SQL writes it (`DECIMAL(9, 2)`).
    pub data_type: String,
    /// `DEFAULT`, as written; `None` for none (null).
    pub default: Option<String>,
    /// `CONSTANT`: the default, never set.
    pub constant: bool,
}

fn bad(what: &str) -> DbError {
    DbError::Corrupt { what: format!("SQL information: {what}") }
}

fn len16(n: usize) -> [u8; 2] {
    u16::try_from(n).expect("short").to_le_bytes()
}

impl Variable {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let d = self.default.as_deref().unwrap_or("");
        let mut b = vec![0u8; VARIABLE_FIXED];
        b[0] = u8::from(self.constant) | u8::from(self.default.is_some()) << 1;
        b[2..4].copy_from_slice(&len16(self.data_type.len()));
        b[4..8].copy_from_slice(&u32::try_from(d.len()).expect("fits").to_le_bytes());
        b.extend_from_slice(self.data_type.as_bytes());
        b.extend_from_slice(d.as_bytes());
        b
    }

    /// From an entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < VARIABLE_FIXED {
            return Err(bad("variable entry"));
        }
        let t = usize::from(u16::from_le_bytes([b[2], b[3]]));
        let d = u32::from_le_bytes(b[4..8].try_into().expect("4")) as usize;
        let text = |s: Option<&[u8]>| s.and_then(|s| String::from_utf8(s.to_vec()).ok()).ok_or_else(|| bad("variable entry"));
        let data_type = text(b.get(VARIABLE_FIXED..VARIABLE_FIXED + t))?;
        let default = text(b.get(VARIABLE_FIXED + t..VARIABLE_FIXED + t + d))?;
        Ok(Self { data_type, default: (b[0] & 2 != 0).then_some(default), constant: b[0] & 1 != 0 })
    }
}

/// A user-defined type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Udt {
    /// `T` distinct (IBM's `METATYPE`).
    pub metatype: u8,
    /// The source type, as SQL writes it.
    pub source: String,
}

impl Udt {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; TYPE_FIXED];
        b[0] = self.metatype;
        b[2..4].copy_from_slice(&len16(self.source.len()));
        b.extend_from_slice(self.source.as_bytes());
        b
    }

    /// From an entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < TYPE_FIXED {
            return Err(bad("type entry"));
        }
        let n = usize::from(u16::from_le_bytes([b[2], b[3]]));
        let source = b
            .get(TYPE_FIXED..TYPE_FIXED + n)
            .and_then(|s| String::from_utf8(s.to_vec()).ok())
            .ok_or_else(|| bad("type entry"))?;
        Ok(Self { metatype: b[0], source })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for v in [
            Variable { data_type: "DECIMAL(9, 2)".into(), default: Some("1.5".into()), constant: false },
            Variable { data_type: "INTEGER".into(), default: None, constant: true },
            Variable { data_type: "VARCHAR(20)".into(), default: Some(String::new()), constant: false },
        ] {
            assert_eq!(Variable::decode(&v.encode()).unwrap(), v);
        }
        let u = Udt { metatype: b'T', source: "CHARACTER(4)".into() };
        assert_eq!(Udt::decode(&u.encode()).unwrap(), u);
    }
}
