// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Conditions, the call stack, program calls, pointers, copies, text
//! moves, `dsply` and `monitor` matching.

#![allow(clippy::similar_names)]

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use codegen::Cond;
use codegen::abi::Env;

use super::mem::{bytes, env as env_of, job, typed};
use super::string::{read_text, write_text};
use crate::act::Frame;
use crate::job::Job;
use crate::space::{FatPtr, PtrKind};

/// Record a condition on the job: an escape message from `QCPFMSG` to
/// the current entry, dispatched to its handlers (`except`).
pub(crate) fn raise_on(j: &mut Job, cond: Cond) {
    let _ = j.send_message(
        &cond.msgid(),
        "QCPFMSG",
        "QSYS",
        Vec::new(),
        qir::MsgType::Escape,
        crate::msg::QueueRef::Same,
    );
}

/// `raise(env, cond)`.
pub extern "sysv64" fn raise(env: *mut Env, cond: i32) {
    let j = job(env);
    let c = u32::try_from(cond)
        .ok()
        .and_then(Cond::from_code)
        .unwrap_or(Cond::Mch3601);
    raise_on(j, c);
}

/// The attention flag: generated code polls it at statement starts
/// and loop back-edges and calls `attention` when it is set. The job
/// process raises it on a timer so a running program looks for break
/// messages.
pub static ATTN: AtomicU64 = AtomicU64::new(0);

/// Set by the job process's stack guard when a `SIGSEGV` on the stack's
/// guard page was forgiven: the next entry raises `MCH0601`.
static STACK_FAULT: AtomicBool = AtomicBool::new(false);

/// The stack guard forgave a fault on the guard page.
pub fn note_stack_fault() {
    STACK_FAULT.store(true, Ordering::Release);
}

/// `attention(env)`: the flag was set; clear it and run the job's
/// attention hook, which takes the breaks waiting for the job.
pub extern "sysv64" fn attention(env: *mut Env) {
    ATTN.store(0, Ordering::Relaxed);
    let j = job(env);
    if let Some(h) = j.attn.clone() {
        h(j);
    }
}

/// `enter(env, code, argc, opdesc)`: a call-stack entry; the static
/// area becomes the running program's activation in the group its
/// attribute names.
pub extern "sysv64" fn enter(env: *mut Env, code: i64, argc: i64, opdesc: i64) {
    let j = job(env);
    let e = env_of(env);
    j.depth += 1;
    j.max_depth = j.max_depth.max(j.depth);
    let code = code.cast_unsigned();
    let saved_ag = j.act.current;
    let saved_statics = e.statics;
    let program = j.act.program_of(code);
    let mut ag = saved_ag;
    if let Some(p) = &program {
        ag = j.act.group_for(&p.object, saved_ag);
        let (base, len) = j.act.activate(p, ag);
        j.spaces.register_static(base as u64, len as u64);
        if !p.object.attr.user_state {
            j.spaces.register_system(base as u64, len as u64);
        }
        e.statics = base;
        j.act.current = ag;
    }
    let control_boundary = program
        .as_ref()
        .is_some_and(|p| p.entry_addr() == Some(usize::try_from(code).unwrap_or(0)));
    // The caller is suspended in the region it marked before the call.
    let (caller_region, caller_stmt) = (j.abi.cur_region, j.abi.cur_stmt);
    if let Some(f) = j.act.frames.last_mut() {
        f.region = caller_region;
        f.stmt = caller_stmt;
    }
    j.abi.cur_region = 0;
    j.abi.cur_stmt = 0;
    let adopts = program.clone();
    j.act.frames.push(Frame {
        code,
        program,
        ag,
        saved_statics,
        saved_ag,
        msgq: crate::msg::MsgQueue::default(),
        control_boundary,
        argc: u32::try_from(argc).unwrap_or(0),
        opdesc: opdesc.cast_unsigned(),
        region: 0,
        stmt: 0,
        handlers: Vec::new(),
        cancel_handlers: Vec::new(),
        debug_vars: Vec::new(),
    });
    j.act.frame_changed(adopts.as_ref(), true);
    // A fault on the guard page the job process forgave, or the
    // in-memory budget run out: the entry fails with `MCH4429` (stack
    // overflow) as soon as it starts.
    let here = std::ptr::from_ref(&code) as usize;
    let faulted = STACK_FAULT.load(Ordering::Acquire) && STACK_FAULT.swap(false, Ordering::AcqRel);
    if faulted || (j.stack_base != 0 && j.stack_base.saturating_sub(here) > j.stack_bytes) {
        raise_on(j, Cond::Mch4429);
    }
}

