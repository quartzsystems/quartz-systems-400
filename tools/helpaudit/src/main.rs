// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `helpaudit [check [--all]|gen]`: the help audit, and the command help
//! generator.

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir()?;
    let root = helpaudit::find_root(&cwd).context("not inside the QS/400 tree")?;
    match args.first().map(String::as_str) {
        None | Some("check") => {
            let limit = if args.iter().any(|a| a == "--all") { usize::MAX } else { 40 };
            let report = helpaudit::audit(&root)?;
            print!("{}", report.details(limit));
            let stale = helpaudit::generate::stale(&root)?;
            if !stale.is_empty() {
                println!("\ncommand help out of date (helpaudit gen): {}", stale.join(" "));
            }
            if report.fails() || !stale.is_empty() {
                bail!("help is missing");
            }
        }
        Some("gen-panels") => {
            for f in helpaudit::generate::panels::write(&root)? {
                println!("{f}");
            }
        }
        Some("gen") => {
            let n = helpaudit::generate::write(&root)?;
            println!("{n} command help groups written");
        }
        Some(other) => bail!("unknown subcommand {other}"),
    }
    Ok(())
}
