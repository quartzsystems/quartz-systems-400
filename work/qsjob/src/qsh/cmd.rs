// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSH` and `STRQSH`. Without a command: the QSH terminal session at a
//! display (`crate::devtools::qsh`). With one: the shell runs it to its
//! end (`qsh_job::run::batch`) with `QIBM_QSH_CMD_OUTPUT` routing its
//! output: `STDOUT` is the terminal display in an interactive job, the
//! spooled file `QPRINT` in a batch job and the console's text in the
//! service console's job; `FILE=` and `FILEAPPEND=` are files of the
//! IFS; `NONE` drops it. The command ends with `QSH0005` (completion,
//! or escape with `QIBM_QSH_CMD_ESCAPE_MSG=Y` and a status above 0),
//! `QSH0006` or `QSH0007`, as IBM's does.

use std::fmt::Write as _;

use cmd::{Resolved, Value};
use mi::Job;
use qir::MsgType;
use qsh::session::batch::MsgKind;

use stg::Client;

use crate::command::Fail;
use crate::console::host::Host;

/// The `CMD` parameter, quotes off; `None` for `*NONE`.
fn command_of(r: &Resolved) -> Option<String> {
    let v = r
        .get("CMD")
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default();
    let t = v.trim();
    let t = if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        t.to_string()
    };
    (!t.is_empty() && !t.eq_ignore_ascii_case("*NONE")).then_some(t)
}

/// Print lines to a printer file as a spooled file; the message.
pub fn print(host: &Host, file: &str, lines: &[String]) -> Result<String, String> {
    use spool::printer::{Control, PageFormat, Printer};
    let (id, prtf) = host
        .link
        .borrow_mut()
        .with(|l| l.prt_open(file.into(), "*LIBL".into(), "*DFTACTGRP".into()))?;
    let mut p = Printer::new(PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: prtf.fold,
    });
    for l in lines {
        p.write_str(l, Control::line());
    }
    let bytes = p.take();
    host.link
        .borrow_mut()
        .with(|l| l.splf_put(id, bytes, p.pages, p.records))?;
    host.link
        .borrow_mut()
        .with(|l| l.splf_close(id, prtf.hold))?;
    Ok(format!("The output was printed to {file}."))
}

/// `QSH` / `STRQSH`.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Result<String, Fail> {
    let Some(cmd) = command_of(r) else {
        if crate::panels::shows_panels(host, r) {
            return crate::devtools::qsh::session(job, host);
        }
        return Err(Fail::new(
            "QSH0002",
            "QSH0002 The QSH terminal session needs a display; run QSH CMD() here.",
        ));
    };
    let terminal = if crate::panels::shows_panels(host, r) { "5250" } else { "PIPELINE" };
    let setup = super::setup(host, terminal);
    let b = {
        let mut side = super::side::JobSide::new(job, host);
        qsh_job::run::batch(&mut side, setup, &cmd)
    };
    let text = String::from_utf8_lossy(&b.output).into_owned();
    let msg = b.result.message;
    let data = msg.data.to_be_bytes().to_vec();
    let line = match msg.id {
        "QSH0005" => format!("QSH0005 The command finished with exit status {}.", msg.data),
        "QSH0006" => format!("QSH0006 The command was stopped by signal {}.", msg.data),
        _ => "QSH0007 The command was stopped by an exception.".to_string(),
    };
    // Where STDOUT goes.
    let mut out = String::new();
    if !text.is_empty() {
        if crate::panels::shows_panels(host, r) {
            crate::devtools::qsh::show_output(host, &text);
        } else if matches!(host.start.role.as_str(), "BATCH" | "SPAWN") {
            let lines: Vec<String> = text.lines().map(str::to_string).collect();
            if let Err(e) = print(host, "QPRINT", &lines) {
                out.push_str(&text);
                let _ = writeln!(out, "The output could not be spooled: {e}");
            }
        } else {
            out.push_str(&text);
        }
    }
    if msg.kind == MsgKind::Escape {
        return Err(Fail {
            msgid: msg.id.to_string(),
            data,
            text: format!("{out}{line}"),
        });
    }
    crate::command::log(job, msg.id, &data, MsgType::Comp, Some(&line));
    out.push_str(&line);
    out.push('\n');
    Ok(out)
}
