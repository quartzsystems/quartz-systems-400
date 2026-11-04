// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `kcbuild [OUT]`: writes the Knowledge Center (default
//! `target/kc`).

use std::path::{Path, PathBuf};

use anyhow::Result;

fn main() -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let out = std::env::args()
        .nth(1)
        .map_or_else(|| repo.join("target/kc"), PathBuf::from);
    let s = kcbuild::build(&repo, &out)?;
    println!(
        "Knowledge Center: {} pages ({} commands, {} messages, {} documents) in {}",
        s.pages,
        s.commands,
        s.messages,
        s.docs,
        out.display()
    );
    Ok(())
}
