// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `id [-G | -g | -u] [-nr] [user]`: user and group IDs as
//! `uid=n(NAME) gid=n(NAME) groups=...` (a group without a profile
//! shows as its number alone, as `gid=0` on the system). `logname`:
//! the job's user profile name.

use qsh::sys::{Credentials, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

fn named(n: u32, name: Option<String>) -> String {
    match name {
        Some(s) => format!("{n}({s})"),
        None => n.to_string(),
    }
}

fn who(cx: &UtilCtx<'_>, user: Option<&str>) -> Result<Credentials, String> {
    let me = cx.sys.credentials();
    match user {
        None => me.map_err(|e| format!("The job's user is not known: {}", e.errno.text())),
        Some(u) => {
            if let Ok(c) = &me
                && c.user.eq_ignore_ascii_case(u)
            {
                return Ok(c.clone());
            }
            let uid = cx
                .sys
                .user_id(u)
                .or_else(|| u.parse().ok().filter(|n| cx.sys.user_name(*n).is_some()))
                .ok_or_else(|| format!("{u}: The user is not known."))?;
            Ok(Credentials {
                user: cx.sys.user_name(uid).unwrap_or_else(|| u.to_ascii_uppercase()),
                uid,
                gid: 0,
                groups: Vec::new(),
            })
        }
    }
}

/// Run `id`.
pub fn run_id(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "id";
    let p = match getopt(&cx.argv[1..], "Ggunr") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let picks = ['G', 'g', 'u'].iter().filter(|c| p.has(**c)).count();
    if picks > 1 {
        return usage(cx, name, "Use one of -G, -g and -u.");
    }
    if picks == 0 && (p.has('n') || p.has('r')) {
        return usage(cx, name, "-n and -r are taken with -G, -g or -u.");
    }
    if p.operands.len() > 1 {
        return usage(cx, name, "At most one user is taken.");
    }
    let c = match who(cx, p.operands.first().map(String::as_str)) {
        Ok(c) => c,
        Err(e) => {
            complain(cx, name, &e);
            return 1;
        }
    };
    let n = p.has('n');
    let gname = |g: u32| cx.sys.group_name(g);
    let text = if p.has('u') {
        if n { c.user.clone() } else { c.uid.to_string() }
    } else if p.has('g') {
        if n {
            gname(c.gid).unwrap_or_else(|| c.gid.to_string())
        } else {
            c.gid.to_string()
        }
    } else if p.has('G') {
        let mut all = vec![c.gid];
        all.extend(c.groups.iter().filter(|g| **g != c.gid));
        all.iter()
            .map(|g| {
                if n {
                    gname(*g).unwrap_or_else(|| g.to_string())
                } else {
                    g.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        let mut s = format!(
            "uid={} gid={}",
            named(c.uid, Some(c.user.clone())),
            named(c.gid, gname(c.gid))
        );
        if !c.groups.is_empty() {
            let gs: Vec<String> = c.groups.iter().map(|g| named(*g, gname(*g))).collect();
            s.push_str(&format!(" groups={}", gs.join(",")));
        }
        s
    };
    let mut out = Out::stdout(cx);
    out.text(&text);
    out.text("\n");
    out.finish(cx, name, 0)
}

/// Run `logname`.
pub fn run_logname(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "logname";
    if cx.argv.len() > 1 {
        return usage(cx, name, "No operands are taken.");
    }
    let user = cx
        .sys
        .credentials()
        .map(|c| c.user)
        .ok()
        .or_else(|| cx.getenv("LOGNAME").map(ToString::to_string));
    let Some(user) = user else {
        complain(cx, name, "The job's user is not known.");
        return 1;
    };
    let mut out = Out::stdout(cx);
    out.text(&user);
    out.text("\n");
    out.finish(cx, name, 0)
}