/// `leave(env)`: the entry ends; the caller's static area and group
/// come back and the frame's spaces die.
pub extern "sysv64" fn leave(env: *mut Env) {
    let j = job(env);
    if j.abi.pending != 0 {
        crate::except::run_cancel_handlers(j);
    }
    let e = env_of(env);
    if let Some(f) = j.act.frames.pop() {
        e.statics = f.saved_statics;
        j.act.current = f.saved_ag;
        j.act.frame_changed(f.program.as_ref(), false);
    }
    // The caller is back where it marked itself before the call.
    let (region, stmt) = j.act.frames.last().map_or((0, 0), |c| (c.region, c.stmt));
    j.abi.cur_region = region;
    j.abi.cur_stmt = stmt;
    j.depth = j.depth.saturating_sub(1);
    let depth = j.depth;
    j.spaces.leave(depth);
    if j.ovr_top > depth
        && let Some(end) = j.ovr_end.clone()
    {
        end(j);
    }
    if j.resume_depth == Some(depth) {
        crate::except::resume(j);
    }
}

/// `deref(env, lo, hi) -> addr`.
pub extern "sysv64" fn deref(env: *mut Env, lo: i64, hi: i64) -> i64 {
    let j = job(env);
    let p = FatPtr::from_halves(lo.cast_unsigned(), hi.cast_unsigned());
    match j.spaces.deref(p) {
        Ok(a) => a.cast_signed(),
        Err(c) => {
            raise_on(j, c);
            0
        }
    }
}

/// `copy(dst, src, len)`.
pub extern "sysv64" fn copy(dst: i64, src: i64, len: i64) {
    let n = usize::try_from(len).unwrap_or(0);
    let s = bytes(src, n).to_vec();
    bytes(dst, n).copy_from_slice(&s);
}

/// `mov_str(env, dst, dty, src, sty)`.
pub extern "sysv64" fn mov_str(_env: *mut Env, dst: i64, dty: i64, src: i64, sty: i64) {
    let t = read_text(src, sty);
    write_text(dst, dty, &t);
}

fn write_ptr(out: i64, p: FatPtr) {
    bytes(out, 16).copy_from_slice(&p.to_bytes());
}

fn read_ptr(addr: i64) -> FatPtr {
    let b: [u8; 16] = bytes(addr, 16).try_into().expect("16 bytes");
    FatPtr::from_bytes(&b)
}

/// `ptr_addr(env, out, native, len)`.
pub extern "sysv64" fn ptr_addr(env: *mut Env, out: i64, native: i64, len: i64) {
    let j = job(env);
    let depth = j.depth;
    let p = j
        .spaces
        .addr(native.cast_unsigned(), len.cast_unsigned(), depth);
    write_ptr(out, p);
}

/// `ptr_add(env, out, ptr, n)`.
pub extern "sysv64" fn ptr_add(env: *mut Env, out: i64, ptr: i64, n: i64) {
    let j = job(env);
    let p = read_ptr(ptr);
    match j.spaces.add(p, n) {
        Ok(q) => write_ptr(out, q),
        Err(c) => {
            raise_on(j, c);
            write_ptr(out, FatPtr::null());
        }
    }
}

/// `ptr_diff(env, p, q) -> i64`.
pub extern "sysv64" fn ptr_diff(env: *mut Env, p: i64, q: i64) -> i64 {
    let j = job(env);
    match j.spaces.diff(read_ptr(p), read_ptr(q)) {
        Ok(d) => d,
        Err(c) => {
            raise_on(j, c);
            0
        }
    }
}

/// `proc_addr(env, out, code)`.
pub extern "sysv64" fn proc_addr(_env: *mut Env, out: i64, code: i64) {
    write_ptr(out, FatPtr::procedure(code.cast_unsigned()));
}

/// `proc_target(env, ptr) -> code`.
pub extern "sysv64" fn proc_target(env: *mut Env, ptr: i64) -> i64 {
    let p = read_ptr(ptr);
    match p.kind() {
        Some(PtrKind::Procedure) => p.offset.cast_signed(),
        Some(PtrKind::Null) => {
            raise_on(job(env), Cond::Mch3601);
            0
        }
        _ => {
            raise_on(job(env), Cond::Mch3602);
            0
        }
    }
}

/// A `char(10)` name in storage.
fn name_at(addr: i64) -> String {
    ccsid::decode_trimmed(bytes(addr, 10))
}

