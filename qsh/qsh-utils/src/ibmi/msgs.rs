// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing what a CL command sent back: completion, informational and
//! notify messages to standard output, diagnostic and escape messages
//! to standard error, each `ID: text` (or the text alone).

use qsh::sys::{ClMessage, ClOutcome, ClRequest, MsgKind, UtilCtx};

use crate::common::io::Out;

/// One message as `system` prints it.
pub fn text(m: &ClMessage, ids: bool) -> String {
    if ids && !m.id.is_empty() {
        format!("{}: {}\n", m.id, m.text)
    } else {
        format!("{}\n", m.text)
    }
}

/// Does a message go to standard error?
pub fn is_error(kind: MsgKind) -> bool {
    matches!(kind, MsgKind::Diagnostic | MsgKind::Escape)
}

/// Write the messages: errors straight to standard error, the rest
/// through `out`.
pub fn write(cx: &UtilCtx<'_>, out: &mut Out, msgs: &[ClMessage], ids: bool) {
    for m in msgs {
        if is_error(m.kind) {
            out.flush();
            let _ = cx.err(&text(m, ids));
        } else {
            out.text(&text(m, ids));
        }
    }
}

/// Run a command through the system's CL runner.
pub fn run_cl(cx: &UtilCtx<'_>, req: &ClRequest) -> Result<ClOutcome, String> {
    let runner = cx
        .sys
        .cl_runner()
        .map_err(|e| format!("CL commands cannot be run: {}", e.errno.text()))?;
    runner
        .run_cl(req)
        .map_err(|e| format!("The command could not be run: {}", e.errno.text()))
}

/// Run a command and write its error messages; `true` when it worked.
pub fn run_quiet(cx: &UtilCtx<'_>, name: &str, command: &str) -> bool {
    let req = ClRequest {
        command: command.to_string(),
        same_job: true,
        ..ClRequest::default()
    };
    match run_cl(cx, &req) {
        Ok(o) => {
            for m in o.messages.iter().filter(|m| is_error(m.kind)) {
                let _ = cx.err(&format!("{name}: {}", text(m, true)));
            }
            !o.failed
        }
        Err(e) => {
            crate::common::complain(cx, name, &e);
            false
        }
    }
}

/// A CL string literal: quoted, quotes doubled.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
