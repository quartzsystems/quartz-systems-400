// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program status data structure and the last status: what a
//! handler records of the exception it took, and handing that
//! exception on as unhandled.
//!
//! The PSDS positions kept are [`PSDS`]; `docs/RPG.md` has the table
//! and `tools/conformance/tests/rpg_layouts.rs` compares the two.

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

/// A PSDS field: its first position (from 1) and length.
pub type Field = (usize, usize);

/// The procedure.
pub const PROC: Field = (1, 10);
/// The status.
pub const STATUS: Field = (11, 5);
/// The status before it.
pub const PREV_STATUS: Field = (16, 5);
/// The statement of the exception.
pub const STMT: Field = (21, 8);
/// The routine running.
pub const ROUTINE: Field = (29, 8);
/// The number of parameters passed.
pub const PARMS: Field = (37, 3);
/// The exception's message type (`MCH`, `CPF`, ...).
pub const EXC_TYPE: Field = (40, 3);
/// The exception's message number.
pub const EXC_NUM: Field = (43, 4);
/// The program's library.
pub const PGM_LIB: Field = (81, 10);
/// The exception's replacement data.
pub const EXC_DATA: Field = (91, 80);
/// The job name.
pub const JOB: Field = (244, 10);
/// The job's user.
pub const USER: Field = (254, 10);
/// The job number.
pub const JOB_NUM: Field = (264, 6);
/// The program holding the procedure.
pub const PGM: Field = (334, 10);
/// The module holding the procedure.
pub const MODULE: Field = (344, 10);
/// The current user profile.
pub const CUR_USER: Field = (374, 10);

/// Every position, by the name the table in `docs/RPG.md` gives it.
pub const PSDS: &[(&str, Field)] = &[
    ("proc", PROC),
    ("status", STATUS),
    ("prev_status", PREV_STATUS),
    ("stmt", STMT),
    ("routine", ROUTINE),
    ("parms", PARMS),
    ("exc_type", EXC_TYPE),
    ("exc_num", EXC_NUM),
    ("pgm_lib", PGM_LIB),
    ("exc_data", EXC_DATA),
    ("job", JOB),
    ("user", USER),
    ("job_num", JOB_NUM),
    ("pgm", PGM),
    ("module", MODULE),
    ("cur_user", CUR_USER),
];

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

/// Text at a field, blank-padded to its length.
fn put_text(ds: &mut [u8], (pos, len): Field, s: &str) {
    let mut b = ebcdic(s);
    b.resize(len, 0x40);
    put(ds, pos, &b);
}

/// `_QRNX_PSDSINIT(ds, len, name, parms)`: the fixed fields at start.
pub extern "sysv64" fn psds_init(env: *mut Env, _argc: i64, _od: i64, ds: i64, len: i32, name: i64, parms: i32) {
    let n = usize::try_from(len).unwrap_or(0);
    if n == 0 {
        return;
    }
    let ds = bytes(ds.cast_unsigned(), n);
    let name = bytes(name.cast_unsigned(), 10).to_vec();
    put(ds, PROC.0, &name);
    put(ds, STATUS.0, &zoned(0, 5));
    put(ds, PREV_STATUS.0, &zoned(0, 5));
    put(ds, PARMS.0, &zoned(i128::from(parms), 3));
    let j = job(env);
    put_text(ds, JOB, &j.ident.name);
    put_text(ds, USER, &j.ident.user);
    put_text(ds, JOB_NUM, &j.ident.number);
    put_text(ds, CUR_USER, &j.ident.user);
    // The program and module of the entry running.
    let Some(f) = j.act.frames.last() else { return };
    let Some(p) = f.program.clone() else { return };
    let (lib, pgm) = p.key.split_once('/').unwrap_or(("", p.key.as_str()));
    put_text(ds, PGM_LIB, lib);
    put_text(ds, PGM, pgm);
    if let Some((mi, _)) = p.proc_of(f.code)
        && let Some(m) = p.object.modules.get(mi as usize)
    {
        put_text(ds, MODULE, &m.module.name);
    }
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
        put(ds, PREV_STATUS.0, &old);
    }
    put(ds, STATUS.0, &zoned(st, 5));
    put_text(ds, STMT, &format!("{:08}", j.abi.cur_stmt));
    let id: Vec<char> = e.msgid.chars().collect();
    let ty: String = id.iter().take(3).collect();
    let num: String = id.iter().skip(3).take(4).collect();
    put_text(ds, EXC_TYPE, &ty);
    put_text(ds, EXC_NUM, &num);
    let mut data = e.data.clone();
    data.resize(EXC_DATA.1, 0x40);
    put(ds, EXC_DATA.0, &data);
}

/// `_QRNX_RERAISE()`: the exception the last handler took, sent again
/// as an escape to the running entry, where no handler waits for it.
pub extern "sysv64" fn reraise(env: *mut Env, _argc: i64, _od: i64) {
    let j = job(env);
    let Some(e) = j.handled.last().cloned() else { return };
    let _ = j.send_message(&e.msgid, &e.msgf, "*LIBL", e.data, qir::MsgType::Escape, mi::QueueRef::Same);
}
