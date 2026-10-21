// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the debug variant of a module calls: a statement starting
//! (`dbg_stmt`) and a variable's place (`dbg_var`).

#![allow(clippy::many_single_char_names)]

use codegen::abi::Env;

use super::mem::{bytes, job};
use crate::debug::{DbgVar, Resume, Step, Stop};

/// `dbg_stmt(env, n)`: stop when a breakpoint or a step says so, and
/// let the hook decide how to go on.
pub extern "sysv64" fn dbg_stmt(env: *mut Env, n: i64) {
    let j = job(env);
    let stmt = u32::try_from(n).unwrap_or(0);
    let Some(f) = j.act.frames.last() else {
        return;
    };
    let Some(p) = f.program.as_ref() else {
        return;
    };
    let program = p.key.clone();
    let procedure = p.proc_name(f.code).unwrap_or_default();
    let depth = j.depth;
    let Some(d) = j.debug.as_mut() else {
        return;
    };
    if !d.programs.contains(&program) {
        return;
    }
    let hit = d.breaks_at(&program, stmt)
        || match d.step {
            Step::None => false,
            Step::Into => true,
            Step::Over(at) => depth <= at,
        };
    if !hit {
        return;
    }
    let stop = Stop {
        program,
        procedure,
        stmt,
        depth,
    };
    d.step = Step::None;
    d.stops.push(stop.clone());
    let Some(mut hook) = d.hook.take() else {
        return;
    };
    let resume = hook(j, &stop);
    if let Some(d) = j.debug.as_mut() {
        d.hook = Some(hook);
        d.step = match resume {
            Resume::Go => Step::None,
            Resume::StepOver => Step::Over(depth),
            Resume::StepInto => Step::Into,
        };
    }
}

/// `dbg_var(env, name, addr, desc)`: the running entry registers a
/// variable; a later registration of the same name replaces it.
pub extern "sysv64" fn dbg_var(env: *mut Env, name: i64, addr: i64, desc: i64) {
    let j = job(env);
    let name = ccsid::decode_trimmed(bytes(name, 64));
    let Some(f) = j.act.frames.last_mut() else {
        return;
    };
    f.debug_vars.retain(|v| v.name != name);
    f.debug_vars.push(DbgVar {
        name,
        addr: addr.cast_unsigned(),
        desc: desc.cast_unsigned(),
    });
}
