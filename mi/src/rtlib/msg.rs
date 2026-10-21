// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `sndpgmmsg`, `rcvmsg` and `rmvmsg`.

#![allow(clippy::similar_names)]

use codegen::abi::Env;

use super::core::raise_on;
use super::decimal::write_num;
use super::mem::{bytes, job, typed};
use super::string::{Text, write_text};
use crate::msg::{ClearMode, QueueRef, msg_type_of};

fn name_at(addr: i64, len: usize) -> String {
    ccsid::decode_trimmed(bytes(addr, len))
}

fn key_at(addr: i64) -> Option<u32> {
    if addr == 0 {
        return None;
    }
    let b = bytes(addr, 4);
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn write_key(addr: i64, key: u32) {
    if addr != 0 {
        bytes(addr, 4).copy_from_slice(&key.to_be_bytes());
    }
}

fn text_of(s: &str) -> Text {
    Text {
        units: s
            .chars()
            .map(|c| ccsid::byte_of(c).map_or(0x6F, u16::from))
            .collect(),
        wide: false,
    }
}

/// `sndpgmmsg(env, msgid, msgf, lib, data, dty, type, to_kind, to_n, key)`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn sndpgmmsg(
    env: *mut Env,
    msgid: i64,
    msgf: i64,
    lib: i64,
    data: i64,
    dty: i64,
    ty: i64,
    to_kind: i64,
    to_n: i64,
    key: i64,
) {
    let id = name_at(msgid, 7);
    let file = name_at(msgf, 10);
    let library = if lib == 0 {
        "*LIBL".to_string()
    } else {
        name_at(lib, 10)
    };
    let data = if data == 0 {
        Vec::new()
    } else {
        typed(data, dty).0.to_vec()
    };
    let Some(mt) = msg_type_of(ty) else {
        raise_on(job(env), codegen::Cond::Mch3601);
        return;
    };
    let j = job(env);
    match j.send_message(
        &id,
        &file,
        &library,
        data,
        mt,
        QueueRef::from_code(to_kind, to_n),
    ) {
        Ok(k) => write_key(key, k),
        Err(c) => raise_on(j, c),
    }
}

/// `rcvmsg(env, pgmq_kind, pgmq_n, type, key, rmv, msg, mty, msgid, data, dty, keyout, sev, sevty) -> 0|1`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn rcvmsg(
    env: *mut Env,
    pgmq_kind: i64,
    pgmq_n: i64,
    ty: i64,
    key: i64,
    rmv: i64,
    msg: i64,
    mty: i64,
    msgid: i64,
    data: i64,
    dty: i64,
    keyout: i64,
    sev: i64,
    sevty: i64,
) -> i32 {
    let j = job(env);
    let want = if ty < 0 { None } else { msg_type_of(ty) };
    let Some(m) = j.receive_message(
        QueueRef::from_code(pgmq_kind, pgmq_n),
        want,
        key_at(key),
        rmv != 0,
    ) else {
        return 0;
    };
    if msg != 0 {
        write_text(msg, mty, &text_of(&m.text));
    }
    if msgid != 0 {
        let mut b = [ccsid::SPACE; 7];
        let _ = ccsid::encode_fixed(&m.msgid, &mut b);
        bytes(msgid, 7).copy_from_slice(&b);
    }
    if data != 0 {
        let (d, _) = typed(data, dty);
        let n = d.len().min(m.data.len());
        d[..n].copy_from_slice(&m.data[..n]);
        d[n..].fill(ccsid::SPACE);
    }
    write_key(keyout, m.key);
    if sev != 0 {
        let v = qdecimal::Decimal::new(i128::from(m.severity), 0).expect("small");
        if let Err(c) = write_num(sev, sevty, &v, qdecimal::Rounding::Truncate) {
            raise_on(j, c);
        }
    }
    1
}

/// `rmvmsg(env, pgmq_kind, pgmq_n, key, clear)`.
pub extern "sysv64" fn rmvmsg(env: *mut Env, pgmq_kind: i64, pgmq_n: i64, key: i64, clear: i64) {
    let j = job(env);
    let q = QueueRef::from_code(pgmq_kind, pgmq_n);
    if let Err(c) = j.remove_messages(q, key_at(key), ClearMode::from_code(clear)) {
        raise_on(j, c);
    }
}
