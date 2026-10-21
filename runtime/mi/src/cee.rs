// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The condition-management APIs of `QLEAWI` as bound procedures: the
//! loader resolves a `*SYSTEM` import to one of these. Every parameter
//! is by reference, so the arguments are storage addresses; the
//! feedback code, when passed, is a 12-byte condition token and
//! `CEE0000` (all zeros) means success. An error with the feedback
//! code omitted is signaled to the caller instead.

use codegen::abi::Env;

use crate::cond::Exception;
use crate::except::{decode_token, encode_token};
use crate::job::Job;
use crate::rtlib::mem::{bytes, job};
use crate::space::{FatPtr, PtrKind};

/// The procedures this module provides, as the binder lists them.
pub const NAMES: &[&str] = &[
    "CEEHDLR", "CEEHDLU", "CEERTX", "CEEUTX", "CEESGL", "CEENCOD", "CEEDCOD", "CEEMRCR",
];

/// The code address of a bound API by name.
#[must_use]
pub fn bound_api(name: &str) -> Option<usize> {
    let f: *const () = match name {
        "CEEHDLR" => ceehdlr as *const (),
        "CEEHDLU" => ceehdlu as *const (),
        "CEERTX" => ceertx as *const (),
        "CEEUTX" => ceeutx as *const (),
        "CEESGL" => ceesgl as *const (),
        "CEENCOD" => ceencod as *const (),
        "CEEDCOD" => ceedcod as *const (),
        "CEEMRCR" => ceemrcr as *const (),
        _ => return None,
    };
    Some(f as usize)
}

/// Success into the feedback code, when there is one.
fn fc_ok(fc: i64) {
    if fc != 0 {
        bytes(fc, 12).fill(0);
    }
}

/// An error: into the feedback code, or signaled to the caller.
fn fc_err(j: &mut Job, fc: i64, msgid: &str, data: Vec<u8>) {
    if fc == 0 {
        let _ = j.send_message(
            msgid,
            "QCEEMSG",
            "QSYS",
            data,
            qir::MsgType::Escape,
            crate::msg::QueueRef::Same,
        );
        return;
    }
    let mut e = Exception::message(msgid, "QCEEMSG", data, j.depth);
    e.key = 0;
    let t = encode_token(j, &e);
    bytes(fc, 12).copy_from_slice(&t);
}

fn api_name(s: &str) -> Vec<u8> {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
    b.resize(7, 0x40);
    b
}

/// The code address a procedure pointer in storage names.
fn proc_target(addr: i64) -> Option<u64> {
    let b: [u8; 16] = bytes(addr, 16).try_into().ok()?;
    let p = FatPtr::from_bytes(&b);
    (p.kind() == Some(PtrKind::Procedure)).then_some(p.offset)
}

fn register(env: *mut Env, pproc: i64, ptoken: i64, fc: i64, cancel: bool, api: &str) {
    let j = job(env);
    let Some(code) = proc_target(pproc) else {
        fc_err(j, fc, "CEE0801", api_name(api));
        return;
    };
    if let Some(f) = j.act.frames.last_mut() {
        let list = if cancel {
            &mut f.cancel_handlers
        } else {
            &mut f.handlers
        };
        list.push((code, ptoken.cast_unsigned()));
    }
    fc_ok(fc);
}

fn unregister(env: *mut Env, pproc: i64, fc: i64, cancel: bool, api: &str) {
    let j = job(env);
    let Some(code) = proc_target(pproc) else {
        fc_err(j, fc, "CEE0801", api_name(api));
        return;
    };
    let removed = j.act.frames.last_mut().is_some_and(|f| {
        let list = if cancel {
            &mut f.cancel_handlers
        } else {
            &mut f.handlers
        };
        match list.iter().rposition(|(c, _)| *c == code) {
            Some(i) => {
                list.remove(i);
                true
            }
            None => false,
        }
    });
    if removed {
        fc_ok(fc);
    } else {
        fc_err(j, fc, "CEE0809", api_name(api));
    }
}

/// `CEEHDLR(proc_ptr, token, fc)`: register a condition handler for the
/// calling entry.
pub extern "sysv64" fn ceehdlr(env: *mut Env, argc: i64, _od: i64, p1: i64, p2: i64, p3: i64) {
    let fc = if argc >= 3 { p3 } else { 0 };
    register(env, p1, p2, fc, false, "CEEHDLR");
}

/// `CEEHDLU(proc_ptr, fc)`: unregister a condition handler.
pub extern "sysv64" fn ceehdlu(env: *mut Env, argc: i64, _od: i64, p1: i64, p2: i64) {
    let fc = if argc >= 2 { p2 } else { 0 };
    unregister(env, p1, fc, false, "CEEHDLU");
}

