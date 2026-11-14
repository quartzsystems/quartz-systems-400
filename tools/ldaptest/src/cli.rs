// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command line: what `main` runs, callable from the tests.

use std::io::Write;

use crate::scenarios::{self, Ctx};
use crate::target::{Command, USAGE};
use crate::{Target, bench, ldif, oracle};

/// The exit code of a usage error.
pub const USAGE_EXIT: i32 = 101;

/// Runs the command line `args` (after the program name) with `env` for the
/// passwords and the oracle's host, printing to `out`: the exit code.
pub fn run(args: &[String], env: &dyn Fn(&str) -> Option<String>, out: &mut dyn Write) -> i32 {
    let t = match Target::from_args(args, env) {
        Ok(t) => t,
        Err(e) => {
            let _ = writeln!(out, "{e}\n{USAGE}");
            return USAGE_EXIT;
        }
    };
    match t.command {
        Command::List => {
            for n in scenarios::names() {
                let _ = writeln!(out, "{n}");
            }
            return 0;
        }
        Command::LdifGen => {
            let (n, parent) = t.generate.clone().unwrap_or_default();
            let _ = out.write_all(ldif::write(&ldif::generate(&parent, n)).as_bytes());
            return 0;
        }
        _ => {}
    }
    let c = match Ctx::new(t) {
        Ok(c) => c,
        Err(e) => {
            let _ = writeln!(out, "fail rootdse: {e}");
            return 1;
        }
    };
    match c.t.command {
        Command::Bench => match bench::run(&c) {
            Ok(lines) => {
                for l in lines {
                    let _ = writeln!(out, "{l}");
                }
                0
            }
            Err(e) => {
                let _ = writeln!(out, "fail bench: {e}");
                1
            }
        },
        Command::Oracle => match oracle::run(&c) {
            Ok((rep, path)) => {
                let fails = rep.failures().len();
                let _ = writeln!(
                    out,
                    "{} scenarios recorded in {} ({fails} failed)",
                    rep.rows.len(),
                    path.display()
                );
                i32::try_from(fails.min(100)).unwrap_or(100)
            }
            Err(e) => {
                let _ = writeln!(out, "fail oracle: {e}");
                1
            }
        },
        _ => {
            let rep = scenarios::run(&c);
            let _ = write!(out, "{rep}");
            i32::try_from(rep.failures().len().min(100)).unwrap_or(100)
        }
    }
}
