// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `mkdir [-p] [-m mode] dir...`.

use qsh::sys::{Errno, FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::{complain, complain_sys, mode, usage};

/// Run `mkdir`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "mkdir";
    let p = match getopt(&cx.argv[1..], "pm:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, name, "A directory name is needed.");
    }
    let parents = p.has('p');
    let base = 0o777 & !cx.umask;
    let mode = match p.value('m') {
        None => base,
        Some(m) => match mode::parse(m, base, true, cx.umask) {
            Some(v) => v,
            None => return usage(cx, name, &format!("The mode {m} is not valid.")),
        },
    };
    let mut status = 0;
    for d in &p.operands {
        let abs = cx.path(d);
        if parents {
            let mut cur = String::from("/");
            let comps: Vec<&str> = abs.split('/').filter(|c| !c.is_empty()).collect();
            for (i, c) in comps.iter().enumerate() {
                cur = crate::file::join(&cur, c);
                let last = i + 1 == comps.len();
                match cx.sys.stat(&cur) {
                    Ok(st) if st.ftype == FileType::Dir => continue,
                    Ok(_) => {
                        let e = if last { Errno::EEXIST } else { Errno::ENOTDIR };
                        complain_sys(cx, name, d, e.into());
                        status = 1;
                        break;
                    }
                    Err(_) => {}
                }
                // Intermediate directories get search permission too.
                let m = if last { mode } else { (0o777 & !cx.umask) | 0o300 };
                if let Err(e) = cx.sys.mkdir(&cur, m) {
                    if e.errno == Errno::EEXIST && !last {
                        continue;
                    }
                    complain_sys(cx, name, d, e);
                    status = 1;
                    break;
                }
            }
        } else if let Err(e) = cx.sys.mkdir(&abs, mode) {
            if e.errno == Errno::EEXIST {
                complain(cx, name, &format!("{d}: The file already exists."));
            } else {
                complain_sys(cx, name, d, e);
            }
            status = 1;
        }
    }
    status
}
