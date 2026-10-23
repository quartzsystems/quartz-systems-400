// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kit's files on disk: DDS members, CSV data, the RLA script and
//! the queries, and the little DDS reading the RPG generator needs.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Physical files in creation and load order.
pub const PHYSICAL: &[&str] = &[
    "CUSTOMER",
    "ORDERS",
    "ORDLINE",
    "PRODUCT",
    "INVENTORY",
    "EVENTS",
    "TYPES",
    "NOTES",
    "LOGHIST",
    "RLALOG",
];

/// Logical files in creation order (after every physical file).
pub const LOGICAL: &[&str] = &[
    "CUSBYNAME",
    "ORDBYCUS",
    "PRDBYDESC",
    "CUSORD",
    "ORDALL",
    "ORDJOIN",
    "ACTIVE",
    "BIGORD",
];

/// Physical files whose data is exported and compared: every one but
/// `RLALOG`, which is the oracle's own log (`rla.csv`).
pub fn exported_physical() -> impl Iterator<Item = &'static str> {
    PHYSICAL.iter().copied().filter(|n| *n != "RLALOG")
}

/// Logical files with one format, exported like a physical file (in
/// arrival sequence, as the machine's `CPYTOIMPF` does).
pub const EXPORTED_LOGICAL: &[&str] = &[
    "CUSBYNAME",
    "ORDBYCUS",
    "PRDBYDESC",
    "ORDJOIN",
    "ACTIVE",
    "BIGORD",
];

/// The source file every DDS member goes into.
pub const SRCFILE: &str = "QDDSSRC";
/// The library.
pub const LIBRARY: &str = "QSTEST";
/// The journal and its first receiver.
pub const JOURNAL: &str = "QSTJRN";
/// The first receiver.
pub const RECEIVER: &str = "QSTRCV0001";

/// Whether a physical file is created with `REUSEDLT(*YES)`.
#[must_use]
pub fn reuse_deleted(name: &str) -> bool {
    name == "LOGHIST"
}

/// The kit directory.
#[derive(Clone, Debug)]
pub struct Kit {
    /// `conformance/db`.
    pub root: PathBuf,
}

impl Kit {
    /// The kit checked in beside this crate.
    #[must_use]
    pub fn locate() -> Self {
        Self {
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("db"),
        }
    }

    /// A DDS member; `kind` is `PF` or `LF`.
    pub fn dds(&self, name: &str, kind: &str) -> io::Result<String> {
        fs::read_to_string(self.root.join("dds").join(format!("{name}.{kind}")))
    }

    /// Every DDS member as (name, kind, text), physical files first.
    pub fn dds_members(&self) -> io::Result<Vec<(String, String, String)>> {
        let mut out = Vec::new();
        for (names, kind) in [(PHYSICAL, "PF"), (LOGICAL, "LF")] {
            for n in names {
                out.push(((*n).to_string(), kind.to_string(), self.dds(n, kind)?));
            }
        }
        Ok(out)
    }

    /// The CSV of a physical file, when the kit has one.
    pub fn csv(&self, name: &str) -> Option<io::Result<Vec<u8>>> {
        let p = self.root.join("data").join(format!("{name}.csv"));
        p.is_file().then(|| fs::read(p))
    }

    /// The RLA script, line by line.
    pub fn rla_lines(&self) -> io::Result<Vec<String>> {
        Ok(
            fs::read_to_string(self.root.join("rla").join("qstest.rla"))?
                .lines()
                .map(str::to_string)
                .collect(),
        )
    }

    /// The fixed queries, comment lines dropped, one statement each
    /// without its `;`.
    pub fn queries(&self) -> io::Result<Vec<String>> {
        let text = fs::read_to_string(self.root.join("sql").join("queries.sql"))?;
        let body: String = text
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(body
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect())
    }

    /// Where the machine's outputs go.
    #[must_use]
    pub fn expected_dir(&self) -> PathBuf {
        self.root.join("expected")
    }

    /// Where the oracle programs live.
    #[must_use]
    pub fn oracle_dir(&self) -> PathBuf {
        self.root.join("oracle")
    }

    /// Where the in-memory run writes its outputs.
    #[must_use]
    pub fn ours_dir() -> PathBuf {
        crate::target_dir().join("conformance-db").join("ours")
    }
}

/// A field as the RPG generator needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdsField {
    /// Name.
    pub name: String,
    /// `ALWNULL`.
    pub allow_null: bool,
    /// Character field (`A`, or a `JREF`/`PFILE` field whose type the
    /// physical file gives: resolved by the caller).
    pub char: Option<bool>,
}

/// The first record format name and the fields of a DDS member, from
/// the fixed columns.
#[must_use]
pub fn dds_fields(text: &str) -> (Option<String>, Vec<DdsField>) {
    let mut format = None;
    let mut fields: Vec<DdsField> = Vec::new();
    for line in text.lines() {
        let chars: Vec<char> = line.chars().collect();
        let col = |i: usize| chars.get(i - 1).copied().unwrap_or(' ');
        let span = |a: usize, b: usize| -> String {
            (a..=b).map(col).collect::<String>().trim().to_string()
        };
        if col(7) == '*' {
            continue;
        }
        let name_type = col(17);
        let name = span(19, 28);
        let keywords = span(45, 80).to_ascii_uppercase();
        match name_type {
            'R' => {
                if format.is_none() {
                    format = Some(name);
                }
            }
            ' ' if !name.is_empty() => {
                let ty = col(35);
                fields.push(DdsField {
                    name,
                    allow_null: keywords.contains("ALWNULL"),
                    char: match ty {
                        ' ' => None,
                        'A' => Some(true),
                        _ => Some(false),
                    },
                });
            }
            ' ' => {
                if keywords.contains("ALWNULL")
                    && let Some(f) = fields.last_mut()
                {
                    f.allow_null = true;
                }
            }
            _ => {}
        }
    }
    (format, fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kit_files_are_present() {
        let kit = Kit::locate();
        let members = kit.dds_members().unwrap();
        assert_eq!(members.len(), PHYSICAL.len() + LOGICAL.len());
        assert!(kit.rla_lines().unwrap().len() > 20);
        assert!(kit.queries().unwrap().len() >= 10);
        for n in PHYSICAL {
            if *n != "RLALOG" {
                assert!(kit.csv(n).is_some(), "{n}.csv");
            }
        }
    }

    #[test]
    fn dds_fields_follow_continuations() {
        let kit = Kit::locate();
        let (fmt, fields) = dds_fields(&kit.dds("CUSTOMER", "PF").unwrap());
        assert_eq!(fmt.as_deref(), Some("CUSREC"));
        let since = fields.iter().find(|f| f.name == "SINCE").unwrap();
        assert!(since.allow_null);
        let note = fields.iter().find(|f| f.name == "NOTE").unwrap();
        assert!(note.allow_null);
        assert_eq!(note.char, Some(true));
        assert!(!fields.iter().find(|f| f.name == "BAL").unwrap().allow_null);
    }
}
