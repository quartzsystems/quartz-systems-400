// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `COMMENT` entry: the long comment `COMMENT ON` gives an object or
//! one of its columns. See `docs/DB_FORMAT.md` §8.1.

use qsname::Name;

use super::{SqlInfo, tag};
use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// `SqlComment` layout.
pub static COMMENT_LAYOUT: Layout = Layout {
    name: "SqlComment",
    size: None,
    fields: &[
        field("column", 0, 10),
        field("reserved", 10, 2),
        Field {
            name: "text",
            offset: Some(12),
            size: None,
        },
    ],
};

/// Bytes before the text.
const HEAD: usize = 12;

/// Blanks in CP037: the object's own comment.
const BLANKS: [u8; 10] = [0x40; 10];

/// A long comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    /// The column's system name; `None` for the object's own.
    pub column: Option<Name>,
    /// The comment.
    pub text: String,
}

impl Comment {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEAD + self.text.len());
        out.extend_from_slice(self.column.as_ref().map_or(&BLANKS, |c| c.as_bytes()));
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(self.text.as_bytes());
        out
    }

    /// Decode an entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let head: &[u8; 10] =
            b.get(..10)
                .and_then(|h| h.try_into().ok())
                .ok_or_else(|| DbError::Corrupt {
                    what: "SQL information: comment".into(),
                })?;
        let column = (*head != BLANKS).then(|| Name::from_bytes(*head));
        let text = String::from_utf8_lossy(b.get(HEAD..).unwrap_or_default()).into_owned();
        Ok(Self { column, text })
    }
}

/// The comment of `column` (`None`: of the object) in `info`.
#[must_use]
pub fn get(info: &SqlInfo, column: Option<Name>) -> Option<String> {
    info.all(tag::COMMENT)
        .filter_map(|b| Comment::decode(b).ok())
        .find(|c| c.column == column)
        .map(|c| c.text)
}

/// Set or, with `text` `None`, drop the comment of `column`.
pub fn set(info: &mut SqlInfo, column: Option<Name>, text: Option<String>) {
    info.entries.retain(|e| {
        e.tag != tag::COMMENT || Comment::decode(&e.body).is_ok_and(|c| c.column != column)
    });
    if let Some(text) = text {
        info.push(tag::COMMENT, Comment { column, text }.encode());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlinf::Kind;

    #[test]
    fn comments_round_trip_per_column() {
        let mut info = SqlInfo::new(Kind::Table);
        let c = Name::parse("CODE").expect("valid");
        set(&mut info, None, Some("the table".into()));
        set(&mut info, Some(c), Some("a column".into()));
        assert_eq!(get(&info, None).as_deref(), Some("the table"));
        assert_eq!(get(&info, Some(c)).as_deref(), Some("a column"));
        set(&mut info, None, None);
        assert_eq!(get(&info, None), None);
        assert_eq!(get(&info, Some(c)).as_deref(), Some("a column"));
    }
}
