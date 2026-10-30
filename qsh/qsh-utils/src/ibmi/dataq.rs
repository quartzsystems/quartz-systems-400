// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dataq [-s | -r | -c | -n] [-p] [-k key] [-t seconds] path [entry
//! ...]`: data queue entries. `-s` sends the operands (joined by
//! blanks; the default when there are any), `-r` receives one and
//! writes it with a newline (the default without operands), `-p` with
//! `-r` leaves it on the queue, `-c` clears the queue, `-n` shows how
//! many entries it holds. `-k` is the key on a keyed queue, `-t` how
//! long `-r` waits (0, the default, not at all; -1 for ever). Status 1
//! when no entry came.

use std::time::Duration;

use qsh::sys::{DtaqOp, DtaqReply, Errno, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "dataq";

/// Run `dataq`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "srcnpk:t:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let modes = ['s', 'r', 'c', 'n'].iter().filter(|c| p.has(**c)).count();
    if modes > 1 {
        return usage(cx, NAME, "Use one of -s, -r, -c and -n.");
    }
    let Some((queue, data)) = p.operands.split_first() else {
        return usage(cx, NAME, "A data queue is needed.");
    };
    let key = p.value('k').map(|k| k.as_bytes().to_vec());
    let wait = match p.value('t') {
        None => Some(Duration::ZERO),
        Some("-1") => None,
        Some(t) => match t.parse::<u64>() {
            Ok(n) => Some(Duration::from_secs(n)),
            Err(_) => return usage(cx, NAME, &format!("The wait time {t} is not valid.")),
        },
    };
    let send = p.has('s') || (modes == 0 && !data.is_empty());
    if !send && !data.is_empty() {
        return usage(cx, NAME, "An entry is taken only when sending.");
    }
    if send && data.is_empty() {
        return usage(cx, NAME, "An entry to send is needed.");
    }
    let op = if send {
        DtaqOp::Send {
            data: data.join(" ").into_bytes(),
            key,
        }
    } else if p.has('c') {
        DtaqOp::Clear
    } else if p.has('n') {
        DtaqOp::Count
    } else {
        DtaqOp::Receive {
            key,
            wait,
            remove: !p.has('p'),
        }
    };
    let path = cx.path(queue);
    let reply = match cx.sys.data_queue(&path, &op) {
        Ok(r) => r,
        Err(e) => {
            let why = match e.errno {
                Errno::ENOENT => "The data queue is not found.",
                _ => e.errno.text(),
            };
            complain(cx, NAME, &format!("{queue}: {why}"));
            return 1;
        }
    };
    let mut out = Out::stdout(cx);
    let status = match reply {
        DtaqReply::Done => 0,
        DtaqReply::Count(n) => {
            out.text(&format!("{n}\n"));
            0
        }
        DtaqReply::Entry(Some(d)) => {
            out.write(&d);
            out.text("\n");
            0
        }
        DtaqReply::Entry(None) => 1,
    };
    out.finish(cx, NAME, status)
}
