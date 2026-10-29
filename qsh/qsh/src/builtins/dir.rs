// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cd`, `pwd`, `umask`.

use super::{fail, opts_or_fail};
use crate::msg::Msg;
use crate::pattern::path::{absolute, join};
use crate::shell::{Res, Shell};
use crate::sys::FileType;

/// Remove `.` and `..` components without looking at the file system.
pub(crate) fn lexical(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(c),
        }
    }
    format!("/{}", parts.join("/"))
}

fn is_dir(sh: &Shell, p: &str) -> bool {
    matches!(sh.sys.stat(p), Ok(st) if st.ftype == FileType::Dir)
}

/// `cd [-L|-P] [dir|-]`.
pub(crate) fn cd(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "LP", "", false)?;
    let physical = o
        .flags
        .iter()
        .rev()
        .find(|(c, _)| *c == 'L' || *c == 'P')
        .is_some_and(|(c, _)| *c == 'P');
    let mut print = false;
    let target = match o.operands.first().map(String::as_str) {
        None => match sh.var("HOME") {
            Some(h) if !h.is_empty() => h,
            _ => {
                return Err(fail(
                    sh,
                    &Msg::Usage("cd".to_string(), "HOME is not set.".to_string()),
                    1,
                ));
            }
        },
        Some("-") => match sh.var("OLDPWD") {
            Some(p) => {
                print = true;
                p
            }
            None => {
                return Err(fail(
                    sh,
                    &Msg::Usage("cd".to_string(), "OLDPWD is not set.".to_string()),
                    1,
                ));
            }
        },
        Some(d) => d.to_string(),
    };
    let mut path = None;
    let first = target.split('/').next().unwrap_or("");
    if !target.starts_with('/')
        && first != "."
        && first != ".."
        && let Some(cdpath) = sh.var("CDPATH")
    {
        for dir in cdpath.split(':') {
            let cand = absolute(
                &sh.cwd,
                &join(if dir.is_empty() { "." } else { dir }, &target),
            );
            if is_dir(sh, &cand) {
                print |= !dir.is_empty();
                path = Some(cand);
                break;
            }
        }
    }
    let path = path.unwrap_or_else(|| absolute(&sh.cwd, &target));
    let new = if physical {
        match sh.sys.realpath(&path) {
            Ok(p) => p,
            Err(e) => return Err(fail(sh, &Msg::Cd(target, e.to_string()), 1)),
        }
    } else {
        lexical(&path)
    };
    if let Err(e) = sh.sys.chdir(&new) {
        return Err(fail(sh, &Msg::Cd(target, e.to_string()), 1));
    }
    let old = sh.cwd.clone();
    sh.cwd.clone_from(&new);
    let _ = sh.vars.set("OLDPWD", &old);
    let _ = sh.vars.set("PWD", &new);
    if print {
        sh.out(&format!("{new}\n"))?;
    }
    Ok(0)
}

/// `pwd [-L|-P]`.
pub(crate) fn pwd(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "LP", "", false)?;
    let physical = o
        .flags
        .iter()
        .rev()
        .find(|(c, _)| *c == 'L' || *c == 'P')
        .is_some_and(|(c, _)| *c == 'P');
    let p = if physical {
        match sh.sys.realpath(&sh.cwd) {
            Ok(p) => p,
            Err(e) => return Err(fail(sh, &Msg::Failed("pwd".to_string(), e.to_string()), 1)),
        }
    } else {
        sh.cwd.clone()
    };
    sh.out(&format!("{p}\n"))?;
    Ok(0)
}

fn symbolic(mask: u32) -> String {
    let perm = !mask & 0o777;
    let who = |shift: u32| {
        let b = (perm >> shift) & 7;
        let mut s = String::new();
        if b & 4 != 0 {
            s.push('r');
        }
        if b & 2 != 0 {
            s.push('w');
        }
        if b & 1 != 0 {
            s.push('x');
        }
        s
    };
    format!("u={},g={},o={}", who(6), who(3), who(0))
}

/// Apply a symbolic mode (`u=rwx,g+w,o-r`) to permission bits.
fn apply_symbolic(perm: u32, spec: &str) -> Option<u32> {
    let mut perm = perm;
    for clause in spec.split(',') {
        let opi = clause.find(['=', '+', '-'])?;
        let (who, rest) = clause.split_at(opi);
        let mut mask = 0u32;
        for c in who.chars() {
            mask |= match c {
                'u' => 0o700,
                'g' => 0o070,
                'o' => 0o007,
                'a' => 0o777,
                _ => return None,
            };
        }
        if who.is_empty() {
            mask = 0o777;
        }
        let op = rest.chars().next()?;
        let mut bits = 0u32;
        for c in rest[1..].chars() {
            bits |= match c {
                'r' => 0o444,
                'w' => 0o222,
                'x' => 0o111,
                _ => return None,
            };
        }
        let bits = bits & mask;
        perm = match op {
            '=' => (perm & !mask) | bits,
            '+' => perm | bits,
            _ => perm & !bits,
        };
    }
    Some(perm)
}

/// `umask [-S] [mask]`.
pub(crate) fn umask(sh: &mut Shell, args: &[String]) -> Res {
    let o = opts_or_fail(sh, args, "S", "", false)?;
    match o.operands.first() {
        None => {
            let s = if o.has('S') {
                symbolic(sh.umask)
            } else {
                format!("{:04o}", sh.umask)
            };
            sh.out(&format!("{s}\n"))?;
            Ok(0)
        }
        Some(m) => {
            let new = if m.chars().all(|c| c.is_digit(8)) {
                u32::from_str_radix(m, 8).ok().filter(|v| *v <= 0o777)
            } else {
                apply_symbolic(!sh.umask & 0o777, m).map(|p| !p & 0o777)
            };
            match new {
                Some(v) => {
                    sh.umask = v;
                    Ok(0)
                }
                None => Err(fail(
                    sh,
                    &Msg::Usage("umask".to_string(), format!("mask {m} is not valid.")),
                    1,
                )),
            }
        }
    }
}
