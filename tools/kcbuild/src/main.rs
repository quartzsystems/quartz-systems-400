// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `kcbuild [OUT]`: writes the Knowledge Center (default `target/kc`)
//! after checking the release notes. `kcbuild notes --update` rewrites
//! the notes' generated blocks; `kcbuild notes` checks them.
//! `QS400_MARKERS=legacy` allows the marker forms of before the rename.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use kcbuild::notes;

fn main() -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let legacy = notes::legacy_from_env();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["notes", "--update"] => {
            let n = notes::update(&repo)?;
            println!("{}: blocks rewritten, {n} markers", notes::NOTES);
        }
        ["notes"] => {
            let p = notes::problems(&repo, legacy)?;
            if !p.is_empty() {
                bail!("{}", p.join("\n"));
            }
            println!("{}: current", notes::NOTES);
        }
        rest => {
            let out = rest.first().map_or_else(|| repo.join("target/kc"), PathBuf::from);
            let s = kcbuild::build_checked(&repo, &out, legacy)?;
            println!(
                "Knowledge Center: {} pages ({} commands, {} messages, {} documents) in {}",
                s.pages,
                s.commands,
                s.messages,
                s.docs,
                out.display()
            );
        }
    }
    Ok(())
}
