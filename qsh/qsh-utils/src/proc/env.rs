// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `env [-i] [name=value]... [utility [argument...]]`: print the
//! environment, or run a utility with changes to it.

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::exec;
use crate::common::io::Out;
use crate::common::{complain, usage};

/// Run `env`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "env";
    let p = match getopt(&cx.argv[1..], "i") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut env: Vec<(String, String)> = if p.has('i') { Vec::new() } else { cx.env.clone() };
    let mut rest = p.operands.as_slice();
    while let Some(a) = rest.first() {
        let Some((k, v)) = a.split_once('=') else {
            break;
        };
        if k.is_empty() {
            break;
        }
        env.retain(|(n, _)| n != k);
        env.push((k.to_string(), v.to_string()));
        rest = &rest[1..];
    }
    if rest.is_empty() {
        let mut out = Out::stdout(cx);
        for (k, v) in &env {
            out.text(&format!("{k}={v}\n"));
        }
        return out.finish(cx, name, 0);
    }
    match exec::run(cx, rest, None, Some(env)) {
        Ok(st) => st,
        Err(e) => {
            complain(cx, name, &format!("{}: {}", rest[0], e.text()));
            e.status()
        }
    }
}

/// Run `printenv [name...]`: the whole environment as `name=value`,
/// or each named value; status 1 when a name is not set.
pub fn run_printenv(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "printenv";
    let p = match getopt(&cx.argv[1..], "") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut out = Out::stdout(cx);
    if p.operands.is_empty() {
        for (k, v) in &cx.env {
            out.text(&format!("{k}={v}
"));
        }
        return out.finish(cx, name, 0);
    }
    let mut status = 0;
    for n in &p.operands {
        match cx.getenv(n) {
            Some(v) => {
                out.text(&format!("{v}
"));
            }
            None => status = 1,
        }
    }
    out.finish(cx, name, status)
}
