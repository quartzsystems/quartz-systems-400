// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `system [-beikKnqsv] command [argument ...]`: run a CL command (the
//! operands joined by blanks). The text of the spooled files it makes
//! goes to standard output, then its messages (see [`super::msgs`]).
//! `-i` runs it in the shell's job instead of a new one, `-e` passes
//! the environment, `-k`/`-K` keep the spooled files and job log, `-n`
//! leaves out message identifiers, `-q` the messages, `-s` the spooled
//! output, `-v` shows the command first; `-b` is accepted. Status 0, or
//! 255 when the command ended with an escape message.

use qsh::sys::{ClRequest, UtilCtx};

use super::msgs;
use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain, usage};

const NAME: &str = "system";

/// The status of a command that failed.
pub const FAILED: i32 = 255;

/// Run `system`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "beikKnqsv") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    let command = p.operands.join(" ");
    if command.trim().is_empty() {
        return usage(cx, NAME, "A CL command is needed.");
    }
    let mut out = Out::stdout(cx);
    if p.has('v') {
        out.text(&command);
        out.text("\n");
    }
    let req = ClRequest {
        command,
        same_job: p.has('i'),
        env: if p.has('e') { cx.env.clone() } else { Vec::new() },
        keep_spool: p.has('k'),
        keep_joblog: p.has('K'),
    };
    let o = match msgs::run_cl(cx, &req) {
        Ok(o) => o,
        Err(e) => {
            out.flush();
            complain(cx, NAME, &e);
            return FAILED;
        }
    };
    if !p.has('s') {
        for f in &o.spooled {
            for page in &f.pages {
                for line in page {
                    out.text(line);
                    out.text("\n");
                }
            }
        }
    }
    if !p.has('q') {
        msgs::write(cx, &mut out, &o.messages, !p.has('n'));
    }
    out.finish(cx, NAME, if o.failed { FAILED } else { 0 })
}
