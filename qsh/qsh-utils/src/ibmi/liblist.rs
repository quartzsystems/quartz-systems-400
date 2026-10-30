// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `liblist [-a [-f | -l] | -d | -c] [library ...]`: show the job's
//! library list (each library and its portion, `SYS`, `PRD`, `CUR` or
//! `USR`), add libraries to the user portion (`-a`, at the front or
//! with `-l` the end; `-f` and `-l` alone imply `-a`), remove them
//! (`-d`) or set the current library (`-c`, `*CRTDFT` for none). In
//! Qshell this is a builtin; here it is a utility over the job's list.

use qsh::sys::{Errno, LiblChange, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "liblist";

/// Is this a library name (`A`-`Z`, `$#@` first, then those, digits,
/// `_` and `.`, ten at most)?
pub fn valid_name(n: &str) -> bool {
    let b = n.as_bytes();
    !b.is_empty()
        && b.len() <= 10
        && (b[0].is_ascii_alphabetic() || b"$#@".contains(&b[0]))
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || b"$#@_.".contains(c))
}

/// Run `liblist`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "acdfl") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let add = p.has('a') || p.has('f') || p.has('l');
    let modes = [add, p.has('d'), p.has('c')]
        .iter()
        .filter(|b| **b)
        .count();
    if modes > 1 || (p.has('f') && p.has('l')) {
        return usage(cx, NAME, "Use one of -a, -d and -c.");
    }
    if modes == 0 {
        if !p.operands.is_empty() {
            return usage(cx, NAME, "Libraries are taken with -a, -d or -c.");
        }
        let list = match cx.sys.library_list() {
            Ok(l) => l,
            Err(e) => {
                complain(
                    cx,
                    NAME,
                    &format!("The library list is not available: {}", e.errno.text()),
                );
                return 1;
            }
        };
        let mut out = Out::stdout(cx);
        for e in list {
            out.text(&format!("{:<10} {}\n", e.name, e.kind.code()));
        }
        return out.finish(cx, NAME, 0);
    }
    if p.operands.is_empty() {
        return usage(cx, NAME, "A library is needed.");
    }
    if p.has('c') && p.operands.len() > 1 {
        return usage(cx, NAME, "Only one current library is taken.");
    }
    let mut status = 0;
    // Adding to the front keeps the operands' order.
    let names: Vec<&String> = if add && !p.has('l') {
        p.operands.iter().rev().collect()
    } else {
        p.operands.iter().collect()
    };
    for n in names {
        let up = n.to_ascii_uppercase();
        let special = p.has('c') && up == "*CRTDFT";
        if !special && !valid_name(&up) {
            complain(cx, NAME, &format!("{n}: The library name is not valid."));
            status = 1;
            continue;
        }
        let ch = if p.has('d') {
            LiblChange::Remove(up)
        } else if p.has('c') {
            LiblChange::SetCurrent(up)
        } else if p.has('l') {
            LiblChange::AddLast(up)
        } else {
            LiblChange::AddFirst(up)
        };
        if let Err(e) = cx.sys.change_library_list(&ch) {
            let why = match e.errno {
                Errno::EEXIST => "The library is already in the library list.",
                Errno::ENOENT if p.has('d') => {
                    "The library is not in the user portion of the library list."
                }
                Errno::ENOENT => "The library is not found.",
                _ => e.errno.text(),
            };
            complain(cx, NAME, &format!("{n}: {why}"));
            status = 1;
        }
    }
    status
}
