// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What session (`.qsd`), keyboard map (`.qkm`) and macro (`.qsm`)
//! files have in common, as QS/400 Access for Desktop writes them
//! (`qsa-config/src/doc.rs`): TOML whose first table is the header
//!
//! ```toml
//! [qsaccess]
//! kind = "display"   # or "keymap", "macro"
//! version = 1
//! ```
//!
//! A file with a higher `version` is refused (`QAD2002`) before anything
//! else in it is looked at; keys this release does not know are kept in
//! each table's `extra` and written back.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Keys a table holds that this release does not know.
pub type Extra = toml::Table;

/// `kind` of a display session file.
pub const DISPLAY: &str = "display";
/// `kind` of a keyboard map file.
pub const KEYMAP: &str = "keymap";
/// `kind` of a macro file.
pub const MACRO: &str = "macro";
/// The format version this release reads and writes.
pub const VERSION: u32 = 1;

/// The `[qsaccess]` table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// What the file holds.
    pub kind: String,
    /// The format's major version.
    pub version: u32,
    /// Keys this release does not know.
    #[serde(flatten)]
    pub extra: Extra,
}

impl Header {
    /// The header this release writes for `kind`.
    pub fn new(kind: &str) -> Header {
        Header {
            kind: kind.to_owned(),
            version: VERSION,
            extra: Extra::new(),
        }
    }
}

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum FileError {
    /// Not TOML.
    #[error("{0}")]
    Parse(String),
    /// No header.
    #[error("the file has no [qsaccess] table with a kind and version")]
    NoHeader,
    /// Another kind of file.
    #[error("the file holds \"{found}\", not {expected}")]
    WrongKind {
        /// The kind found.
        found: String,
        /// What was wanted.
        expected: &'static str,
    },
    /// A newer format.
    #[error("the file is format version {0}")]
    Newer(i64),
    /// A value this release refuses.
    #[error("{0}")]
    Invalid(String),
}

impl FileError {
    /// The message for file `name`: `QAD2002` for a newer format,
    /// `QAD2001` otherwise, with its substitution data.
    pub fn message(&self, name: &str) -> (&'static str, Vec<String>) {
        match self {
            FileError::Newer(v) => ("QAD2002", vec![name.to_string(), v.to_string()]),
            other => {
                let why = other.to_string();
                (
                    "QAD2001",
                    vec![
                        name.to_string(),
                        why.trim().trim_end_matches('.').to_string(),
                    ],
                )
            }
        }
    }
}

fn kind_name(kind: &str) -> &'static str {
    match kind {
        DISPLAY => "a display session",
        KEYMAP => "a keyboard map",
        MACRO => "a macro",
        _ => "this kind of file",
    }
}

/// Check the header of `text` only: kind and version.
pub fn check_header(text: &str, kind: &str) -> Result<(), FileError> {
    let table: toml::Table =
        toml::from_str(text).map_err(|e| FileError::Parse(e.message().trim().to_owned()))?;
    let header = table
        .get("qsaccess")
        .and_then(toml::Value::as_table)
        .ok_or(FileError::NoHeader)?;
    let found = header
        .get("kind")
        .and_then(toml::Value::as_str)
        .ok_or(FileError::NoHeader)?;
    let version = header
        .get("version")
        .and_then(toml::Value::as_integer)
        .ok_or(FileError::NoHeader)?;
    if found != kind {
        return Err(FileError::WrongKind {
            found: found.to_owned(),
            expected: kind_name(kind),
        });
    }
    if version > i64::from(VERSION) {
        return Err(FileError::Newer(version));
    }
    if version < 1 {
        return Err(FileError::Invalid(format!(
            "format version {version} is not valid"
        )));
    }
    Ok(())
}

/// Parse a file of `kind`: the header first, then the whole as `T`. A
/// UTF-8 byte-order mark, as some editors write, is dropped.
pub fn parse<T: DeserializeOwned>(text: &str, kind: &str) -> Result<T, FileError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    check_header(text, kind)?;
    toml::from_str(text).map_err(|e| FileError::Parse(e.message().trim().to_owned()))
}

/// The TOML text of `value`.
pub fn to_text<T: Serialize>(value: &T) -> String {
    toml::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_checks() {
        let ok = "[qsaccess]\nkind = \"display\"\nversion = 1\n";
        assert!(check_header(ok, DISPLAY).is_ok());
        assert!(
            matches!(check_header(ok, MACRO), Err(FileError::WrongKind { found, .. }) if found == "display")
        );
        assert!(matches!(
            check_header("[qsaccess]\nkind = \"display\"\nversion = 2\n", DISPLAY),
            Err(FileError::Newer(2))
        ));
        assert!(matches!(
            check_header("[system]\nname = \"a\"\n", DISPLAY),
            Err(FileError::NoHeader)
        ));
        assert!(matches!(
            check_header("[qsaccess\n", DISPLAY),
            Err(FileError::Parse(_))
        ));
        assert_eq!(FileError::Newer(2).message("a.qsd").0, "QAD2002");
        let (id, data) = FileError::Invalid("CCSID 9 is not supported.".into()).message("a.qsd");
        assert_eq!(
            (id, data[1].as_str()),
            ("QAD2001", "CCSID 9 is not supported")
        );
    }
}
