// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `clrtmp [-c]`: remove everything in `/tmp` (the directory stays).
//! `-c` removes only what the job's user owns, and directories left
//! empty by that. Objects that cannot be removed are named on standard
//! error and the status is 1.

use qsh::sys::{Errno, FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::walk::{Control, Visit, Walker};
use crate::common::{complain, usage};

const NAME: &str = "clrtmp";

/// The directory cleared.
pub const TMP: &str = "/tmp";

/// Run `clrtmp`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "c") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if !p.operands.is_empty() {
        return usage(cx, NAME, "No operands are taken.");
    }
    let mine = if p.has('c') {
        match cx.sys.credentials() {
            Ok(c) => Some(c.uid),
            Err(e) => {
                complain(cx, NAME, &format!("The user is not known: {}", e.errno.text()));
                return 1;
            }
        }
    } else {
        None
    };
    let owned = |uid: u32| mine.is_none_or(|u| u == uid);
    // Files first, then directories deepest first.
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    let mut errors: Vec<(String, &'static str)> = Vec::new();
    Walker::default().walk(cx.sys, TMP, &mut |v| {
        match v {
            Visit::Pre(e) if e.depth > 0 && e.st.ftype != FileType::Dir => {
                if owned(e.st.uid) {
                    files.push(e.path.clone());
                }
            }
            Visit::Post(e) if e.depth > 0 => dirs.push((e.path.clone(), e.st.uid)),
            Visit::Fail(path, err) => errors.push((path.to_string(), err.errno.text())),
            _ => {}
        }
        Control::Continue
    });
    for f in files {
        if let Err(e) = cx.sys.unlink(&f) {
            errors.push((f, e.errno.text()));
        }
    }
    for (d, uid) in dirs {
        if !owned(uid) {
            continue;
        }
        let empty = cx.sys.read_dir(&d).is_ok_and(|n| n.is_empty());
        if !empty {
            // Under -c a directory holding others' files stays quietly.
            if mine.is_none() {
                errors.push((d, Errno::ENOTEMPTY.text()));
            }
            continue;
        }
        if let Err(e) = cx.sys.rmdir(&d) {
            errors.push((d, e.errno.text()));
        }
    }
    let status = i32::from(!errors.is_empty());
    for (path, why) in errors {
        complain(cx, NAME, &format!("{path}: {why}"));
    }
    status
}
