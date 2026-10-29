// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `chown [-hR] owner[:group] file...` and `chgrp [-hR] group file...`:
//! change the owner and group, by profile name or number.

use qsh::sys::{FileType, UtilCtx};

use crate::common::args::getopt;
use crate::common::walk::{Control, Visit, Walker};
use crate::common::{complain, complain_sys, usage};

/// Run `chown`.
pub fn run_chown(cx: &mut UtilCtx<'_>) -> i32 {
    change(cx, "chown", false)
}

/// Run `chgrp`.
pub fn run_chgrp(cx: &mut UtilCtx<'_>) -> i32 {
    change(cx, "chgrp", true)
}

fn user(cx: &UtilCtx<'_>, s: &str) -> Option<u32> {
    cx.sys
        .user_id(s)
        .or_else(|| s.parse().ok())
}

fn group(cx: &UtilCtx<'_>, s: &str) -> Option<u32> {
    cx.sys
        .group_id(s)
        .or_else(|| s.parse().ok())
}

fn change(cx: &mut UtilCtx<'_>, name: &str, group_only: bool) -> i32 {
    let p = match getopt(&cx.argv[1..], "hR") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if p.operands.len() < 2 {
        return usage(
            cx,
            name,
            if group_only {
                "A group and a file name are needed."
            } else {
                "An owner and a file name are needed."
            },
        );
    }
    let spec = &p.operands[0];
    let (uid, gid) = if group_only {
        match group(cx, spec) {
            Some(g) => (None, Some(g)),
            None => {
                complain(cx, name, &format!("{spec}: The group is not known."));
                return 1;
            }
        }
    } else {
        let (u, g) = match spec.split_once(':') {
            Some((u, g)) => (u, Some(g)),
            None => (spec.as_str(), None),
        };
        let uid = if u.is_empty() {
            None
        } else {
            match user(cx, u) {
                Some(id) => Some(id),
                None => {
                    complain(cx, name, &format!("{u}: The user is not known."));
                    return 1;
                }
            }
        };
        let gid = match g {
            None | Some("") => None,
            Some(g) => match group(cx, g) {
                Some(id) => Some(id),
                None => {
                    complain(cx, name, &format!("{g}: The group is not known."));
                    return 1;
                }
            },
        };
        (uid, gid)
    };
    let recursive = p.has('R');
    let mut status = 0;
    for f in &p.operands[1..] {
        let abs = cx.path(f);
        if recursive {
            let sys = cx.sys;
            let mut failed = Vec::new();
            Walker::default().walk(sys, &abs, &mut |v| {
                match v {
                    Visit::Pre(e) => {
                        if e.st.ftype == FileType::Symlink {
                            return Control::Prune;
                        }
                        if let Err(err) = sys.chown(&e.path, uid, gid) {
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
        if let Err(e) = cx.sys.stat(&abs).and_then(|_| cx.sys.chown(&abs, uid, gid)) {
            complain_sys(cx, name, f, e);
            status = 1;
        }
    }
    status
}
