// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exception model. An escape message sent to a call-stack entry
//! is dispatched here while every frame is still intact: the search
//! runs from that entry towards its control boundary, asking each
//! entry's ILE condition handlers first and then the `monitor`
//! regions it is in. A hit leaves the exception pending for the
//! percolation that follows (`ABI.md` §7), or resumes it. Nothing up
//! to the boundary brings the function check `CPF9999`, and an
//! unhandled function check the inquiry `CEE9901`, whose reply cancels
//! the request or resumes it (`ILE.md` §5).

#![allow(clippy::many_single_char_names)]

use qir::MsgType;

use crate::act::{Frame, call_handler};
use crate::cond::Exception;
use crate::job::{Job, QueueTarget};

/// Result codes a condition handler (`CEEHDLR`) returns.
pub mod result {
    /// Resume at the instruction after the failing one.
    pub const RESUME: i32 = 10;
    /// Percolate to the next handler.
    pub const PERCOLATE: i32 = 20;
    /// Percolate, skipping the entry's other handlers.
    pub const PERCOLATE_SKIP: i32 = 21;
    /// Promote: the new condition token replaces the condition and
    /// the search goes on as if it had been raised at the handler's
    /// level.
    pub const PROMOTE: i32 = 30;
    /// Promote, skipping the entry's other handlers.
    pub const PROMOTE_SKIP: i32 = 31;
}

/// What the handler search found.
enum Found {
    /// A `monitor` will take it as the stack percolates.
    Pending,
    /// A condition handler resumed.
    Resumed,
    /// A handler failed with its own exception, dispatched already.
    Replaced,
    /// Nothing up to the control boundary, by frame index.
    Unhandled(Option<usize>),
}

/// What one condition handler decided.
enum Verdict {
    Resume,
    Percolate,
    Skip,
    Replaced,
    /// A new condition takes the place of the old; `true` skips the
    /// entry's other handlers.
    Promote(Exception, bool),
}

/// Dispatch an exception sent to the entry at `e.depth`.
pub(crate) fn dispatch(j: &mut Job, e: &Exception) {
    j.exception = Some(e.clone());
    j.abi.pending = 1;
    if j.act.frames.is_empty() {
        return;
    }
    match search(j, e) {
        Found::Pending | Found::Replaced => {}
        Found::Resumed => resume(j),
        Found::Unhandled(boundary) => {
            // A handler may have promoted the condition on the way.
            let cur = j.exception.clone().unwrap_or_else(|| e.clone());
            unhandled(j, &cur, boundary);
        }
    }
}

/// Clear the pending exception as handled.
pub(crate) fn resume(j: &mut Job) {
    if let Some(e) = j.exception.take() {
        j.handled.push(e);
    }
    j.abi.pending = 0;
    j.resume_depth = None;
}

/// Resume the entry the exception was sent to: at the failing
/// instruction when it is the running one, otherwise after its call,
/// once the entries above it have ended.
fn resume_target(j: &mut Job, e: &Exception) {
    let top = u32::try_from(j.act.frames.len()).unwrap_or(0);
    if e.depth >= top {
        resume(j);
    } else {
        j.resume_depth = Some(e.depth);
    }
}

fn search(j: &mut Job, e: &Exception) -> Found {
    let top = j.act.frames.len();
    let start = (e.depth as usize).clamp(1, top) - 1;
    // The condition under search: a promoting handler replaces it.
    let mut cur = e.clone();
    for i in (0..=start).rev() {
        let handlers = j.act.frames[i].handlers.clone();
        for (proc_addr, token) in handlers.iter().rev() {
            match call_cond_handler(j, *proc_addr, *token, &cur) {
                Verdict::Resume => return Found::Resumed,
                Verdict::Replaced => return Found::Replaced,
                Verdict::Skip => break,
                Verdict::Percolate => {}
                Verdict::Promote(mut new, skip) => {
                    new.depth = u32::try_from(i + 1).unwrap_or(u32::MAX);
                    j.exception = Some(new.clone());
                    cur = new;
                    if skip {
                        break;
                    }
                }
            }
        }
        let f = &j.act.frames[i];
        let region = if i + 1 == top {
            j.abi.cur_region
        } else {
            f.region
        };
        if region_handles(f, region, &cur) {
            return Found::Pending;
        }
        if f.control_boundary {
            return Found::Unhandled(Some(i));
        }
    }
    Found::Unhandled(None)
}

/// Whether a `monitor` region of the frame's procedure, or one
/// enclosing it, has a handler for the exception.
fn region_handles(f: &Frame, region: u32, e: &Exception) -> bool {
    let Some(p) = &f.program else {
        return false;
    };
    let Some((mi, _)) = p.proc_of(f.code) else {
        return false;
    };
    let code = &p.object.modules[mi as usize].module.code;
    let mut r = region;
    while r != 0 {
        let Some(d) = code.regions.get(r as usize - 1) else {
            return false;
        };
        if d.handlers.iter().any(|(k, a, b)| e.matches(*k, *a, *b)) {
            return true;
        }
        r = d.parent;
    }
    false
}

