// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qirc`: the IR assembler and disassembler.
//!
//! ```text
//! qirc asm IN.qir [-o OUT.qirm]      text to a binary module (verified first)
//! qirc dis IN.qirm [-o OUT.qir]      binary module to text
//! qirc check IN.qir|IN.qirm          parse or read, then verify
//! qirc fmt IN.qir [-o OUT.qir]       canonical text
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "qirc", about = "QS/400 IR assembler and disassembler", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Assemble text into a binary module.
    Asm {
        /// The `.qir` file.
        input: PathBuf,
        /// Output; the input with `.qirm` by default.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write even when the verifier objects.
        #[arg(long)]
        no_verify: bool,
    },
    /// Disassemble a binary module into text.
    Dis {
        /// The `.qirm` file.
        input: PathBuf,
        /// Output; standard output by default.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Parse or read a module and verify it.
    Check {
        /// A `.qir` or `.qirm` file.
        input: PathBuf,
    },
    /// Print a module in canonical text form.
    Fmt {
        /// A `.qir` or `.qirm` file.
        input: PathBuf,
        /// Output; standard output by default.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Load a module from text or binary by the file's extension.
fn load(path: &Path) -> Result<qir::Module> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if bytes.starts_with(qir::binary::MAGIC) {
        return qir::binary::read(&bytes).with_context(|| format!("reading {}", path.display()));
    }
    let text =
        String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", path.display()))?;
    qir::parse(&text).map_err(|e| anyhow::anyhow!("{}:{e}", path.display()))
}

fn emit(output: Option<&Path>, text: &str) -> Result<()> {
    if let Some(p) = output {
        std::fs::write(p, text).with_context(|| format!("writing {}", p.display()))
    } else {
        print!("{text}");
        Ok(())
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Asm {
            input,
            output,
            no_verify,
        } => {
            let m = load(&input)?;
            if let Err(e) = qir::verify(&m) {
                if no_verify {
                    eprintln!("{}: {e}", input.display());
                } else {
                    bail!("{}: {e}", input.display());
                }
            }
            let out = output.unwrap_or_else(|| input.with_extension("qirm"));
            std::fs::write(&out, qir::binary::write(&m))
                .with_context(|| format!("writing {}", out.display()))?;
            eprintln!(
                "{}: module {} with {} procedures written",
                out.display(),
                m.name,
                m.procs.len()
            );
        }
        Cmd::Dis { input, output } | Cmd::Fmt { input, output } => {
            let m = load(&input)?;
            emit(output.as_deref(), &qir::print(&m))?;
        }
        Cmd::Check { input } => {
            let m = load(&input)?;
            qir::verify(&m).map_err(|e| anyhow::anyhow!("{}: {e}", input.display()))?;
            println!(
                "{}: module {} verified, {} procedures",
                input.display(),
                m.name,
                m.procs.len()
            );
        }
    }
    Ok(())
}
