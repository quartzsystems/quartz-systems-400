// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `du [-a | -s] [-kx] [-H | -L] [file...]`: storage used, in
//! 512-byte blocks (`-k`: kilobytes), one line per directory.

use std::collections::HashSet;

use qsh::sys::{FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::walk::{Control, Visit, Walker};
use crate::common::{complain_sys, usage};

/// Run `du`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "du";
    let p = match getopt(&cx.argv[1..], "askxHL") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let all = p.has('a');
    let summary = p.has('s');
    if all && summary {
        return usage(cx, name, "-a and -s cannot both be given.");
    }
    let kilo = p.has('k');
    let w = Walker {
        follow_root: p.has('H') || p.has('L'),
        follow_all: p.has('L'),
        one_fs: p.has('x'),
    };
    let operands = if p.operands.is_empty() {
        vec![".".to_string()]
    } else {
        p.operands.clone()
    };
    let mut out = Out::stdout(cx);
    let mut status = 0;
    let show = |blocks: u64| -> u64 {
        if kilo {
            blocks.div_ceil(2)
        } else {
            blocks
        }
    };
    for op in &operands {
        let abs = cx.path(op);
        // The shown path: the operand with the walked suffix.
        let prefix_len = abs.len();
        let mut stack: Vec<u64> = Vec::new();
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut errors = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        let sys = cx.sys;
        w.walk(sys, &abs, &mut |v| {
            match v {
                Visit::Pre(e) => {
                    let shown = format!("{op}{}", &e.path[prefix_len.min(e.path.len())..]);
                    if e.st.ftype == FileType::Dir {
                        stack.push(e.st.blocks);
                        return Control::Continue;
                    }
                    let counted = e.st.nlink <= 1 || seen.insert((e.st.dev, e.st.ino));
                    let b = if counted { e.st.blocks } else { 0 };
                    match stack.last_mut() {
                        Some(top) => *top += b,
                        None => lines.push(format!("{}\t{}", show(b), shown)),
                    }
                    if all && counted && !stack.is_empty() && !summary {
                        lines.push(format!("{}\t{}", show(b), shown));
                    }
                }
                Visit::Post(e) => {
                    let shown = format!("{op}{}", &e.path[prefix_len.min(e.path.len())..]);
                    let total = stack.pop().unwrap_or(0);
                    if let Some(top) = stack.last_mut() {
                        *top += total;
                    }
                    if !summary || stack.is_empty() {
                        lines.push(format!("{}\t{}", show(total), shown));
                    }
                }
                Visit::Fail(path, e) => {
                    let shown = format!("{op}{}", &path[prefix_len.min(path.len())..]);
                    errors.push((shown, e));
                }
            }
            Control::Continue
        });
        for (path, e) in errors {
            complain_sys(cx, name, &path, e);
            status = 1;
        }
        for l in lines {
            out.text(&l);
            out.text("\n");
        }
    }
    out.finish(cx, name, status)
}
