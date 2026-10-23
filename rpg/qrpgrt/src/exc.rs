// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program status data structure and the last status: what a
//! handler records of the exception it took, and handing that
//! exception on as unhandled.
//!
//! PSDS positions (from 1): 1-10 the procedure, 11-15 the status,
//! 16-20 the previous status, 21-28 the statement, 29-36 the routine,
//! 37-39 the parameters, 40-42 the exception type, 43-46 its number,
//! 91-170 the exception data.

use codegen::abi::Env;
use qdecimal::{Decimal, Rounding, Zoned};

use crate::value::bytes;

fn job(env: *mut Env) -> &'static mut mi::Job {
    mi::storage::job_of(env)
}

/// Zoned digits with the RPG positive sign.
fn zoned(n: i128, digits: u8) -> Vec<u8> {
    let z = Zoned::new(digits, 0).expect("valid");
    let mut b = z
        .encode(&Decimal::new(n, 0).unwrap_or_else(|_| Decimal::zero()), Rounding::Truncate)
        .unwrap_or_else(|_| vec![0xF0; usize::from(digits)]);
    if n >= 0
        && let Some(last) = b.last_mut()
    {
        *last |= 0xF0;
    }
    b
}

/// Put bytes at a PSDS position (1-based) when the structure has room.
fn put(ds: &mut [u8], pos: usize, data: &[u8]) {
    let from = pos - 1;
    if from + data.len() <= ds.len() {
        ds[from..from + data.len()].copy_from_slice(data);
    }
}

fn ebcdic(s: &str) -> Vec<u8> {
    s.chars().map(|c| ccsid::byte_of(c).unwrap_or(0x40)).collect()
}

/// `_QRNX_PSDSINIT(ds, len, name, parms)`: the fixed fields at start.
pub extern "sysv64" fn psds_init(_env: *mut Env, _argc: i64, _od: i64, ds: i64, len: i32, name: i64, parms: i32) {
    let n = usize::try_from(len).unwrap_or(0);
    if n == 0 {
        return;
    }
    let ds = bytes(ds.cast_unsigned(), n);
    let name = bytes(name.cast_unsigned(), 10).to_vec();
    put(ds, 1, &name);
    put(ds, 11, &zoned(0, 5));
    put(ds, 16, &zoned(0, 5));
    put(ds, 37, &zoned(i128::from(parms), 3));
}

/// `_QRNX_PSDSERR(ds, len, status)`: the exception the last handler
/// took into the PSDS and the last status.
pub extern "sysv64" fn psds_err(env: *mut Env, _argc: i64, _od: i64, ds: i64, len: i32, status: i64) {
    let j = job(env);
    let Some(e) = j.handled.last().cloned() else { return };
    let st = i128::from(e.status);
    bytes(status.cast_unsigned(), 5).copy_from_slice(&zoned(st, 5));
    let n = usize::try_from(len).unwrap_or(0);
    if n == 0 {
        return;
    }
    let ds = bytes(ds.cast_unsigned(), n);
    if n >= 15 {
        let old = ds[10..15].to_vec();
        put(ds, 16, &old);
    }
    put(ds, 11, &zoned(st, 5));
    put(ds, 21, &ebcdic(&format!("{:08}", j.abi.cur_stmt)));
    let id: Vec<char> = e.msgid.chars().collect();
    let ty: String = id.iter().take(3).collect();
    let num: String = id.iter().skip(3).take(4).collect();
    put(ds, 40, &ebcdic(&format!("{ty:<3}")));
    put(ds, 43, &ebcdic(&format!("{num:<4}")));
    let mut data = e.data.clone();
    data.resize(80, 0x40);
    put(ds, 91, &data);
}

/// `_QRNX_RERAISE()`: the exception the last handler took, sent again
/// as an escape to the running entry, where no handler waits for it.
pub extern "sysv64" fn reraise(env: *mut Env, _argc: i64, _od: i64) {
    let j = job(env);
    let Some(e) = j.handled.last().cloned() else { return };
    let _ = j.send_message(&e.msgid, &e.msgf, "*LIBL", e.data, qir::MsgType::Escape, mi::QueueRef::Same);
}
