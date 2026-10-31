// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The code's layouts must match the tables in `docs/MEDIA.md`.

use std::path::Path;

use optical::iso::hybrid::{self, HybridLayout, TAIL_SECTORS};
use optical::manifest;

fn doc() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/MEDIA.md");
    std::fs::read_to_string(p).expect("docs/MEDIA.md")
}

/// The table after `heading` whose header's first cell is `first`.
fn table_with(md: &str, heading: &str, first: &str) -> Vec<Vec<String>> {
    let start = md.find(heading).expect("heading");
    let rest = &md[start..];
    let at = rest
        .find(&format!("| {first} |"))
        .unwrap_or_else(|| panic!("no table starting {first}"));
    let body = &rest[at..];
    body.lines()
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
fn hybrid_region_table() {
    let md = doc();
    let rows = table_with(&md, "### Hybrid install image", "Region");
    let total = 4000 * 2048;
    let l = HybridLayout::new(total, 3000, 100 * 2048).unwrap();
    let names: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
    let code: Vec<&str> = l.regions.iter().map(|r| r.name).collect();
    assert_eq!(names, code);
    for (row, r) in rows.iter().zip(&l.regions) {
        if let Ok(off) = row[1].parse::<u64>() {
            assert_eq!(off, r.offset, "{} offset", r.name);
        }
        if let Ok(len) = row[2].parse::<u64>() {
            assert_eq!(len, r.len, "{} length", r.name);
        }
        if let Ok(s) = row[3].parse::<u64>() {
            assert_eq!(s, r.sector(), "{} sector", r.name);
        }
        if let Ok(lba) = row[4].parse::<u64>() {
            assert_eq!(lba, r.lba(), "{} LBA", r.name);
        }
    }
    assert_eq!(l.region(hybrid::names::BACKUP_ARRAY).offset, total - 16896);
    assert!(md.contains(&format!("is {TAIL_SECTORS} sectors")));
    assert!(md.contains(&hybrid::guid_text(hybrid::QUARTZ_MEDIA)));
    assert!(md.contains(&hybrid::guid_text(hybrid::EFI_SYSTEM)));
    assert!(md.contains(hybrid::ESP_NAME) && md.contains(hybrid::MEDIA_NAME));
}

#[test]
fn manifest_table() {
    let md = doc();
    let rows = table_with(&md, "### Install media manifest", "Line");
    let keys: Vec<&str> = rows.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(keys, [manifest::MAGIC, "release", "volume", "file"]);
    assert!(md.contains(manifest::PATH));
    for f in [manifest::SLOT_A, manifest::SLOT_B, manifest::BOOT_UKI] {
        assert!(md.contains(f), "{f}");
    }
}