/// `CEERTX(proc_ptr, token, fc)`: register a cancel handler.
pub extern "sysv64" fn ceertx(env: *mut Env, argc: i64, _od: i64, p1: i64, p2: i64, p3: i64) {
    let fc = if argc >= 3 { p3 } else { 0 };
    register(env, p1, p2, fc, true, "CEERTX");
}

/// `CEEUTX(proc_ptr, fc)`: unregister a cancel handler.
pub extern "sysv64" fn ceeutx(env: *mut Env, argc: i64, _od: i64, p1: i64, p2: i64) {
    let fc = if argc >= 2 { p2 } else { 0 };
    unregister(env, p1, fc, true, "CEEUTX");
}

/// The message file a facility's messages live in.
fn file_of(msgid: &str) -> &'static str {
    match msgid.get(..3) {
        Some("CEE") => "QCEEMSG",
        Some("SQL") => "QSQLMSG",
        _ => "QCPFMSG",
    }
}

/// `CEESGL(cond_token, q_data_token, fc)`: signal the condition at the
/// calling entry as an escape message.
pub extern "sysv64" fn ceesgl(env: *mut Env, argc: i64, _od: i64, p1: i64, _p2: i64, p3: i64) {
    let fc = if argc >= 3 { p3 } else { 0 };
    let j = job(env);
    let Some(msgid) = decode_token(bytes(p1, 12)) else {
        fc_ok(fc);
        return;
    };
    fc_ok(fc);
    let _ = j.send_message(
        &msgid,
        file_of(&msgid),
        "QSYS",
        Vec::new(),
        qir::MsgType::Escape,
        crate::msg::QueueRef::Same,
    );
}

fn i16_at(addr: i64) -> i16 {
    let b: [u8; 2] = bytes(addr, 2).try_into().expect("2 bytes");
    i16::from_be_bytes(b)
}

/// `CEENCOD(c_1, c_2, case, severity, control, facility, isi, cond_token,
/// fc)`: build a condition token from its parts.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn ceencod(
    _env: *mut Env,
    argc: i64,
    _od: i64,
    p1: i64,
    p2: i64,
    p3: i64,
    p4: i64,
    p5: i64,
    p6: i64,
    p7: i64,
    p8: i64,
    p9: i64,
) {
    let fc = if argc >= 9 { p9 } else { 0 };
    let mut t = [0u8; 12];
    t[0..2].copy_from_slice(&i16_at(p1).to_be_bytes());
    t[2..4].copy_from_slice(&i16_at(p2).to_be_bytes());
    let case = u8::try_from(i16_at(p3)).unwrap_or(0) & 3;
    let sev = u8::try_from(i16_at(p4)).unwrap_or(0) & 7;
    let ctl = u8::try_from(i16_at(p5)).unwrap_or(0) & 7;
    t[4] = (case << 6) | (sev << 3) | ctl;
    t[5..8].copy_from_slice(bytes(p6, 3));
    t[8..12].copy_from_slice(bytes(p7, 4));
    bytes(p8, 12).copy_from_slice(&t);
    fc_ok(fc);
}

/// `CEEDCOD(cond_token, c_1, c_2, case, severity, control, facility,
/// isi, fc)`: take a condition token apart.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn ceedcod(
    _env: *mut Env,
    argc: i64,
    _od: i64,
    p1: i64,
    p2: i64,
    p3: i64,
    p4: i64,
    p5: i64,
    p6: i64,
    p7: i64,
    p8: i64,
    p9: i64,
) {
    let fc = if argc >= 9 { p9 } else { 0 };
    let t = bytes(p1, 12).to_vec();
    bytes(p2, 2).copy_from_slice(&t[0..2]);
    bytes(p3, 2).copy_from_slice(&t[2..4]);
    bytes(p4, 2).copy_from_slice(&i16::from(t[4] >> 6).to_be_bytes());
    bytes(p5, 2).copy_from_slice(&i16::from((t[4] >> 3) & 7).to_be_bytes());
    bytes(p6, 2).copy_from_slice(&i16::from(t[4] & 7).to_be_bytes());
    bytes(p7, 3).copy_from_slice(&t[5..8]);
    bytes(p8, 4).copy_from_slice(&t[8..12]);
    fc_ok(fc);
}

/// `CEEMRCR(action, fc)`: move the resume cursor. Only action 0, the
/// instruction after the failing one, exists here.
pub extern "sysv64" fn ceemrcr(env: *mut Env, argc: i64, _od: i64, p1: i64, p2: i64) {
    let fc = if argc >= 2 { p2 } else { 0 };
    let b: [u8; 4] = bytes(p1, 4).try_into().expect("4 bytes");
    let action = i32::from_be_bytes(b);
    if action == 0 {
        fc_ok(fc);
    } else {
        fc_err(job(env), fc, "CEE0810", b.to_vec());
    }
}
