// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `attr [-hp] file [name[=value] ...]`: an object's attributes. With no
//! names every attribute prints as `NAME=value`; a name prints its
//! value (`NAME=value` with `-p`); `name=value` changes it. `-h` works
//! on a symbolic link itself. `setccsid [-h] ccsid file ...` sets the
//! `CCSID` attribute.

use qsh::sys::{Errno, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

fn why(e: Errno) -> &'static str {
    match e {
        Errno::EINVAL => "The attribute or its value is not valid.",
        Errno::EPERM => "The attribute cannot be changed.",
        _ => e.text(),
    }
}

/// Run `attr`.
pub fn run_attr(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "attr";
    let p = match getopt(&cx.argv[1..], "hp") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let Some((file, names)) = p.operands.split_first() else {
        return usage(cx, name, "A file is needed.");
    };
    let follow = !p.has('h');
    let path = cx.path(file);
    let mut out = Out::stdout(cx);
    let mut status = 0;
    let mut all = match cx.sys.attributes(&path, follow) {
        Ok(a) => a,
        Err(e) => {
            complain(cx, name, &format!("{file}: {}", e.errno.text()));
            return 1;
        }
    };
    if names.is_empty() {
        for (k, v) in &all {
            out.text(&format!("{k}={v}\n"));
        }
        return out.finish(cx, name, 0);
    }
    for n in names {
        if let Some((k, v)) = n.split_once('=') {
            match cx.sys.set_attribute(&path, follow, k, v) {
                Ok(()) => {
                    if let Ok(a) = cx.sys.attributes(&path, follow) {
                        all = a;
                    }
                }
                Err(e) => {
                    out.flush();
                    complain(cx, name, &format!("{file}: {n}: {}", why(e.errno)));
                    status = 1;
                }
            }
            continue;
        }
        let up = n.to_ascii_uppercase();
        match all.iter().find(|(k, _)| *k == up) {
            Some((k, v)) if p.has('p') => {
                out.text(&format!("{k}={v}\n"));
            }
            Some((_, v)) => {
                out.text(&format!("{v}\n"));
            }
            None => {
                out.flush();
                complain(cx, name, &format!("{file}: {n}: The attribute is not known."));
                status = 1;
            }
        }
    }
    out.finish(cx, name, status)
}

/// Run `setccsid`.
pub fn run_setccsid(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "setccsid";
    let p = match getopt(&cx.argv[1..], "h") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let Some((ccsid, files)) = p.operands.split_first() else {
        return usage(cx, name, "A CCSID and a file are needed.");
    };
    if files.is_empty() {
        return usage(cx, name, "A CCSID and a file are needed.");
    }
    if !ccsid.parse::<u16>().is_ok_and(|n| n > 0) {
        return usage(cx, name, &format!("The CCSID {ccsid} is not valid."));
    }
    let mut status = 0;
    for f in files {
        if let Err(e) = cx
            .sys
            .set_attribute(&cx.path(f), !p.has('h'), "CCSID", ccsid)
        {
            complain(cx, name, &format!("{f}: {}", why(e.errno)));
            status = 1;
        }
    }
    status
}
