// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command line: what `main` runs, callable from the tests.

use std::io::Write;

use crate::ctx::Ctx;
use crate::target::{Command, USAGE};
use crate::{Target, scenarios};

/// The exit code of a usage error.
pub const USAGE_EXIT: i32 = 101;

/// Runs the command line `args` (after the program name), printing to `out`:
/// the exit code.
pub fn run(args: &[String], out: &mut dyn Write) -> i32 {
    let t = match Target::from_args(args) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(out, "{e}\n{USAGE}");
            return USAGE_EXIT;
        }
    };
    if t.command == Command::List {
        for n in scenarios::names() {
            let _ = writeln!(out, "{n}");
        }
        return 0;
    }
    let c = match Ctx::resolve(&t) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(out, "{e}");
            return USAGE_EXIT;
        }
    };
    match t.command {
        Command::Probe => {
            let _ = write!(out, "{}", scenarios::probe::run(&c));
            0
        }
        Command::Bench { mib, bs } => match crate::bench::run(&c, mib, bs) {
            Ok(b) => {
                let _ = write!(out, "{b}");
                0
            }
            Err(e) => {
                let _ = writeln!(out, "fail bench: {e}");
                1
            }
        },
        Command::HandleSave(ref f) | Command::HandleUse(ref f) => {
            let r = if matches!(t.command, Command::HandleSave(_)) {
                crate::handle::save(&c, f)
            } else {
                crate::handle::use_saved(&c, f)
            };
            match r {
                Ok(s) => {
                    let _ = write!(out, "{s}");
                    0
                }
                Err(e) => {
                    let _ = writeln!(out, "fail handle: {e}");
                    1
                }
            }
        }
        _ => {
            let r = scenarios::run_all(&c);
            let _ = write!(out, "{r}");
            i32::try_from(r.failures().len().min(100)).unwrap_or(100)
        }
    }
}
