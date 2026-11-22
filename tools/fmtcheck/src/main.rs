// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `fmtcheck [check|update [--force]|savf FILE...]`: the format freeze
//! check; `savf` checks save streams against the frozen save format.

use anyhow::{Context, Result, bail};
use fmtcheck::{Row, State};

fn print(rows: &[Row]) {
    for r in rows {
        let state = match &r.state {
            State::Same => "same".to_string(),
            State::New => "new (not in the lock)".to_string(),
            State::ChangedWithoutBump => "CHANGED WITHOUT A VERSION BUMP".to_string(),
            State::Bumped { from } => {
                format!("version {from} -> {}: update the lock", r.now.version)
            }
        };
        let draft = if r.draft { " (draft)" } else { "" };
        let doc = if r.doc_unmarked {
            "; its document lacks the frozen section"
        } else {
            ""
        };
        println!("{:<22} v{:<10} {state}{draft}{doc}", r.name, r.now.version);
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir()?;
    let root = fmtcheck::find_root(&cwd).context("not inside the QS/400 tree")?;
    match args.first().map(String::as_str) {
        None | Some("check") => {
            let rows = fmtcheck::check(&root)?;
            print(&rows);
            let bad = rows.iter().filter(|r| r.fails()).count();
            if bad > 0 {
                bail!("{bad} frozen format(s) fail the check");
            }
        }
        Some("update") => {
            let force = args.iter().any(|a| a == "--force");
            let rows = fmtcheck::update(&root, force)?;
            print(&rows);
        }
        Some("savf") => {
            let files = &args[1..];
            if files.is_empty() {
                bail!("savf: name the save streams to check");
            }
            let mut bad = 0;
            for f in files {
                let bytes = std::fs::read(f).with_context(|| format!("cannot read {f}"))?;
                match fmtcheck::savf::check(&bytes) {
                    Ok(s) => println!(
                        "{f}: version {}, kind {}, {} objects, {} records: frozen format",
                        s.version, s.kind, s.objects, s.records
                    ),
                    Err(v) => {
                        println!("{f}: {v}");
                        bad += 1;
                    }
                }
            }
            if bad > 0 {
                bail!("{bad} save stream(s) do not match the frozen save format");
            }
        }
        Some(o) => bail!("unknown command {o}; use check, update or savf"),
    }
    Ok(())
}
