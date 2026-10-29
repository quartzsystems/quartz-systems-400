// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sed [-En] [-e script]... [-f file]... [file...]`, `sed [-En] script
//! [file...]`: the POSIX stream editor. [`parse`] reads the script,
//! [`exec`] runs the cycle over the inputs.

pub mod exec;
pub mod parse;

use qsh::sys::UtilCtx;

use crate::common::args::getopt;
use crate::common::io::read_input;
use crate::common::{complain, complain_sys, usage};

/// Run `sed`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "sed";
    let p = match getopt(&cx.argv[1..], "Ernae:f:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let extended = p.has('E') || p.has('r');
    let mut quiet = p.has('n');
    let mut script: Vec<u8> = Vec::new();
    let mut given = false;
    for (o, v) in &p.opts {
        match o {
            'e' => {
                script.extend_from_slice(v.as_deref().unwrap_or("").as_bytes());
                script.push(b'\n');
                given = true;
            }
            'f' => {
                let f = v.as_deref().unwrap_or("");
                match read_input(cx, f) {
                    Ok(d) => {
                        script.extend(d);
                        script.push(b'\n');
                        given = true;
                    }
                    Err(e) => {
                        complain_sys(cx, name, f, e);
                        return 1;
                    }
                }
            }
            _ => {}
        }
    }
    let mut operands = p.operands.clone();
    if !given {
        if operands.is_empty() {
            return usage(cx, name, "A script is needed.");
        }
        script = operands.remove(0).into_bytes();
        script.push(b'\n');
    }
    if script.starts_with(b"#n\n") {
        quiet = true;
    }
    let prog = match parse::parse(cx, &script, extended) {
        Ok(s) => s,
        Err(e) => {
            complain(cx, name, &e);
            return 1;
        }
    };
    exec::run(cx, name, prog, quiet, &operands)
}
