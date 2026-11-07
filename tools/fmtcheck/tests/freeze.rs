// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The tree's frozen formats match the lock, and the check catches what
//! it must.

use std::fs;
use std::path::{Path, PathBuf};

use fmtcheck::State;

fn root() -> PathBuf {
    fmtcheck::find_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("repository root")
}

#[test]
fn the_tree_matches_the_lock() {
    let rows = fmtcheck::check(&root()).expect("check");
    let bad: Vec<String> = rows
        .iter()
        .filter(|r| r.fails())
        .map(|r| format!("{} {:?} doc_unmarked={}", r.name, r.state, r.doc_unmarked))
        .collect();
    assert!(
        bad.is_empty(),
        "frozen formats changed; bump the version and run `cargo run -p fmtcheck -- update`:\n{}",
        bad.join("\n")
    );
}

#[test]
fn docs_and_formatting_do_not_count() {
    let a = fmtcheck::canonical("/// one\npub struct A { x: u8 }").unwrap();
    let b = fmtcheck::canonical("/// two\n// note\npub   struct A {\n    x: u8,\n}").unwrap();
    // A trailing comma is a token; everything else is the same.
    assert_eq!(a.replace(" ,", ""), b.replace(" ,", ""));
    let c = fmtcheck::canonical("pub struct A { x: u16 }").unwrap();
    assert_ne!(a, c);
}

/// A scratch tree with one format.
fn scratch(name: &str, src: &str, version: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fmtcheck-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("tools/fmtcheck")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join(fmtcheck::MANIFEST),
        "[[format]]\nname = \"x\"\nfrozen_at = \"V1R1M0\"\ndoc = \"X.md\"\n\
         version = [\"src/x.rs::VERSION\"]\nfiles = [\"src/x.rs\"]\n",
    )
    .unwrap();
    fs::write(dir.join("X.md"), "# X\n\n## Frozen at V1R1M0\n").unwrap();
    fs::write(
        dir.join("src/x.rs"),
        format!("pub const VERSION: u16 = {version};\n{src}"),
    )
    .unwrap();
    dir
}

#[test]
fn a_change_needs_a_bump() {
    let dir = scratch(
        "bump",
        "pub struct H { a: u32 }\n#[cfg(test)] mod tests {}",
        1,
    );
    assert_eq!(fmtcheck::check(&dir).unwrap()[0].state, State::New);
    fmtcheck::update(&dir, false).unwrap();
    assert_eq!(fmtcheck::check(&dir).unwrap()[0].state, State::Same);

    // Tests and comments may change freely.
    fs::write(
        dir.join("src/x.rs"),
        "pub const VERSION: u16 = 1;\n// hi\npub struct H { a: u32 }\n#[cfg(test)] mod tests { fn t() {} }",
    )
    .unwrap();
    assert_eq!(fmtcheck::check(&dir).unwrap()[0].state, State::Same);

    // A layout change without a bump fails, and update refuses it.
    fs::write(
        dir.join("src/x.rs"),
        "pub const VERSION: u16 = 1;\npub struct H { a: u64 }",
    )
    .unwrap();
    let rows = fmtcheck::check(&dir).unwrap();
    assert_eq!(rows[0].state, State::ChangedWithoutBump);
    assert!(rows[0].fails());
    fmtcheck::update(&dir, false).unwrap();
    assert_eq!(
        fmtcheck::check(&dir).unwrap()[0].state,
        State::ChangedWithoutBump
    );

    // With the bump it is reported until the lock follows.
    fs::write(
        dir.join("src/x.rs"),
        "pub const VERSION: u16 = 2;\npub struct H { a: u64 }",
    )
    .unwrap();
    assert_eq!(
        fmtcheck::check(&dir).unwrap()[0].state,
        State::Bumped { from: "1".into() }
    );
    fmtcheck::update(&dir, false).unwrap();
    assert_eq!(fmtcheck::check(&dir).unwrap()[0].state, State::Same);

    // The document must say it is frozen.
    fs::write(dir.join("X.md"), "# X\n").unwrap();
    assert!(fmtcheck::check(&dir).unwrap()[0].doc_unmarked);
    let _ = fs::remove_dir_all(&dir);
}
