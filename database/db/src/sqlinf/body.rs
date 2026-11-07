// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bodies of the `VIEW`, `INDEX` and `DEPENDENCY` entries. See
//! `docs/DB_FORMAT.md` §8.1.

use qsname::Name;

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// `SqlView` layout.
pub static VIEW_LAYOUT: Layout = Layout {
    name: "SqlView",
    size: None,
    fields: &[
        field("check_option", 0, 1),
        field("flags", 1, 1),
        field("column_count", 2, 2),
        field("query_len", 4, 4),
        Field {
            name: "query",
            offset: Some(8),
            size: None,
        },
        Field {
            name: "columns",
            offset: None,
            size: None,
        },
    ],
};

/// `SqlIndex` layout.
pub static INDEX_LAYOUT: Layout = Layout {
    name: "SqlIndex",
    size: None,
    fields: &[
        field("flags", 0, 1),
        field("reserved", 1, 1),
        field("key_count", 2, 2),
        field("include_count", 4, 2),
        field("reserved2", 6, 2),
        field("where_len", 8, 4),
        Field {
            name: "where",
            offset: Some(12),
            size: None,
        },
        Field {
            name: "keys",
            offset: None,
            size: None,
        },
        Field {
            name: "includes",
            offset: None,
            size: None,
        },
    ],
};

/// `SqlDependency` layout.
pub static DEPENDENCY_LAYOUT: Layout = Layout {
    name: "SqlDependency",
    size: Some(DEPENDENCY_LEN),
    fields: &[
        field("type_code", 0, 2),
        field("reserved", 2, 2),
        field("library", 4, 10),
        field("name", 14, 10),
        field("reserved2", 24, 8),
    ],
};

/// Bytes of an `SqlDependency`.
pub const DEPENDENCY_LEN: usize = 32;

/// `SqlTable` layout.
pub static TABLE_LAYOUT: Layout = Layout {
    name: "SqlTable",
    size: Some(TABLE_LEN),
    fields: &[field("flags", 0, 2), field("reserved", 2, 6)],
};

/// Bytes of an `SqlTable`.
pub const TABLE_LEN: usize = 8;

/// A table's SQL attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TableDef {
    /// `VOLATILE`: its row count changes too much for statistics.
    pub volatile: bool,
}

impl TableDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; TABLE_LEN];
        out[0] = u8::from(self.volatile);
        out
    }

    /// From the entry body; flags it does not know are left alone.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let head = b.get(..2).ok_or_else(|| bad("table"))?;
        Ok(Self {
            volatile: head[0] & 1 != 0,
        })
    }
}

fn bad(what: &str) -> DbError {
    DbError::Corrupt {
        what: format!("SQL information: {what}"),
    }
}

/// Bytes `n` at `at`, moving `at` on.
fn take<'a>(b: &'a [u8], at: &mut usize, n: usize, what: &str) -> Result<&'a [u8]> {
    let s = b.get(*at..*at + n).ok_or_else(|| bad(what))?;
    *at += n;
    Ok(s)
}

fn u16_at(b: &[u8], at: &mut usize, what: &str) -> Result<u16> {
    let s = take(b, at, 2, what)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn text(b: &[u8], at: &mut usize, n: usize, what: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(take(b, at, n, what)?).into_owned())
}

fn put_short(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&u16::try_from(s.len()).expect("short text").to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// `WITH CHECK OPTION`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CheckOption {
    /// None.
    #[default]
    None = 0,
    /// `CASCADED` (the default when `WITH CHECK OPTION` names neither).
    Cascaded = 1,
    /// `LOCAL`.
    Local = 2,
}

/// A view's attributes: the query it runs and its column names.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ViewDef {
    /// The check option.
    pub check: CheckOption,
    /// Read only (not updatable).
    pub read_only: bool,
    /// The query is in system naming (`LIB/FILE`).
    pub sys_naming: bool,
    /// The query text after `AS`, without the check option.
    pub query: String,
    /// The view's column names, as the statement gave them or as the
    /// query named them.
    pub columns: Vec<String>,
}

impl ViewDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![
            self.check as u8,
            u8::from(self.read_only) | u8::from(self.sys_naming) << 1,
        ];
        out.extend_from_slice(
            &u16::try_from(self.columns.len())
                .expect("columns")
                .to_le_bytes(),
        );
        out.extend_from_slice(
            &u32::try_from(self.query.len())
                .expect("query")
                .to_le_bytes(),
        );
        out.extend_from_slice(self.query.as_bytes());
        for c in &self.columns {
            put_short(&mut out, c);
        }
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut at = 0;
        let head = take(b, &mut at, 8, "view")?;
        let check = match head[0] {
            0 => CheckOption::None,
            1 => CheckOption::Cascaded,
            2 => CheckOption::Local,
            _ => return Err(bad("view check option")),
        };
        let count = u16::from_le_bytes([head[2], head[3]]);
        let qlen = u32::from_le_bytes(head[4..8].try_into().expect("4")) as usize;
        let query = text(b, &mut at, qlen, "view query")?;
        let mut columns = Vec::with_capacity(count.into());
        for _ in 0..count {
            let n = u16_at(b, &mut at, "view column")?;
            columns.push(text(b, &mut at, n.into(), "view column")?);
        }
        Ok(Self {
            check,
            read_only: head[1] & 1 != 0,
            sys_naming: head[1] & 2 != 0,
            query,
            columns,
        })
    }
}

