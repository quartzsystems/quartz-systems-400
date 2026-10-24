// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job-level operations: `COMMIT` and `ROLBK`, `DUMP`, and the heap
//! operations with RPG's statuses.

use codegen::abi::Env;

fn job(env: *mut Env) -> &'static mut mi::Job {
    mi::storage::job_of(env)
}

/// `_QRNX_COMMIT(kind)`: 1 commits, 2 rolls back. Without commitment
/// control it is status 802.
pub extern "sysv64" fn commit(env: *mut Env, _argc: i64, _od: i64, _kind: i32) {
    let j = job(env);
    if !j.cmtctl {
        let _ = j.send_message("RNX0802", "QRNXMSG", "*LIBL", vec![0x40; 40], qir::MsgType::Escape, mi::QueueRef::Same);
    }
}

/// `_QRNX_DUMP()`: `DUMP` under `DEBUG(*YES)`.
// BLOCKED(phase9): no formatted dump is written; pub400's holds the job
// and the dates, so no corpus member can compare one.
pub extern "sysv64" fn dump(_env: *mut Env, _argc: i64, _od: i64) {}

/// Send the RPG runtime's escape for a status.
fn signal(j: &mut mi::Job, status: u16) {
    let id = format!("RNX{status:04}");
    let _ = j.send_message(&id, "QRNXMSG", "*LIBL", vec![0x40; 40], qir::MsgType::Escape, mi::QueueRef::Same);
}

fn write_ptr(addr: i64, p: mi::FatPtr) {
    crate::value::bytes(addr.cast_unsigned(), 16).copy_from_slice(&p.to_bytes());
}

fn read_ptr(addr: i64) -> mi::FatPtr {
    let b: [u8; 16] = crate::value::bytes(addr.cast_unsigned(), 16).try_into().unwrap_or([0; 16]);
    mi::FatPtr::from_bytes(&b)
}

/// `_QRNX_ALLOC(ptr, n)`: `ALLOC` and `%ALLOC`. A length below 1, or
/// one the heap cannot give, is status 425.
pub extern "sysv64" fn alloc(env: *mut Env, _argc: i64, _od: i64, out: i64, n: i64) {
    let j = job(env);
    if let Some(Ok(p)) = u64::try_from(n).ok().filter(|n| *n > 0).map(|n| j.alloc(n)) {
        write_ptr(out, p);
    } else {
        write_ptr(out, mi::FatPtr::null());
        signal(j, 425);
    }
}

/// `_QRNX_REALLOC(ptr, n)`: `REALLOC` and `%REALLOC`, the pointer
/// replaced. A bad length is status 425, a pointer the heap did not
/// give 426.
pub extern "sysv64" fn realloc(env: *mut Env, _argc: i64, _od: i64, ptr: i64, n: i64) {
    let j = job(env);
    let Some(n) = u64::try_from(n).ok().filter(|n| *n > 0) else {
        signal(j, 425);
        return;
    };
    match j.realloc(read_ptr(ptr), n) {
        Ok(p) => write_ptr(ptr, p),
        Err(_) => signal(j, 426),
    }
}

/// `_QRNX_DTALCK(name, lib)`: before `OUT`, the area must be locked by
/// this job (`IN *LOCK`): status 412 otherwise.
pub extern "sysv64" fn dta_locked(env: *mut Env, _argc: i64, _od: i64, name: i64, lib: i64) {
    let j = job(env);
    let n = ccsid::decode_trimmed(crate::value::bytes(name.cast_unsigned(), 10));
    let l = ccsid::decode_trimmed(crate::value::bytes(lib.cast_unsigned(), 10));
    if j.data.as_mut().and_then(|d| d.dtaara_locked(&n, &l)) == Some(false) {
        signal(j, 412);
    }
}

/// `_QRNX_DATAPGM()`: `DATA-INTO` and `DATA-GEN` cannot call their
/// parser or generator: status 355.
pub extern "sysv64" fn data_pgm(env: *mut Env, _argc: i64, _od: i64) {
    signal(job(env), 355);
}

