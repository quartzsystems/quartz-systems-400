// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Develop.Compile` (QAP version 8) in the connection job: the command
//! runs as `RunCommand` runs it, in the lane with the connection's
//! library list, and the diagnostics are what the compiler left on the
//! job as its last compile's (`mi::Job::last_compile`). A command that
//! is no compiler leaves none.

use compdiag::Diagnostic;
use qapd::{Compiled, Ran};

use crate::console::Console;

/// Run compiler command `command`: what `RunCommand` gives (the job,
/// its messages, the status: `Ok`, or `Error` when the compile failed)
/// and the compile's diagnostics, in listing order. `cancelled` says
/// whether the client cancelled this request.
pub fn compile(
    console: &mut Console,
    command: &str,
    cancelled: &dyn Fn() -> bool,
) -> (Ran, Vec<Diagnostic>) {
    // Nothing from an earlier compile answers for this one.
    console.job().set_last_compile(Vec::new());
    let ran = super::commands::run(console, command, cancelled);
    let diags = console.job().last_compile().unwrap_or_default().to_vec();
    (ran, diags)
}

/// `Compile` for the lane: the run and its diagnostics in the
/// protocol's form. A failed compile is `Error` with the body still
/// filled; the escape message is the reply's message.
pub fn run(console: &mut Console, command: &str, cancelled: &dyn Fn() -> bool) -> Compiled {
    let (ran, diags) = compile(console, command, cancelled);
    Compiled {
        ran,
        diagnostics: diags.into_iter().map(Into::into).collect(),
    }
}
