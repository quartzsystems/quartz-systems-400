// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console commands that act on the job itself: calls, activation
//! groups, the call stack, message queues, replies, dumps and the
//! debugger outside a stop.

use std::fmt::Write as _;

use mi::QueueRef;
use wm::cmd::Params;

use super::Console;

/// Run `cmd`; its output.
pub fn command(c: &mut Console, cmd: &str, line: &str) -> String {
    let p = Params::parse(line);
    let r = match cmd {
        "call" => super::call::call(c, line),
        "dspactgrp" => Ok(c.job().actgrp_text()),
        "rclactgrp" => rclactgrp(c, &p),
        "dspcallstk" => Ok(c.job().call_stack_text()),
        "dspmsgq" => Ok(dspmsgq(c, &p)),
        "rpy" => rpy(c, &p),
        "dmp" => {
            let j = c.job();
            Ok(j.last_dump.clone().unwrap_or_else(|| j.dump(true)))
        }
        "strdbg" => strdbg(c, &p),
        "enddbg" => {
            c.job().enddbg();
            Ok("Debug ended.\n".to_string())
        }
        "addbkp" => addbkp(c, &p, true),
        "rmvbkp" => addbkp(c, &p, false),
        "dspbkp" => Ok(dspbkp(c)),
        "dspjoblog" => Ok(c.dspjoblog()),
        other => Err(format!("{other} is not a job command")),
    };
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

/// `rclactgrp NAME|*ELIGIBLE`.
fn rclactgrp(c: &mut Console, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "ACTGRP")?.to_ascii_uppercase();
    let j = c.job();
    if name == "*ELIGIBLE" {
        j.rclactgrp_eligible();
        return Ok("Eligible activation groups reclaimed.\n".to_string());
    }
    Ok(match j.rclactgrp(&name) {
        Ok(()) => format!("Activation group {name} reclaimed.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `dspmsgq [*PGM|*EXT|QSYSOPR]`.
fn dspmsgq(c: &mut Console, p: &Params) -> String {
    let which = p
        .pos
        .first()
        .map_or_else(|| "*EXT".to_string(), |s| s.to_ascii_uppercase());
    let q = match which.as_str() {
        "*PGM" | "*SAME" => QueueRef::Same,
        "QSYSOPR" | "*SYSOPR" => QueueRef::SysOpr,
        _ => QueueRef::Ext,
    };
    c.job().msgq_text(q)
}

/// `rpy KEY REPLY`: answer an inquiry on the external queue, or queue
/// the reply for the next one.
fn rpy(c: &mut Console, p: &Params) -> Result<String, String> {
    let (key, reply) = match p.pos.as_slice() {
        [k, r] => (k.clone(), r.to_ascii_uppercase()),
        [r] => (String::new(), r.to_ascii_uppercase()),
        _ => return Err("rpy [KEY] REPLY".into()),
    };
    let j = c.job();
    let k = u32::from_str_radix(&key, 16).ok();
    if let Some(k) = k
        && let Some(m) = j.ext_msgq.msgs.iter_mut().find(|m| m.key == k)
    {
        m.reply = Some(reply.clone());
        return Ok(format!("Reply {reply} recorded for message {key}.\n"));
    }
    j.inquiry_replies.push_back(reply.clone());
    Ok(format!("Reply {reply} queued for the next inquiry.\n"))
}

/// `strdbg LIB/PGM`.
fn strdbg(c: &mut Console, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual.as_str()), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    Ok(match c.job().strdbg(&name, &lib) {
        Ok(key) => format!("{key} is under debug; its next call runs the debug view.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `addbkp N [PGM(LIB/PGM)]` and `rmvbkp N|*ALL [PGM(..)]`.
fn addbkp(c: &mut Console, p: &Params, add: bool) -> Result<String, String> {
    let n = p.pos_or(0, "STMT")?;
    let pgm = p.upper("PGM");
    let j = c.job();
    let program = pgm.unwrap_or_else(|| {
        j.debug
            .as_ref()
            .and_then(|d| d.programs.last().cloned())
            .unwrap_or_default()
    });
    if program.is_empty() {
        return Ok("No program is under debug; strdbg first or give PGM().\n".into());
    }
    if add {
        return Ok(match n.parse::<u32>() {
            Ok(stmt) => {
                j.addbkp(&program, stmt);
                format!("Breakpoint at {program} statement {stmt}.\n")
            }
            Err(_) => "addbkp N\n".into(),
        });
    }
    let gone = match n.parse::<u32>() {
        Ok(stmt) => j.rmvbkp(&program, Some(stmt)),
        Err(_) => j.rmvbkp(&program, None),
    };
    Ok(format!("{gone} breakpoint(s) removed.\n"))
}

/// `dspbkp`.
fn dspbkp(c: &mut Console) -> String {
    let j = c.job();
    let mut out = String::new();
    if let Some(d) = &j.debug {
        for (pgm, n) in &d.breakpoints {
            let _ = writeln!(out, "  {pgm:<21} statement {n}");
        }
        for pgm in &d.programs {
            let _ = writeln!(out, "  {pgm} under debug");
        }
    }
    if out.is_empty() {
        "No breakpoints.\n".into()
    } else {
        out
    }
}

/// `dtaara get *LDA` and `dtaara set *LDA TEXT`: the job's own area.
pub fn lda(c: &mut Console, args: &[&str]) -> String {
    match args {
        ["get", _] => {
            let text = ccsid::from_ebcdic(&c.lda.borrow());
            format!("*LDA: '{}'\n", text.trim_end())
        }
        ["set", _, rest @ ..] => {
            let text = rest.join(" ");
            let Ok(mut b) = ccsid::to_ebcdic(&text) else {
                return format!("{text}: not CP037\n");
            };
            b.resize(stg::remote::LDA_LEN, 0x40);
            *c.lda.borrow_mut() = b;
            "*LDA changed.\n".into()
        }
        _ => "Usage: dtaara get *LDA | set *LDA TEXT\n".into(),
    }
}