/// `_QRNX_SIGNAL(status)`: the escape for an RPG status the compiled
/// code found.
pub extern "sysv64" fn signal_status(env: *mut Env, _argc: i64, _od: i64, status: i32) {
    signal(job(env), u16::try_from(status).unwrap_or(9999));
}

/// `_QRNX_SPECIAL(pgm, lib, opt, area, len)`: one operation of a
/// `SPECIAL` file, by calling its program with the option, a status, an
/// error code and the record. The RPG status: 0, 11 at the end of
/// file, 1217 when the program cannot be called to open or close the
/// file, 1299 for any other failure.
pub extern "sysv64" fn special(env: *mut Env, _argc: i64, _od: i64, pgm: i64, lib: i64, opt: i32, area: i64, _len: i32) -> i32 {
    let j = job(env);
    let p = ccsid::decode_trimmed(crate::value::bytes(pgm.cast_unsigned(), 10));
    let l = ccsid::decode_trimmed(crate::value::bytes(lib.cast_unsigned(), 10));
    let o = char::from_u32(u32::try_from(opt).unwrap_or(0)).unwrap_or('R');
    let mut option = [crate::value::ebcdic(&o.to_string())[0]];
    let mut status = [0xF0u8];
    let mut error = [0xF0u8; 5];
    let args = [option.as_mut_ptr() as u64, status.as_mut_ptr() as u64, error.as_mut_ptr() as u64, area.cast_unsigned()];
    let failed = |j: &mut mi::Job| {
        j.clear();
        if matches!(o, 'O' | 'C') { 1217 } else { 1299 }
    };
    match j.call_program(&p, &l, &args) {
        Err(_) => failed(j),
        Ok(()) if j.pending() => failed(j),
        Ok(()) => match status[0] {
            0xF1 => 11,
            0xF2 => 1299,
            _ => 0,
        },
    }
}

/// `_QRNX_FIXZONED(dst, src, digits)`: `FIXNBR(*ZONED)`: a copy of a
/// zoned value with each digit that is not one (a blank among them)
/// made zero and a sign that is not one made positive.
pub extern "sysv64" fn fix_zoned(_env: *mut Env, _argc: i64, _od: i64, dst: i64, src: i64, n: i32) {
    let n = usize::try_from(n).unwrap_or(0);
    let s = crate::value::bytes(src.cast_unsigned(), n).to_vec();
    let d = crate::value::bytes(dst.cast_unsigned(), n);
    for (k, b) in s.iter().enumerate() {
        let digit = if b & 0x0F <= 9 { b & 0x0F } else { 0 };
        let zone = if k + 1 == n && matches!(b >> 4, 0xB | 0xD) { b >> 4 } else { 0xF };
        d[k] = zone << 4 | digit;
    }
}

/// `_QRNX_ALTSEQ(dst, src, sd, table)`: character data translated
/// through the alternate collating sequence, for a compare.
pub extern "sysv64" fn altseq(_env: *mut Env, _argc: i64, _od: i64, dst: i64, src: i64, sd: i64, tab: i64) {
    let d = crate::value::Desc::of(sd);
    let raw = crate::value::bytes(src.cast_unsigned(), d.size()).to_vec();
    let data = if d.kind == crate::value::Kind::VarChar {
        d.var_data(&raw).to_vec()
    } else {
        raw
    };
    let t = crate::value::bytes(tab.cast_unsigned(), 256).to_vec();
    let out = crate::value::bytes(dst.cast_unsigned(), data.len());
    for (o, b) in out.iter_mut().zip(&data) {
        *o = t[usize::from(*b)];
    }
}

/// `_QRNX_NOHANDLER()`: the open of a file whose Open Access handler
/// cannot be called ends the program: a function check to its caller.
pub extern "sysv64" fn no_handler(env: *mut Env, _argc: i64, _od: i64) {
    job(env).cancel_request("CEE9901", "QCEEMSG", Vec::new());
}
