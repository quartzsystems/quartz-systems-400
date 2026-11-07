// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a compiler command reports, for its reply: each compiler puts
//! its listing's messages here as `compdiag` values while it runs
//! ([`put`]), with their second-level texts from the message files
//! ([`help`]); [`super::run`] moves them into the reply, and the job
//! keeps them as its last compile's.
//!
//! BLOCKED(rel): `OPTION(*EVENTF)` is accepted by every compiler command
//! and writes no `EVFEVENT` file; QAP's `Develop.Compile` carries these
//! diagnostics instead (QAP 8/9 handoff, Part C 4).

use compdiag::{Diagnostic, MemberName, Where};
use stg::proto::CompileDiag;

use crate::ipl::system::System;

/// The second-level text of message `id` with `values`, from the file
/// its prefix names on the library list; empty when no file has it.
pub fn help(system: &mut System) -> impl FnMut(&str, &[String]) -> String + '_ {
    move |id: &str, values: &[String]| {
        if id.is_empty() {
            return String::new();
        }
        let file = sysapi::param::file_of(id);
        crate::stgsvc::msgs::description(system, id, &(file.to_string(), "*LIBL".to_string()))
            .map(|d| d.format_help(&d.data_of(values)))
            .unwrap_or_default()
    }
}

/// A compiler's diagnostics, for the reply of the command running.
pub fn put(system: &mut System, diags: Vec<Diagnostic>) {
    system.compiled.extend(diags);
}

/// The diagnostics put since the command started, on the wire.
pub fn take(system: &mut System) -> Vec<CompileDiag> {
    std::mem::take(&mut system.compiled)
        .into_iter()
        .map(CompileDiag::from)
        .collect()
}

/// `diags` on the wire, for a reply made apart from [`super::run`] (a
/// command that took turns).
pub fn wire(diags: Vec<Diagnostic>) -> Vec<CompileDiag> {
    diags.into_iter().map(CompileDiag::from).collect()
}

/// A source member, as a diagnostic points into it.
pub fn member(library: &str, file: &str, member: &str) -> Where {
    Where::Member(MemberName {
        library: library.to_string(),
        file: file.to_string(),
        member: member.to_string(),
    })
}
