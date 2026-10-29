// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cp [-fip] source target`, `cp [-fip] source... directory`,
//! `cp -R [-H | -L | -P] [-fip] source... target`: copy files and,
//! with `-R`, trees. A member of a `/QSYS.LIB` file is copied as its
//! records through the system.

use qsh::sys::{Errno, FileType, OpenFlags, Stat, Sys, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::{READ, confirm, copy};
use crate::common::{complain, complain_sys, usage};
use crate::file::{basename, join};

/// How `cp` copies.
#[derive(Clone, Copy, Default)]
pub struct Opts {
    /// `-f`: remove a destination that cannot be opened.
    pub force: bool,
    /// `-i`: ask before replacing.
    pub interactive: bool,
    /// `-p`: keep mode, owner and times.
    pub preserve: bool,
    /// `-R`: copy trees.
    pub recursive: bool,
    /// `-H`: follow links named as operands.
    pub follow_root: bool,
    /// `-L`: follow every link.
    pub follow_all: bool,
}

fn write_flags() -> OpenFlags {
    OpenFlags {
        write: true,
        create: true,
        truncate: true,
        ..OpenFlags::default()
    }
}

/// Copy one regular file's bytes and attributes.
#[allow(clippy::too_many_arguments)]
fn copy_file(
    cx: &UtilCtx<'_>,
    name: &str,
    src: &str,
    src_abs: &str,
    st: &Stat,
    dst: &str,
    dst_abs: &str,
    o: Opts,
) -> bool {
    let sys: &dyn Sys = cx.sys;
    let from = match sys.open(src_abs, READ, 0) {
        Ok(fd) => fd,
        Err(e) => {
            complain_sys(cx, name, src, e);
            return false;
        }
    };
    let exists = sys.stat(dst_abs).is_ok();
    if exists && o.interactive && !confirm(cx, &format!("{name}: replace {dst}? ")) {
        return true;
    }
    let mode = if o.preserve {
        st.mode
    } else {
        st.mode & 0o777 & !cx.umask
    };
    let to = match sys.open(dst_abs, write_flags(), mode) {
        Ok(fd) => fd,
        Err(e) if o.force && exists && e.errno != Errno::EISDIR => {
            if let Err(e) = sys.unlink(dst_abs) {
                complain_sys(cx, name, dst, e);
                return false;
            }
            match sys.open(dst_abs, write_flags(), mode) {
                Ok(fd) => fd,
                Err(e) => {
                    complain_sys(cx, name, dst, e);
                    return false;
                }
            }
        }
        Err(e) => {
            complain_sys(cx, name, dst, e);
            return false;
        }
    };
    if let Err(e) = copy(&from, &to) {
        complain_sys(cx, name, dst, e.into());
        return false;
    }
    drop(to);
    if o.preserve {
        let _ = sys.chmod(dst_abs, st.mode);
        let _ = sys.utimes(dst_abs, st.mtime);
        let _ = sys.chown(dst_abs, Some(st.uid), Some(st.gid));
    }
    true
}

/// Copy a path (file, link or tree) to `dst`.
pub fn copy_path(
    cx: &UtilCtx<'_>,
    name: &str,
    src: &str,
    dst: &str,
    o: Opts,
    root: bool,
) -> bool {
    let sys: &dyn Sys = cx.sys;
    let src_abs = cx.path(src);
    let dst_abs = cx.path(dst);
    let follow = !o.recursive || o.follow_all || (root && o.follow_root);
    let st = match if follow {
        sys.stat(&src_abs)
    } else {
        sys.lstat(&src_abs)
    } {
        Ok(st) => st,
        Err(e) => {
            complain_sys(cx, name, src, e);
            return false;
        }
    };
    match st.ftype {
        FileType::Dir => {
            if !o.recursive {
                complain(cx, name, &format!("{src}: The target is a directory."));
                return false;
            }
            if let Ok(dst_st) = sys.stat(&dst_abs) {
                if dst_st.ftype != FileType::Dir {
                    complain(cx, name, &format!("{dst}: The path is not a directory."));
                    return false;
                }
                if dst_st.dev == st.dev && dst_st.ino == st.ino {
                    complain(cx, name, &format!("{src} and {dst} are the same file."));
                    return false;
                }
            } else if let Err(e) = sys.mkdir(&dst_abs, (st.mode & 0o777 & !cx.umask) | 0o700) {
                complain_sys(cx, name, dst, e);
                return false;
            }
            let mut ok = true;
            match sys.read_dir(&src_abs) {
                Ok(mut names) => {
                    names.sort();
                    for n in names {
                        ok &= copy_path(cx, name, &join(src, &n), &join(dst, &n), o, false);
                    }
                }
                Err(e) => {
                    complain_sys(cx, name, src, e);
                    ok = false;
                }
            }
            let mode = if o.preserve {
                st.mode
            } else {
                st.mode & 0o777 & !cx.umask
            };
            let _ = sys.chmod(&dst_abs, mode);
            if o.preserve {
                let _ = sys.utimes(&dst_abs, st.mtime);
                let _ = sys.chown(&dst_abs, Some(st.uid), Some(st.gid));
            }
            ok
        }
        FileType::Symlink => {
            let target = match sys.readlink(&src_abs) {
                Ok(t) => t,
                Err(e) => {
                    complain_sys(cx, name, src, e);
                    return false;
                }
            };
            if sys.lstat(&dst_abs).is_ok() {
                if o.interactive && !confirm(cx, &format!("{name}: replace {dst}? ")) {
                    return true;
                }
                if let Err(e) = sys.unlink(&dst_abs) {
                    complain_sys(cx, name, dst, e);
                    return false;
                }
            }
            match sys.symlink(&target, &dst_abs) {
                Ok(()) => true,
                Err(e) => {
                    complain_sys(cx, name, dst, e);
                    false
                }
            }
        }
        _ => {
            if let Ok(d) = sys.stat(&dst_abs)
                && d.dev == st.dev
                && d.ino == st.ino
            {
                complain(cx, name, &format!("{src} and {dst} are the same file."));
                return false;
            }
            copy_file(cx, name, src, &src_abs, &st, dst, &dst_abs, o)
        }
    }
}

/// Is a path an existing directory?
pub fn is_dir(cx: &UtilCtx<'_>, path: &str) -> bool {
    cx.sys
        .stat(&cx.path(path))
        .is_ok_and(|st| st.ftype == FileType::Dir)
}

/// Run `cp`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "cp";
    let p = match getopt(&cx.argv[1..], "fipRrHLP") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let mut o = Opts {
        force: p.has('f'),
        interactive: p.has('i'),
        preserve: p.has('p'),
        recursive: p.has('R') || p.has('r'),
        ..Opts::default()
    };
    for (c, _) in &p.opts {
        match c {
            'H' => {
                o.follow_root = true;
                o.follow_all = false;
            }
            'L' => {
                o.follow_all = true;
                o.follow_root = false;
            }
            'P' => {
                o.follow_all = false;
                o.follow_root = false;
            }
            _ => {}
        }
    }
    if p.operands.len() < 2 {
        return usage(cx, name, "A source and a target are needed.");
    }
    let target = p.operands.last().cloned().unwrap_or_default();
    let sources = &p.operands[..p.operands.len() - 1];
    let target_is_dir = is_dir(cx, &target);
    if sources.len() > 1 && !target_is_dir {
        complain(cx, name, &format!("{target}: The path is not a directory."));
        return 1;
    }
    let mut status = 0;
    for src in sources {
        let dst = if target_is_dir {
            join(&target, basename(src))
        } else {
            target.clone()
        };
        if !copy_path(cx, name, src, &dst, o, true) {
            status = 1;
        }
    }
    status
}
