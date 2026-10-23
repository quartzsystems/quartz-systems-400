// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `genmenus [--root DIR] [--out DIR]`: write the command-group menus
//! as UIM sources (`ui/menus/pnlgrp/` by default) and print the
//! inventory reconciliation.

use std::path::PathBuf;

use anyhow::{Context, bail};
use menus::generate::{Inputs, generate, write};

fn main() -> anyhow::Result<()> {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => root = PathBuf::from(args.next().context("--root needs a directory")?),
            "--out" => {
                out = Some(PathBuf::from(
                    args.next().context("--out needs a directory")?,
                ));
            }
            other => bail!("unknown argument {other}; use --root DIR or --out DIR"),
        }
    }
    let out = out.unwrap_or_else(|| root.join("ui/menus/pnlgrp"));
    let inputs = Inputs::read(&root).with_context(|| format!("inputs under {}", root.display()))?;
    let generated = generate(&inputs);
    write(&generated, &out).with_context(|| format!("writing {}", out.display()))?;
    print!("{}", generated.report);
    println!("written to {}", out.display());
    Ok(())
}