/// Call a condition handler with the condition token, its own token,
/// a result code and a new-condition token, the pending flag clear
/// while it runs.
fn call_cond_handler(j: &mut Job, proc_addr: u64, token: u64, e: &Exception) -> Verdict {
    let mut tok = encode_token(j, e);
    let mut rc = [0u8; 4];
    let mut newtok = [0u8; 12];
    let saved = j.exception.take();
    j.abi.pending = 0;
    let env = j.env();
    crate::act::escape(j);
    call_handler(
        usize::try_from(proc_addr).unwrap_or(0),
        env,
        [
            tok.as_mut_ptr() as u64,
            token,
            rc.as_mut_ptr() as u64,
            newtok.as_mut_ptr() as u64,
        ],
        4,
    );
    if j.abi.pending != 0 {
        return Verdict::Replaced;
    }
    j.exception = saved;
    j.abi.pending = 1;
    let rc = i32::from_be_bytes(rc);
    if matches!(rc, result::PROMOTE | result::PROMOTE_SKIP)
        && let Some(msgid) = decode_token(&newtok)
    {
        let new = Exception::message(&msgid, crate::cee::file_of(&msgid), Vec::new(), e.depth);
        return Verdict::Promote(new, rc == result::PROMOTE_SKIP);
    }
    match rc {
        result::RESUME => Verdict::Resume,
        result::PERCOLATE_SKIP | result::PROMOTE_SKIP => Verdict::Skip,
        _ => Verdict::Percolate,
    }
}

/// Run the cancel handlers (`CEERTX`) of the entry being left with an
/// exception pending, newest first.
pub(crate) fn run_cancel_handlers(j: &mut Job) {
    let Some(f) = j.act.frames.last() else {
        return;
    };
    let hs = f.cancel_handlers.clone();
    if hs.is_empty() {
        return;
    }
    let saved = j.exception.take();
    j.abi.pending = 0;
    for (proc_addr, token) in hs.iter().rev() {
        let env = j.env();
        crate::act::escape(j);
        call_handler(
            usize::try_from(*proc_addr).unwrap_or(0),
            env,
            [*token, 0, 0, 0],
            1,
        );
        if j.abi.pending != 0 {
            return;
        }
    }
    j.exception = saved;
    j.abi.pending = 1;
}

/// The program name of a frame, blank when unknown.
fn program_name(f: &Frame) -> String {
    f.program
        .as_ref()
        .map(|p| p.object.name.clone())
        .unwrap_or_default()
}

fn fixed(s: &str, n: usize) -> Vec<u8> {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
    b.resize(n, 0x40);
    b
}

/// No handler up to the control boundary: the function check, then
/// the inquiry and its reply.
fn unhandled(j: &mut Job, e: &Exception, boundary: Option<usize>) {
    let top = j.act.frames.len();
    let failing = (e.depth as usize).clamp(1, top) - 1;
    if e.msgid != "CPF9999" {
        let mut data = fixed(&e.msgid, 7);
        data.extend(fixed(&program_name(&j.act.frames[failing]), 10));
        j.send_message_to(
            QueueTarget::Frame(failing),
            "CPF9999",
            "QCPFMSG",
            "QSYS",
            data,
            MsgType::Escape,
        );
        return;
    }
    let original = ccsid::decode_trimmed(e.data.get(..7).unwrap_or(&e.data));
    let bi = boundary.unwrap_or(0);
    let pgm = program_name(&j.act.frames[bi]);
    let reply = inquire(j, &pgm, &original);
    match reply.as_str() {
        "G" | "I" | "R" => resume_target(j, e),
        "D" => {
            let d = j.dump(true);
            let _ = j.spool_text("QPSRVDMP", &d);
            j.last_dump = Some(d);
            cancel(j, &pgm, &original, boundary);
        }
        "F" => {
            let d = j.dump(false);
            let _ = j.spool_text("QPSRVDMP", &d);
            j.last_dump = Some(d);
            cancel(j, &pgm, &original, boundary);
        }
        _ => cancel(j, &pgm, &original, boundary),
    }
}

