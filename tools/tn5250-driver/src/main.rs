// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `tn5250-driver`: a scripted 5250 display. It connects to QS/400
//! under QEMU, to the primary oracle or to pub400.com, negotiates like
//! a real emulator, runs a keystroke script, and captures or compares
//! screens. `ftp` moves files with the oracle's FTP server;
//! `hostsvr-trace` records a client's host server traffic; `ddm-trace`
//! records DDM/DRDA traffic on port 446.

use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use tn5250_driver::{
    Connect, Encoding, ModelArg, ScriptOptions, Target, ddm, env, ftp, hostsvr, session,
};

/// Which system to talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum TargetArg {
    /// The QS/400 under test (`--host`, default `127.0.0.1:2323`).
    Qs400,
    /// The primary oracle, from `QS_ORACLE_*`.
    Oracle,
    /// pub400.com, from `QS_PUB400_*`: read-only, throttled.
    Pub400,
}

impl From<TargetArg> for Target {
    fn from(t: TargetArg) -> Self {
        match t {
            TargetArg::Qs400 => Self::Qs400,
            TargetArg::Oracle => Self::Oracle,
            TargetArg::Pub400 => Self::Pub400,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum EncodingArg {
    /// DES (password levels 0 and 1).
    Des,
    /// SHA-1 (password levels 2 and 3).
    Sha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ModelOpt {
    /// `IBM-3179-2`, 24×80.
    M3179,
    /// `IBM-3477-FC`, 27×132.
    M3477,
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
    /// Run a client through a recording proxy of the host server ports.
    HostsvrTrace(HostsvrArgs),
    /// Record DDM/DRDA traffic through a proxy of port 446.
    DdmTrace(DdmArgs),
}

/// Arguments of `ddm-trace`.
#[derive(clap::Args, Debug, Clone)]
struct DdmArgs {
    /// The system the proxy forwards to.
    #[arg(long, value_enum, default_value_t = TargetArg::Pub400)]
    target: TargetArg,
    /// Where the trace goes (`OUT/<scenario>.trace`).
    #[arg(long)]
    out: PathBuf,
    /// The scenario's name.
    #[arg(long)]
    scenario: String,
    /// The DDM port.
    #[arg(long, default_value_t = 446)]
    port: u16,
    /// Ports forwarded without recording (comma separated).
    #[arg(long, value_delimiter = ',')]
    forward: Vec<u16>,
    /// Milliseconds open connections get to close.
    #[arg(long, default_value_t = 3000)]
    settle: u64,
    /// The RDB name the driver's requester connects to.
    #[arg(long, default_value = "PUB400")]
    rdb: String,
    /// The `SECMEC` to ask for first.
    #[arg(long)]
    secmec: Option<u16>,
    /// A statement for the driver's requester (repeatable).
    #[arg(long)]
    sql: Vec<String>,
    /// The package collection (`{user}` stands for the user).
    #[arg(long)]
    collection: Option<String>,
    /// Bind the package with this many dynamic sections first.
    #[arg(long, default_value_t = 0)]
    bind: u16,
    /// Only ask for this `SECMEC` and report what the server offers.
    #[arg(long)]
    offer: Option<u16>,
    /// An external client instead (after `--`).
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments of `hostsvr-trace`.
#[derive(clap::Args, Debug, Clone)]
struct HostsvrArgs {
    /// The system the proxy forwards to.
    #[arg(long, value_enum, default_value_t = TargetArg::Oracle)]
    target: TargetArg,
    /// Where the traces go (`OUT/<server>/<scenario>.trace`).
    #[arg(long)]
    out: PathBuf,
    /// The scenario's name.
    #[arg(long)]
    scenario: String,
    /// The ports to listen on (comma separated).
    #[arg(long, value_delimiter = ',')]
    ports: Vec<u16>,
    /// Milliseconds open connections get to close after the client exits.
    #[arg(long, default_value_t = 3000)]
    settle: u64,
    /// The client command (after `--`).
    #[arg(last = true)]
    command: Vec<String>,
}

/// Arguments of `run`.
#[derive(clap::Args, Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // command-line switches
struct RunArgs {
    /// The system.
    #[arg(long, value_enum)]
    target: TargetArg,
    /// Host (and `:port`) overriding the target's.
    #[arg(long)]
    host: Option<String>,
    /// The user to sign on as; the target's by default.
    #[arg(long)]
    user: Option<String>,
    /// The password; the target's by default.
    #[arg(long)]
    password: Option<String>,
    /// Send the password at negotiation (RFC 4777 auto-signon).
    #[arg(long)]
    auto_signon: bool,
    /// The substitute encoding for auto-signon.
    #[arg(long, value_enum, default_value_t = EncodingArg::Sha)]
    encoding: EncodingArg,
    /// The device name to ask for.
    #[arg(long, default_value = "")]
    device: String,
    /// The terminal model.
    #[arg(long, value_enum, default_value_t = ModelOpt::M3179)]
    model: ModelOpt,
    /// Claim GUI support (windows arrive as structured fields).
    #[arg(long)]
    gui: bool,
    /// Print the negotiation, every record and what it did.
    #[arg(long)]
    trace: bool,
    /// With `--trace`: the records in hex too.
    #[arg(long)]
    hex: bool,
    /// Where snapshots go (and are compared against).
    #[arg(long)]
    golden: Option<PathBuf>,
    /// Write snapshots instead of comparing them.
    #[arg(long)]
    update: bool,
    /// The release recorded in the golden index for this target.
    #[arg(long, default_value = "")]
    release: String,
    /// Milliseconds a `wait` gives up after.
    #[arg(long, default_value_t = 15000)]
    timeout: u64,
    /// Scripts to run, in order, over one session.
    scripts: Vec<PathBuf>,
}

/// Arguments of `ftp`.
#[derive(clap::Args, Debug, Clone)]
struct FtpArgs {
    /// The system.
    #[arg(long, value_enum)]
    target: TargetArg,
    /// Host overriding the target's.
    #[arg(long)]
    host: Option<String>,
    /// The operation: `ls PATH`, `get REMOTE LOCAL`, `put LOCAL REMOTE`,
    /// `quote COMMAND`.
    op: String,
    /// Its operands.
    operands: Vec<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run(args) => {
            if args.scripts.is_empty() {
                bail!("no script given");
            }
            let e = env::Env::load()?;
            let target: Target = args.target.into();
            let creds = e.credentials(target, args.host.as_deref())?;
            let connect = Connect {
                target,
                host: creds.host,
                port: creds.port,
                user: args.user.clone().unwrap_or(creds.user),
                password: args.password.clone().unwrap_or(creds.password),
                auto_signon: args.auto_signon,
                encoding: match args.encoding {
                    EncodingArg::Des => Encoding::Des,
                    EncodingArg::Sha => Encoding::Sha,
                },
                device: args.device.clone(),
                model: match args.model {
                    ModelOpt::M3179 => ModelArg::M3179,
                    ModelOpt::M3477 => ModelArg::M3477,
                },
                gui: args.gui,
                trace: args.trace,
                hex: args.hex,
            };
            let options = ScriptOptions {
                golden: args.golden.clone(),
                update: args.update,
                release: args.release.clone(),
                timeout_ms: args.timeout,
            };
            session::run(&connect, &options, &args.scripts)
        }
        Cmd::Ftp(args) => {
            let e = env::Env::load()?;
            let creds = e.credentials(args.target.into(), args.host.as_deref())?;
            ftp::run(&args.op, &args.operands, &creds).context("ftp")
        }
        Cmd::HostsvrTrace(args) => {
            if args.target == TargetArg::Pub400 {
                bail!("pub400 is not recorded by this mode");
            }
            let run = hostsvr::TraceRun {
                target: args.target.into(),
                out: args.out,
                scenario: args.scenario,
                ports: if args.ports.is_empty() {
                    hostsvr::DEFAULT_PORTS.to_vec()
                } else {
                    args.ports
                },
                settle: std::time::Duration::from_millis(args.settle),
                command: args.command,
            };
            hostsvr::run(&run).map(|_| ())
        }
        Cmd::DdmTrace(args) => {
            let run = ddm::TraceRun {
                target: args.target.into(),
                out: args.out,
                scenario: args.scenario,
                port: args.port,
                forward: args.forward,
                settle: std::time::Duration::from_millis(args.settle),
                rdb: args.rdb,
                mechanism: args.secmec,
                sql: args.sql,
                collection: args.collection,
                bind: args.bind,
                offer: args.offer,
                command: args.command,
            };
            ddm::run(&run).map(|_| ())
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
