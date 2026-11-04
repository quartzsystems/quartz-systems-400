// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help everywhere: every shipped panel, display file, command,
//! parameter and message has help, in our own words, and the command
//! help groups are what the generator makes from the definitions.

use std::path::Path;

fn root() -> std::path::PathBuf {
    helpaudit::find_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("the tree")
}

#[test]
fn everything_has_help() {
    let report = helpaudit::audit(&root()).expect("the audit runs");
    assert!(!report.fails(), "\n{}", report.details(60));
    assert!(report.categories.iter().all(|c| c.total > 0));
}

#[test]
fn command_help_is_generated() {
    let stale = helpaudit::generate::stale(&root()).expect("the generator runs");
    assert!(stale.is_empty(), "run `cargo run -p helpaudit -- gen`: {stale:?}");
}
