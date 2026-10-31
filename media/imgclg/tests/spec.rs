// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The metadata file and the statuses must match `docs/MEDIA.md`.

use std::path::Path;

use imgclg::Error;
use imgclg::model::{ENTRY_FIELDS, EntryStatus, KEYS, MAX_ENTRIES, META_FILE, META_MAGIC};

fn doc() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/MEDIA.md");
    std::fs::read_to_string(p).expect("docs/MEDIA.md")
}

/// The table after `heading` whose header's first cell is `first`.
fn table_with(md: &str, heading: &str, first: &str) -> Vec<Vec<String>> {
    let rest = &md[md.find(heading).expect("heading")..];
    let at = rest
        .find(&format!("| {first} |"))
        .unwrap_or_else(|| panic!("no table starting {first}"));
    rest[at..]
        .lines()
        .take_while(|l| l.starts_with('|'))
        .skip(2)
        .map(|l| {
            l.trim_matches('|')
                .split('|')
                .map(|c| c.trim().trim_matches('`').to_string())
                .collect()
        })
        .collect()
}

#[test]
fn metadata_file_tables() {
    let md = doc();
    let keys = table_with(&md, "### Catalog metadata file", "Key");
    let keys: Vec<&str> = keys.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(keys, KEYS);
    let fields = table_with(&md, "### Catalog metadata file", "Field");
    let fields: Vec<&str> = fields.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(fields, ENTRY_FIELDS);
    assert!(md.contains(META_FILE) && md.contains(META_MAGIC));
    assert!(md.contains(&format!("holds {MAX_ENTRIES} entries")));
}

#[test]
fn status_table() {
    let md = doc();
    let rows = table_with(&md, "### States", "Status");
    let names: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
    let code = [
        EntryStatus::Mounted,
        EntryStatus::Loaded,
        EntryStatus::Available,
    ]
    .map(EntryStatus::as_str);
    assert_eq!(names, code);
}

#[test]
fn error_table() {
    let md = doc();
    let rows = table_with(&md, "### Catalog metadata file", "Error");
    let ids: Vec<&str> = rows.iter().map(|r| r[1].as_str()).collect();
    let s = String::new;
    let samples = [
        Error::NotFound(s()),
        Error::Exists(s()),
        Error::DirInUse {
            dir: s(),
            catalog: s(),
        },
        Error::NoDir(s()),
        Error::NoEntry {
            catalog: s(),
            index: 1,
        },
        Error::IndexInUse(1),
        Error::Full(s()),
        Error::NoImage(s()),
        Error::BadImage {
            path: s(),
            why: s(),
        },
        Error::Loaded(s()),
        Error::DeviceInUse(s()),
        Error::DuplicateVolume(s()),
        Error::Verify {
            catalog: s(),
            problems: 1,
        },
        Error::ReadOnlyImage(1),
        Error::Invalid(s()),
        Error::Damaged(s()),
    ];
    assert_eq!(ids.len(), samples.len());
    for (e, id) in samples.iter().zip(ids) {
        assert_eq!(e.msgid(), id, "{e}");
    }
}
