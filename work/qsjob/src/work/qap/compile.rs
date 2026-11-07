// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QAP's `Develop.Compile` (version 8, operation 15) in the connection
//! job's lane: the command run as `RunCommand` runs it
//! (`super::commands::run`), then the diagnostics the compiler left on
//! the job for it (`rpg/compdiag`), as `qapd::Compiled`.

use qap_proto::Status;
use qapd::{Compiled, Refusal};
use stg::Shared;

use crate::console::Console;

/// Run the compile `command` in the job's runtime and take its
/// diagnostics. `cancelled` as for `RunCommand`.
///
/// BLOCKED(rel): the compilers do not leave their diagnostics on the job
/// yet (QAP handoff Part C); until they do, `Compile` is refused with
/// `TCP1A60` rather than run without them.
pub fn run(
    link: &Shared,
    _console: &mut Console,
    _command: &str,
    _cancelled: &dyn Fn() -> bool,
) -> Result<Compiled, Refusal> {
    let message = super::host::message(
        &mut link.borrow_mut(),
        qapd::develop::NOT_AVAILABLE,
        &["Compile"],
    );
    Err(Refusal {
        status: Status::Error,
        message,
    })
}
