// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `logger [-i] [-f file] [-p priority] [-t tag] [message ...]`: put a
//! message in the history log `QHST` (through `SNDMSG`). The operands
//! are one message; without them each line of the file (`-f`) or of
//! standard input is one. `-t` puts `tag: ` before it, `-i` the
//! process ID in brackets after the tag; `-p` (`facility.level`) is
//! checked and otherwise has no effect.

use qsh::sys::UtilCtx;

use super::msgs::{quote, run_quiet};
use crate::common::args::getopt;
use crate::common::io::{Lines, input};
use crate::common::{complain_sys, usage};

const NAME: &str = "logger";

const FACILITIES: &[&str] = &[
    "auth", "authpriv", "cron", "daemon", "ftp", "kern", "lpr", "mail", "news", "syslog",
    "user", "uucp", "local0", "local1", "local2", "local3", "local4", "local5", "local6",
    "local7", "security",
];
const LEVELS: &[&str] = &[
    "emerg", "alert", "crit", "err", "warning", "notice", "info", "debug", "panic", "error",
    "warn",
];

/// Is this a `facility.level` (or a level alone)?
pub fn valid_priority(p: &str) -> bool {
    let (f, l) = p.split_once('.').unwrap_or(("user", p));
    FACILITIES.contains(&f.to_ascii_lowercase().as_str())
        && LEVELS.contains(&l.to_ascii_lowercase().as_str())
}

/// Run `logger`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "if:p:t:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if let Some(pr) = p.value('p')
        && !valid_priority(pr)
    {
        return usage(cx, NAME, &format!("The priority {pr} is not valid."));
    }
    let mut prefix = p.value('t').unwrap_or_default().to_string();
    if p.has('i') {
        prefix.push_str(&format!("[{}]", cx.sys.pid()));
    }
    if !prefix.is_empty() {
        prefix.push_str(": ");
    }
    let mut messages = Vec::new();
    if p.operands.is_empty() {
        let src = p.value('f').unwrap_or("-");
        let fd = match input(cx, src) {
            Ok(f) => f,
            Err(e) => {
                complain_sys(cx, NAME, src, e);
                return 1;
            }
        };
        let mut lines = Lines::new(fd);
        while let Ok(Some((l, _))) = lines.next_text() {
            let l = String::from_utf8_lossy(&l).into_owned();
            if !l.is_empty() {
                messages.push(l);
            }
        }
    } else {
        messages.push(p.operands.join(" "));
    }
    let mut status = 0;
    for m in messages {
        let cmd = format!(
            "SNDMSG MSG({}) TOMSGQ(QSYS/QHST)",
            quote(&format!("{prefix}{m}"))
        );
        if !run_quiet(cx, NAME, &cmd) {
            status = 1;
        }
    }
    status
}
