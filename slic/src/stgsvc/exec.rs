// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Console commands run in the service on a job's behalf: the object,
//! storage, database and ILE build commands. `slic` keeps its own; the
//! job keeps calls and the debugger.

use crate::ipl::system::System;
use crate::procs::svcjob::LOCAL;
use crate::svc;

/// Run one line; its output.
pub fn run(system: &mut System, line: &str) -> String {
    if let Some(reply) = svc::db::intercept(system, line) {
        return reply;
    }
    let word = line.split_whitespace().next().unwrap_or("");
    if LOCAL.contains(&word) {
        return format!("{word} is answered by the console, not the storage service\n");
    }
    svc::reply(system, line)
}
