// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `chmod [-R] mode file...`: change permission bits, with symbolic
//! or octal modes.

use qsh::sys::{FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::walk::{Control, Visit, Walker};
use crate::common::{complain_sys, mode, usage};

/// Run `chmod`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "chmod";
    // A mode such as `-w` looks like an option: take it as the operand.
    let args: Vec<String> = cx.argv[1..].to_vec();
    let (opts, rest): (Vec<String>, Vec<String>) = {
        let mut opts = Vec::new();
        let mut i = 0;
        while i < args.len() && args[i] == "-R" {
            opts.push(args[i].clone());
            i += 1;
        }
        (opts, args[i..].to_vec())
    };
    let p = match getopt(&opts, "R") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let rest: Vec<String> = if rest.first().is_some_and(|a| a == "--") {
        rest[1..].to_vec()
    } else {
        rest
    };
    if rest.len() < 2 {
        return usage(cx, name, "A mode and a file name are needed.");
    }
    let spec = &rest[0];
    if mode::parse(spec, 0, false, 0).is_none() {
        return usage(cx, name, &format!("The mode {spec} is not valid."));
    }
    let recursive = p.has('R');
    let mut status = 0;
    for f in &rest[1..] {
        let abs = cx.path(f);
        if recursive {
            let w = Walker::default();
            let sys = cx.sys;
            let umask = cx.umask;
            let mut failed: Vec<(String, qsh::sys::SysError)> = Vec::new();
            w.walk(sys, &abs, &mut |v| {
                match v {
                    Visit::Pre(e) => {
                        if e.st.ftype == FileType::Symlink {
                            return Control::Prune;
                        }
                        let m = mode::parse(spec, e.st.mode, e.st.ftype == FileType::Dir, umask)
                            .unwrap_or(e.st.mode);
                        if let Err(err) = sys.chmod(&e.path, m) {
                            failed.push((e.path.clone(), err));
                        }
                    }
                    Visit::Fail(path, err) => failed.push((path.to_string(), err)),
                    Visit::Post(_) => {}
                }
                Control::Continue
            });
            for (path, e) in failed {
                complain_sys(cx, name, &crate::common::shown(f, &abs, &path), e);
                status = 1;
            }
            continue;
        }
        let st = match cx.sys.stat(&abs) {
            Ok(st) => st,
            Err(e) => {
                complain_sys(cx, name, f, e);
                status = 1;
                continue;
            }
        };
        let m = mode::parse(spec, st.mode, st.ftype == FileType::Dir, cx.umask).unwrap_or(st.mode);
        if let Err(e) = cx.sys.chmod(&abs, m) {
            complain_sys(cx, name, f, e);
            status = 1;
        }
    }
    status
}
