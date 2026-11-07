// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `.qkm` files: a whole keyboard map, as QS/400 Access for Desktop
//! writes them (`qsa-config/src/qkm.rs`). Loading one copies it.
//!
//! ```toml
//! [qsaccess]
//! kind = "keymap"
//! version = 1
//!
//! [keys]
//! Enter = "Enter"
//! "Right Ctrl" = "FieldExit"
//! ```

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::doc::{self, Extra, FileError, Header};
use super::keymap::{self, KeyMap};

/// File extension, without the dot.
pub const EXTENSION: &str = "qkm";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct KeymapDoc {
    qsaccess: Header,
    #[serde(default)]
    keys: BTreeMap<String, String>,
    #[serde(flatten)]
    extra: Extra,
}

/// A keyboard map file.
#[derive(Clone, Debug, PartialEq)]
pub struct KeymapFile {
    /// The header.
    pub header: Header,
    /// The map.
    pub map: KeyMap,
    /// Tables this release does not know.
    pub extra: Extra,
}

impl KeymapFile {
    /// A file holding `map`.
    pub fn new(map: KeyMap) -> KeymapFile {
        KeymapFile {
            header: Header::new(doc::KEYMAP),
            map,
            extra: Extra::new(),
        }
    }

    /// Parse `.qkm` text. Every function must be one this release knows.
    pub fn parse(text: &str) -> Result<KeymapFile, FileError> {
        let d: KeymapDoc = doc::parse(text, doc::KEYMAP)?;
        for (k, f) in &d.keys {
            if !keymap::is_function(f) {
                return Err(FileError::Invalid(format!(
                    "key {k} is given an unknown function \"{f}\""
                )));
            }
        }
        Ok(KeymapFile {
            header: d.qsaccess,
            map: KeyMap { keys: d.keys },
            extra: d.extra,
        })
    }

    /// The `.qkm` text.
    pub fn to_text(&self) -> String {
        let mut header = self.header.clone();
        header.kind = doc::KEYMAP.into();
        header.version = doc::VERSION;
        doc::to_text(&KeymapDoc {
            qsaccess: header,
            keys: self.map.keys.clone(),
            extra: self.extra.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_refusals() {
        let mut map = KeyMap::defaults();
        map.assign("Ctrl+Enter", "FieldExit");
        map.clear_function("Clear");
        let text = KeymapFile::new(map.clone()).to_text();
        assert!(
            text.starts_with("[qsaccess]\nkind = \"keymap\"\nversion = 1\n"),
            "{text}"
        );
        assert!(text.contains("\"Right Ctrl\" = \"FieldExit\""), "{text}");
        assert_eq!(KeymapFile::parse(&text).unwrap().map, map);
        let kept = "[qsaccess]\nkind = \"keymap\"\nversion = 1\n\n[keys]\nF1 = \"Help\"\n\n[about]\nauthor = \"ops\"\n";
        assert!(
            KeymapFile::parse(kept)
                .unwrap()
                .to_text()
                .contains("author = \"ops\"")
        );
        assert!(matches!(
            KeymapFile::parse("[qsaccess]\nkind = \"keymap\"\nversion = 1\n[keys]\nF1 = \"Fly\"\n"),
            Err(FileError::Invalid(_))
        ));
        assert!(matches!(
            KeymapFile::parse("[qsaccess]\nkind = \"keymap\"\nversion = 4\n"),
            Err(FileError::Newer(4))
        ));
    }

    /// The desktop's own test file reads.
    #[test]
    fn the_desktops_file() {
        let f = KeymapFile::parse(include_str!("testdata/operations.qkm")).unwrap();
        assert!(!f.map.keys.is_empty());
    }
}
