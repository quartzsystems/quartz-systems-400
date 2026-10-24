// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rpg-oracle`: compile and run corpus members on an IBM i over FTP
//! and write their normalized outcomes.
//!
//! ```text
//! rpg-oracle run MEMBER.rpgle... [--out DIR]
//! rpg-oracle refresh DIR [--out DIR] [--missing]
//! rpg-oracle clean
//! ```
//!
//! Only pub400 has an RPG compiler (`docs/PHASE9.md`), so it is the
//! target. Everything lives in the account's `B` library, compiles are
//! at least 10 seconds apart, and every object, member and spooled
//! file a run makes is deleted before it ends.

#![allow(clippy::many_single_char_names)]

mod display;
mod session;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use rpg_oracle::Case;

#[derive(Parser)]
#[command(about = "Compile and run RPG corpus members on the oracle")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run members and write their outcomes.
    Run {
        /// The members.
        files: Vec<PathBuf>,
        /// Where outcomes go (default: an `expected/` beside each member's folder).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run every member of a folder.
    Refresh {
        /// The folder of members.
        dir: PathBuf,
        /// Where outcomes go.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Only members without an outcome.
        #[arg(long)]
        missing: bool,
    },
    /// Delete what an interrupted run left.
    Clean,
    /// Read the diagnostics of every outcome in a folder again from the
    /// listing beside it.
    Rediag {
        /// The folder of outcomes.
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { files, out } => run(&files, out.as_deref(), false),
        Cmd::Refresh { dir, out, missing } => {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .with_context(|| dir.display().to_string())?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("rpgle") || e.eq_ignore_ascii_case("sqlrpgle"))
                })
                .collect();
            files.sort();
            run(&files, out.as_deref(), missing)
        }
        Cmd::Clean => {
            let mut s = session::Session::open()?;
            s.clean();
            s.close();
            Ok(())
        }
        Cmd::Rediag { dir } => rediag(&dir),
    }
}

/// Replace the `diag:` lines of each outcome with those its listing
/// gives now.
fn rediag(dir: &Path) -> Result<()> {
    let mut n = 0;
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().is_none_or(|x| x != "out") {
            continue;
        }
        let lst = p.with_extension("lst");
        let Ok(listing) = std::fs::read_to_string(&lst) else { continue };
        let lines: Vec<String> = listing.lines().map(str::to_string).collect();
        let diags = rpg_oracle::listing::diags(&lines);
        let text = std::fs::read_to_string(&p)?;
        let mut out = String::new();
        let mut placed = false;
        for l in text.lines() {
            if l.starts_with("diag: ") {
                continue;
            }
            out.push_str(l);
            out.push('\n');
            if l.starts_with("created: ") && !placed {
                placed = true;
                for d in &diags {
                    let _ = writeln!(out, "diag: {} {:02} line {} seq {}", d.msgid, d.severity, d.line, d.seq);
                }
            }
        }
        std::fs::write(&p, out)?;
        n += 1;
    }
    println!("{n} outcomes read again");
    Ok(())
}

/// Where a member's outcome goes.
fn out_dir(file: &Path, out: Option<&Path>) -> PathBuf {
    out.map_or_else(
        || {
            file.parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new("."))
                .join("expected")
        },
        Path::to_path_buf,
    )
}

fn run(files: &[PathBuf], out: Option<&Path>, missing: bool) -> Result<()> {
    let mut cases = Vec::new();
    for f in files {
        let c = Case::load(f).with_context(|| f.display().to_string())?;
        if c.name.len() > 10 {
            bail!("{}: a member name has at most 10 characters", c.name);
        }
        if let Some(line) = c.too_wide() {
            bail!("{}: line {line} is wider than the record length allows", c.name);
        }
        let dir = out_dir(f, out);
        if missing && dir.join(format!("{}.out", c.name)).exists() {
            continue;
        }
        cases.push((c, dir));
    }
    if cases.is_empty() {
        println!("nothing to run");
        return Ok(());
    }
    let mut s = session::Session::open()?;
    let mut failed = 0;
    for (c, dir) in &cases {
        match s.case(c) {
            Ok((outcome, listing)) => {
                std::fs::create_dir_all(dir)?;
                std::fs::write(dir.join(format!("{}.out", c.name)), outcome.render())?;
                std::fs::write(dir.join(format!("{}.lst", c.name)), listing.join("\n") + "\n")?;
                println!(
                    "{}: {}{}",
                    c.name,
                    if outcome.created { "created" } else { "not created" },
                    match &outcome.run {
                        rpg_oracle::outcome::Run::Escape(id) => format!(", escape {id}"),
                        rpg_oracle::outcome::Run::Ok => ", ran".into(),
                        rpg_oracle::outcome::Run::None => String::new(),
                    }
                );
            }
            Err(e) => {
                failed += 1;
                eprintln!("{}: {e:#}", c.name);
            }
        }
    }
    s.close();
    if failed > 0 {
        bail!("{failed} of {} members failed to run", cases.len());
    }
    Ok(())
}