/// One key of an index: a column or an expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexKey {
    /// An expression (else a column name).
    pub expression: bool,
    /// `DESC`.
    pub descend: bool,
    /// The column's SQL name or the expression's text.
    pub text: String,
}

/// An index's SQL attributes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct IndexDef {
    /// `UNIQUE WHERE NOT NULL`.
    pub where_not_null: bool,
    /// `ENCODED VECTOR`: kept as an ordinary index.
    pub encoded_vector: bool,
    /// The sparse condition, empty for none.
    pub where_text: String,
    /// The keys as written.
    pub keys: Vec<IndexKey>,
    /// `INCLUDE` aggregates of an encoded vector index, as written.
    pub includes: Vec<String>,
}

impl IndexDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let flags = u8::from(self.where_not_null) | u8::from(self.encoded_vector) << 1;
        let mut out = vec![flags, 0];
        out.extend_from_slice(&u16::try_from(self.keys.len()).expect("keys").to_le_bytes());
        out.extend_from_slice(
            &u16::try_from(self.includes.len())
                .expect("includes")
                .to_le_bytes(),
        );
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(
            &u32::try_from(self.where_text.len())
                .expect("where")
                .to_le_bytes(),
        );
        out.extend_from_slice(self.where_text.as_bytes());
        for k in &self.keys {
            out.push(u8::from(k.expression));
            out.push(u8::from(k.descend));
            put_short(&mut out, &k.text);
        }
        for i in &self.includes {
            put_short(&mut out, i);
        }
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut at = 0;
        let head = take(b, &mut at, 12, "index")?;
        let keys_n = u16::from_le_bytes([head[2], head[3]]);
        let inc_n = u16::from_le_bytes([head[4], head[5]]);
        let wlen = u32::from_le_bytes(head[8..12].try_into().expect("4")) as usize;
        let flags = head[0];
        let where_text = text(b, &mut at, wlen, "index condition")?;
        let mut keys = Vec::with_capacity(keys_n.into());
        for _ in 0..keys_n {
            let kd = take(b, &mut at, 2, "index key")?;
            let (expression, descend) = (kd[0] != 0, kd[1] != 0);
            let n = u16_at(b, &mut at, "index key")?;
            keys.push(IndexKey {
                expression,
                descend,
                text: text(b, &mut at, n.into(), "index key")?,
            });
        }
        let mut includes = Vec::with_capacity(inc_n.into());
        for _ in 0..inc_n {
            let n = u16_at(b, &mut at, "index include")?;
            includes.push(text(b, &mut at, n.into(), "index include")?);
        }
        Ok(Self {
            where_not_null: flags & 1 != 0,
            encoded_vector: flags & 2 != 0,
            where_text,
            keys,
            includes,
        })
    }
}

/// One object a view or index depends on, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// Its type.
    pub type_code: u16,
    /// Its library.
    pub library: Name,
    /// Its system name.
    pub name: Name,
}

impl Dependency {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; DEPENDENCY_LEN];
        out[0..2].copy_from_slice(&self.type_code.to_le_bytes());
        out[4..14].copy_from_slice(self.library.as_bytes());
        out[14..24].copy_from_slice(self.name.as_bytes());
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < DEPENDENCY_LEN {
            return Err(bad("dependency"));
        }
        Ok(Self {
            type_code: u16::from_le_bytes([b[0], b[1]]),
            library: Name::from_bytes(b[4..14].try_into().expect("10")),
            name: Name::from_bytes(b[14..24].try_into().expect("10")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_round_trip() {
        let v = ViewDef {
            check: CheckOption::Local,
            read_only: true,
            sys_naming: true,
            query: "SELECT A FROM T".into(),
            columns: vec!["A".into(), "long name".into()],
        };
        assert_eq!(ViewDef::decode(&v.encode()).unwrap(), v);
        let i = IndexDef {
            where_not_null: true,
            encoded_vector: false,
            where_text: "AMT > 2".into(),
            keys: vec![
                IndexKey {
                    expression: true,
                    descend: false,
                    text: "UPPER(NAME)".into(),
                },
                IndexKey {
                    expression: false,
                    descend: true,
                    text: "ID".into(),
                },
            ],
            includes: vec!["SUM(AMT)".into()],
        };
        assert_eq!(IndexDef::decode(&i.encode()).unwrap(), i);
        let d = Dependency {
            type_code: 0x0102,
            library: Name::parse("QGPL").unwrap(),
            name: Name::parse("T").unwrap(),
        };
        assert_eq!(Dependency::decode(&d.encode()).unwrap(), d);
        assert!(ViewDef::decode(&v.encode()[..9]).is_err());
    }
}