fn args_at(argc: i64, argv: i64) -> Vec<u64> {
    let n = usize::try_from(argc).unwrap_or(0);
    bytes(argv, 8 * n)
        .chunks(8)
        .map(|c| u64::from_le_bytes(c.try_into().expect("8 bytes")))
        .collect()
}

/// `call_pgm(env, name, lib, argc, argv)`.
pub extern "sysv64" fn call_pgm(env: *mut Env, name: i64, lib: i64, argc: i64, argv: i64) {
    let n = name_at(name);
    let l = name_at(lib);
    let args = args_at(argc, argv);
    let j = job(env);
    if let Err(c) = j.call_program(&n, &l, &args) {
        raise_on(j, c);
    }
}

/// `call_pgm_var(env, addr, len, argc, argv)`.
pub extern "sysv64" fn call_pgm_var(env: *mut Env, addr: i64, len: i64, argc: i64, argv: i64) {
    let n = name_at(addr);
    let l = if len >= 20 {
        name_at(addr + 10)
    } else {
        "*LIBL".into()
    };
    let args = args_at(argc, argv);
    let j = job(env);
    if let Err(c) = j.call_program(&n, &l, &args) {
        raise_on(j, c);
    }
}

/// `call_pgm_ptr(env, ptr, argc, argv)`: system pointers come with the
/// object spaces; until then the call is `MCH3602`.
pub extern "sysv64" fn call_pgm_ptr(env: *mut Env, ptr: i64, argc: i64, argv: i64) {
    let j = job(env);
    let p = read_ptr(ptr);
    match p.kind() {
        Some(PtrKind::System) => {
            let Some((name, lib)) = j.act.source.by_id(p.offset) else {
                raise_on(j, Cond::Mch3401);
                return;
            };
            let args = args_at(argc, argv);
            if let Err(c) = j.call_program(&name, &lib, &args) {
                raise_on(j, c);
            }
        }
        Some(PtrKind::Null) => raise_on(j, Cond::Mch3601),
        _ => raise_on(j, Cond::Mch3602),
    }
}

/// The target of an import the binder left unresolved: `MCH3401`.
pub extern "sysv64" fn unresolved(env: *mut Env) {
    raise_on(job(env), Cond::Mch3401);
}

/// `dsply(env, src, sty, reply, rty)`: the text goes to the job's
/// console; a reply, when asked for, comes from the queued replies or
/// is blank.
pub extern "sysv64" fn dsply(env: *mut Env, src: i64, sty: i64, reply: i64, rty: i64) {
    let line = display_text(src, sty);
    let j = job(env);
    j.console.push(line);
    if reply != 0 {
        let r = j.replies.pop_front().unwrap_or_default();
        let units: Vec<u16> = r
            .chars()
            .map(|c| ccsid::byte_of(c).map_or(0x6F, u16::from))
            .collect();
        write_text(reply, rty, &super::string::Text { units, wide: false });
    }
}

/// A storage operand as display text: text decoded from CP037, a
/// number rendered, anything else in hexadecimal.
pub(crate) fn display_text(addr: i64, desc: i64) -> String {
    let (b, t) = typed(addr, desc);
    if t.is_string() || t.class() == qir::TypeClass::DateTime {
        let txt = read_text(addr, desc);
        return if txt.wide {
            String::from_utf16_lossy(&txt.units)
        } else {
            let raw: Vec<u8> = txt
                .units
                .iter()
                .map(|u| u8::try_from(*u).unwrap_or(0x6F))
                .collect();
            ccsid::decode_trimmed(&raw)
        };
    }
    if t.is_numeric() {
        return match super::decimal::read_num(addr, desc) {
            Ok(v) => v.value.to_string(),
            Err(_) => "*ERROR".into(),
        };
    }
    if t == qir::Type::Ind {
        return if b[0] == 0xF1 { "1".into() } else { "0".into() };
    }
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// `monitor_match(env, kind, a, b) -> 0|1`.
pub extern "sysv64" fn monitor_match(env: *mut Env, kind: i64, a: i64, b: i64) -> i32 {
    let j = job(env);
    let Some(e) = j.exception.as_ref() else {
        return 0;
    };
    let hit = e.matches(kind, a, b);
    // A handler takes the exception only at the entry it was sent to
    // or at one of its callers, never in the sender below the target.
    if hit && j.depth <= e.depth {
        let e = j.exception.take().expect("present");
        j.handled.push(e);
        j.abi.pending = 0;
        1
    } else {
        0
    }
}
