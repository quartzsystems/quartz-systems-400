// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `textaudit [check [--all]|list [CRATE]|baseline]`: the text audit.

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir()?;
    let root = textaudit::find_root(&cwd).context("not inside the QS/400 tree")?;
    match args.first().map(String::as_str) {
        None | Some("check") => {
            let limit = if args.iter().any(|a| a == "--all") { usize::MAX } else { 40 };
            let report = textaudit::audit(&root)?;
            print!("{}", report.details(limit));
            if !report.stale.is_empty() {
                println!("\n{} baseline entries moved (textaudit baseline)", report.stale.len());
            }
            if report.fails() {
                bail!("new text literals on display paths");
            }
        }
        Some("list") => {
            let report = textaudit::audit(&root)?;
            for (c, v) in &report.sites {
                if args.get(1).is_some_and(|w| w != c) {
                    continue;
                }
                for s in v {
                    println!("{}:{}\t{}\t{:?}", s.file, s.line, s.kind.name(), s.text);
                }
            }
        }
        Some("baseline") => {
            let report = textaudit::audit(&root)?;
            std::fs::write(root.join(textaudit::BASELINE), report.baseline_text())?;
            println!("{} sites in the baseline", report.total());
        }
        Some(other) => bail!("unknown subcommand {other}"),
    }
    Ok(())
}
