// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The check, execute and cancel rounds of `PRER0100` and `PWRD0200`
//! (the synchronous direct call method, in the job that ran the
//! command): every program is asked whether the state can be reached;
//! when all say yes each is told to prepare, else those already asked
//! are told to cancel. A program that cannot be called or answers
//! anything but 0 counts as yes, as IBM treats it.

use crate::call::{ExitRegistry, ProgramCaller};
use crate::formats::wm::{self, Prepare, action};
use crate::formats::ExitRequest;

/// How the rounds went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prepared {
    /// Whether every program allowed the state.
    pub allowed: bool,
    /// The program that refused, `lib/pgm`.
    pub refused_by: Option<String>,
    /// The longest wait a program asked for, seconds (30 when none
    /// asked).
    pub wait: i32,
    /// How many programs were called.
    pub programs: usize,
}

fn call(caller: &mut dyn ProgramCaller, lib: &str, pgm: &str, p: &Prepare) -> Vec<Vec<u8>> {
    let mut parms = wm::prepare_parms(p);
    if caller.call(pgm, lib, &mut parms).is_err() {
        // Treated as allowing: the check's buffers say status 1.
        return wm::prepare_parms(p);
    }
    parms
}

/// Run the rounds of `point` `format` (`PRER0100` or `PWRD0200`) with
/// the command's `delay`.
pub fn prepare(
    reg: &mut dyn ExitRegistry,
    caller: &mut dyn ProgramCaller,
    point: &str,
    format: &str,
    delay: i32,
) -> Prepared {
    let Ok(programs) = reg.programs(point, format) else {
        // The registry could not be read: nothing stops the command.
        return Prepared {
            allowed: true,
            wait: 0,
            ..Prepared::default()
        };
    };
    let mut out = Prepared {
        allowed: true,
        programs: programs.len(),
        ..Prepared::default()
    };
    let ask = |a: char, wait: i32| Prepare {
        format: format.into(),
        action: a,
        delay,
        exec_wait: wait,
    };
    let mut waits = Vec::new();
    let mut asked = 0;
    for p in &programs {
        let lib = if p.lib.is_empty() { "*LIBL" } else { &p.lib };
        let parms = call(caller, lib, &p.pgm, &ask(action::CHECK, 0));
        asked += 1;
        let (yes, wait) = wm::check_answer(&parms);
        waits.push(if wait > 0 { wait } else { 30 });
        if !yes {
            out.allowed = false;
            out.refused_by = Some(format!("{lib}/{}", p.pgm));
            break;
        }
    }
    let round = if out.allowed { action::EXECUTE } else { action::CANCEL };
    for (i, p) in programs.iter().take(asked).enumerate() {
        // The one that refused is not called again.
        if !out.allowed && i + 1 == asked {
            continue;
        }
        let lib = if p.lib.is_empty() { "*LIBL" } else { &p.lib };
        let _ = call(caller, lib, &p.pgm, &ask(round, waits[i]));
    }
    out.wait = waits.iter().copied().max().unwrap_or(0).min(3600);
    out
}

/// `PWRD0100` or `ATTN0100`: every program called with no parameters,
/// errors ignored; how many were.
pub fn bare(
    reg: &mut dyn ExitRegistry,
    caller: &mut dyn ProgramCaller,
    point: &str,
    format: &str,
) -> usize {
    crate::call::call_exit(
        reg,
        caller,
        point,
        &ExitRequest::Bare {
            format: format.into(),
        },
    )
    .called
    .len()
}