/// Send `CEE9901` as an inquiry to the external queue and answer it
/// from the queued replies, or with the description's default.
fn inquire(j: &mut Job, pgm: &str, original: &str) -> String {
    let mut data = fixed(pgm, 10);
    data.extend(fixed(original, 7));
    let key = j.send_message_to(
        QueueTarget::Ext,
        "CEE9901",
        "QCEEMSG",
        "QSYS",
        data.clone(),
        MsgType::Inq,
    );
    let desc = j.msgs.describe("CEE9901", "QCEEMSG", "QSYS");
    let mut reply = None;
    while let Some(r) = j.inquiry_replies.pop_front() {
        let r = r.trim().to_ascii_uppercase();
        if desc.as_ref().is_none_or(|d| d.reply_ok(&r)) {
            reply = Some(r);
            break;
        }
    }
    // No reply queued: the job's inquiry handler (its INQMSGRPY).
    if reply.is_none()
        && let Some(mut ask) = j.inquiry.take()
    {
        let (text, help) = desc.as_ref().map_or_else(
            || (String::new(), String::new()),
            |d| (d.format(&data), d.format_help(&data)),
        );
        let q = crate::job::Inquiry {
            msgid: "CEE9901".into(),
            msgf: "QCEEMSG".into(),
            data: data.clone(),
            text,
            help,
            dft: desc.as_ref().and_then(|d| d.dft_reply.clone()),
            valid: desc
                .as_ref()
                .map(|d| d.valid_replies.clone())
                .unwrap_or_default(),
        };
        reply = ask(&q)
            .map(|r| r.trim().to_ascii_uppercase())
            .filter(|r| desc.as_ref().is_none_or(|d| d.reply_ok(r)));
        j.inquiry = Some(ask);
    }
    let reply = reply
        .or_else(|| desc.and_then(|d| d.dft_reply))
        .unwrap_or_else(|| "C".into());
    if let Some(m) = j.ext_msgq.msgs.iter_mut().find(|m| m.key == key) {
        m.reply = Some(reply.clone());
    }
    j.inquiry_log.push(reply.clone());
    reply
}

/// Cancel the request: the entries from the failing one down to the
/// control boundary end as the exception percolates, and the
/// boundary's caller receives `CEE9901` as an escape; with no caller
/// it goes to the external queue and stays pending for the job.
fn cancel(j: &mut Job, pgm: &str, original: &str, boundary: Option<usize>) {
    let mut data = fixed(pgm, 10);
    data.extend(fixed(original, 7));
    cancel_with(j, "CEE9901", "QCEEMSG", data, boundary);
}

/// Cancel to a boundary with a given escape for its caller.
fn cancel_with(j: &mut Job, msgid: &str, msgf: &str, data: Vec<u8>, boundary: Option<usize>) {
    j.exception = None;
    j.abi.pending = 0;
    match boundary {
        Some(b) if b > 0 => {
            j.send_message_to(
                QueueTarget::Frame(b - 1),
                msgid,
                msgf,
                "QSYS",
                data,
                MsgType::Escape,
            );
        }
        // The first entry's caller is the job: the escape to the
        // external queue leaves the exception pending.
        _ => {
            j.send_message_to(
                QueueTarget::Ext,
                msgid,
                msgf,
                "QSYS",
                data,
                MsgType::Escape,
            );
        }
    }
}

/// Cancel from the running entry down to its control boundary.
pub(crate) fn cancel_to_boundary(j: &mut Job, msgid: &str, msgf: &str, data: Vec<u8>) {
    let boundary = j.act.frames.iter().rposition(|f| f.control_boundary);
    cancel_with(j, msgid, msgf, data, boundary);
}

/// The 12-byte condition token of an exception: severity and message
/// number, case and severity bits, the facility and the message key.
#[must_use]
pub fn encode_token(j: &mut Job, e: &Exception) -> [u8; 12] {
    let sev = match j.severity_of(e) {
        0..=9 => 0u16,
        10..=29 => 1,
        30..=39 => 2,
        40..=49 => 3,
        _ => 4,
    };
    let num = u16::from_str_radix(e.msgid.get(3..).unwrap_or("0"), 16).unwrap_or(0);
    let mut t = [0u8; 12];
    t[0..2].copy_from_slice(&sev.to_be_bytes());
    t[2..4].copy_from_slice(&num.to_be_bytes());
    t[4] = (1 << 6) | (u8::try_from(sev).unwrap_or(0) << 3);
    t[5..8].copy_from_slice(&fixed(e.msgid.get(..3).unwrap_or(""), 3));
    t[8..12].copy_from_slice(&e.key.to_be_bytes());
    t
}

/// The message identifier a condition token names, or none for the
/// all-zero `CEE0000`.
#[must_use]
pub fn decode_token(t: &[u8]) -> Option<String> {
    if t.len() < 12 || t.iter().all(|b| *b == 0) {
        return None;
    }
    let num = u16::from_be_bytes([t[2], t[3]]);
    let fac = ccsid::decode_trimmed(&t[5..8]);
    Some(format!("{fac}{num:04X}"))
}
