// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tn5250-driver`: a scripted 5250 display. It connects to QS/400
//! under QEMU, to the primary oracle or to pub400.com, negotiates like
//! a real emulator, runs a keystroke script, and captures or compares
//! screens. `ftp` moves files with the oracle's FTP server.

mod env;
mod ftp;
mod golden;
mod mask;
mod script;
mod session;
mod trace;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

/// Which system to talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Target {
    /// The QS/400 under test (`--host`, default `127.0.0.1:2323`).
    Qs400,
    /// The primary oracle, from `QS_ORACLE_*`.
    Oracle,
    /// pub400.com, from `QS_PUB400_*`: read-only, throttled.
    Pub400,
}

impl Target {
    /// The name used in golden paths and the index.
    pub fn name(self) -> &'static str {
        match self {
            Self::Qs400 => "qs400",
            Self::Oracle => "oracle",
            Self::Pub400 => "pub400",
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "tn5250-driver", about = "QS/400 scripted TN5250E client")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run a keystroke script against a target.
    Run(RunArgs),
    /// Talk to the target's FTP server.
    Ftp(FtpArgs),
    /// Print the environment the targets resolve to (secrets hidden).
    Env,
}

/// Arguments of `run`.
#[derive(clap::Args, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // command-line switches
pub struct RunArgs {
    /// The system.
    #[arg(long, value_enum)]
    pub target: Target,
    /// Host (and `:port`) overriding the target's.
    #[arg(long)]
    pub host: Option<String>,
    /// The user to sign on as; the target's by default.
    #[arg(long)]
    pub user: Option<String>,
    /// The password; the target's by default.
    #[arg(long)]
    pub password: Option<String>,
    /// Send the password at negotiation (RFC 4777 auto-signon).
    #[arg(long)]
    pub auto_signon: bool,
    /// The substitute encoding for auto-signon.
    #[arg(long, value_enum, default_value_t = Encoding::Sha)]
    pub encoding: Encoding,
    /// The device name to ask for.
    #[arg(long, default_value = "")]
    pub device: String,
    /// The terminal model.
    #[arg(long, value_enum, default_value_t = ModelArg::M3179)]
    pub model: ModelArg,
    /// Claim GUI support (windows arrive as structured fields).
    #[arg(long)]
    pub gui: bool,
    /// Print the negotiation, every record and what it did.
    #[arg(long)]
    pub trace: bool,
    /// With `--trace`: the records in hex too.
    #[arg(long)]
    pub hex: bool,
    /// Where snapshots go (and are compared against).
    #[arg(long)]
    pub golden: Option<PathBuf>,
    /// Write snapshots instead of comparing them.
    #[arg(long)]
    pub update: bool,
    /// The release recorded in the golden index for this target.
    #[arg(long, default_value = "")]
    pub release: String,
    /// Milliseconds a `wait` gives up after.
    #[arg(long, default_value_t = 15000)]
    pub timeout: u64,
    /// Scripts to run, in order, over one session.
    pub scripts: Vec<PathBuf>,
}

/// The auto-signon substitute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Encoding {
    /// DES (password levels 0 and 1).
    Des,
    /// SHA-1 (password levels 2 and 3).
    Sha,
}

/// The models the driver negotiates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ModelArg {
    /// `IBM-3179-2`, 24×80.
    M3179,
    /// `IBM-3477-FC`, 27×132.
    M3477,
}

/// Arguments of `ftp`.
#[derive(clap::Args, Debug, Clone)]
pub struct FtpArgs {
    /// The system.
    #[arg(long, value_enum)]
    pub target: Target,
    /// Host overriding the target's.
    #[arg(long)]
    pub host: Option<String>,
    /// The operation: `ls PATH`, `get REMOTE LOCAL`, `put LOCAL REMOTE`,
    /// `quote COMMAND`.
    pub op: String,
    /// Its operands.
    pub operands: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run(args) => {
            if args.scripts.is_empty() {
                bail!("no script given");
            }
            let e = env::Env::load()?;
            let creds = e.credentials(args.target, args.host.as_deref())?;
            session::run(&args, &creds)
        }
        Cmd::Ftp(args) => {
            let e = env::Env::load()?;
            let creds = e.credentials(args.target, args.host.as_deref())?;
            ftp::run(&args, &creds).context("ftp")
        }
        Cmd::Env => {
            let e = env::Env::load()?;
            for t in [Target::Qs400, Target::Oracle, Target::Pub400] {
                match e.credentials(t, None) {
                    Ok(c) => println!("{}: {}:{} user {}", t.name(), c.host, c.port, c.user),
                    Err(err) => println!("{}: {err}", t.name()),
                }
            }
            Ok(())
        }
    }
}
