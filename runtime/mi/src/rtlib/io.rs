// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The runtime behind `dbopen`, `dbio`, `dbclose`, `dspio`/`prtio`,
//! `dtaara`, `dtaq` and `rslvsp`: files are opened per activation and
//! module, the information data structure is kept current after every
//! operation, and data objects go through the job's data source. A
//! printer file opens a spooled file through the job's spool sink and
//! runs its line printer here.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::single_match_else
)]

use codegen::Cond;
use codegen::abi::Env;
use qdecimal::{Decimal, Packed, Rounding, Zoned};
use qir::Type;

use super::core::raise_on;
use super::mem::{bytes, env as env_of, job, ty};
use crate::io::spool::PrinterState;
use crate::io::display::DisplayInput;
use crate::io::{
    DataAreaKind, DataAreaValue, FileInfo, IoFail, IoOp, IoRequest, IoResult, KeyRef, OpenSpec,
};
use crate::job::{FileKey, FileState, Job};
use crate::msg::QueueRef;
use crate::space::{FatPtr, PtrKind};

/// RPG status for an operation on a file that is not open.
const CLOSED: i32 = 1211;
/// RPG status for an unsupported device operation.
const NO_DEVICE: i32 = 1299;

/// The running entry's file declaration and the key of its state.
fn file_of(env: *mut Env, file: i64) -> Option<(FileKey, qir::FileRef)> {
    let j = job(env);
    let statics = env_of(env).statics as u64;
    let f = j.act.frames.last()?;
    let p = f.program.as_ref()?;
    let (mi, _) = p.proc_of(f.code)?;
    let fr = p.object.modules[mi as usize]
        .module
        .ir
        .files
        .get(usize::try_from(file).ok()?)?
        .clone();
    let key = FileKey {
        statics,
        module: mi,
        file: u32::try_from(file).ok()?,
    };
    Some((key, fr))
}

/// Send an I/O failure as an escape to the running entry.
fn escape(j: &mut Job, f: &IoFail) {
    let _ = j.send_message(
        &f.msgid,
        "QCPFMSG",
        "QSYS",
        f.data.clone(),
        qir::MsgType::Escape,
        QueueRef::Same,
    );
}

/// Zoned digits of a status.
fn zoned5(status: u16) -> [u8; 5] {
    let s = format!("{status:05}");
    let mut out = [0xF0; 5];
    for (o, c) in out.iter_mut().zip(s.bytes()) {
        *o = 0xF0 | (c - b'0');
    }
    out
}

fn put(buf: &mut [u8], off: usize, data: &[u8]) {
    if off + data.len() <= buf.len() {
        buf[off..off + data.len()].copy_from_slice(data);
    }
}

fn put_text(buf: &mut [u8], off: usize, n: usize, s: &str) {
    let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
    b.resize(n, 0x40);
    put(buf, off, &b);
}

/// Keep the information data structure current (`crate::io::infds`).
fn write_infds(
    addr: u64,
    len: usize,
    info: &FileInfo,
    r: &IoResult,
    open: bool,
    opcode: &str,
    stmt: u32,
) {
    use crate::io::infds as at;
    if addr == 0 || len == 0 {
        return;
    }
    let buf = bytes(addr.cast_signed(), len);
    let text = |buf: &mut [u8], (off, n): (usize, usize), s: &str| put_text(buf, off, n, s);
    text(buf, at::FILE, &info.file);
    text(buf, at::OPEN, if open { "1" } else { "0" });
    text(buf, at::EOF, if r.eof { "1" } else { "0" });
    put(buf, at::STATUS.0, &zoned5(r.status));
    text(buf, at::OPCODE, opcode);
    text(buf, at::ROUTINE, "*INIT");
    text(buf, at::STMT, &stmt.to_string());
    let format = if r.format.is_empty() { &info.format } else { &r.format };
    text(buf, at::RECORD, format);
    text(buf, at::MSGID, r.msgid.as_deref().unwrap_or(""));
    let member = if r.member.is_empty() { &info.member } else { &r.member };
    text(buf, at::MEMBER, member);
    put(buf, at::RRN.0, &u32::try_from(r.rrn).unwrap_or(u32::MAX).to_be_bytes());
    put(buf, at::KEY.0, &r.key);
}

/// The running entry's activation group.
fn actgrp(j: &Job) -> String {
    j.act
        .frames
        .last()
        .and_then(|f| j.act.groups().find(|g| g.id == f.ag))
        .map_or_else(|| "*DFTACTGRP".into(), |g| g.name.clone())
}

/// Open a printer file: a spooled file through the job's sink.
fn open_printer(j: &mut Job, key: FileKey, fr: &qir::FileRef, infds: u64, infds_len: usize) -> i32 {
    let info = FileInfo {
        file: fr.file.clone(),
        lib: fr.lib.clone(),
        ..FileInfo::default()
    };
    let ag = actgrp(j);
    let opened = match j.spool.as_mut() {
        Some(s) => s.open(&fr.file, &fr.lib, &ag),
        None => Err(IoFail::new(1299, "CPF4101", &[&fr.file, &fr.lib])),
    };
    match opened {
        Ok(mut o) => {
            // The program's page length and overflow line, if it gives them.
            if fr.form.0 != 0 {
                o.prtf.page_len = fr.form.0;
                o.prtf.ovrflw = o.prtf.ovrflw.min(fr.form.0);
            }
            if fr.form.1 != 0 {
                o.prtf.ovrflw = fr.form.1;
            }
            write_infds(
                infds,
                infds_len,
                &info,
                &IoResult::default(),
                true,
                "OPEN",
                j.abi.cur_stmt,
            );
            j.files.insert(
                key,
                FileState {
                    handle: o.id,
                    infds,
                    infds_len,
                    info,
                    printer: Some(Box::new(PrinterState::new(o))),
                    display: false,
                    fb: 0,
                },
            );
            0
        }
        Err(f) => {
            let r = IoResult {
                status: f.status,
                msgid: Some(f.msgid.clone()),
                ..IoResult::default()
            };
            write_infds(infds, infds_len, &info, &r, false, "OPEN", j.abi.cur_stmt);
            escape(j, &f);
            i32::from(f.status)
        }
    }
}

/// Open a display file through the job's display source.
fn open_display(j: &mut Job, key: FileKey, fr: &qir::FileRef, infds: u64, infds_len: usize, rpg: bool) -> i32 {
    let mut info = FileInfo {
        file: fr.file.clone(),
        lib: fr.lib.clone(),
        ..FileInfo::default()
    };
    let ag = actgrp(j);
    let opened = match j.display.as_mut() {
        Some(d) => d.open(&fr.file, &fr.lib, &ag),
        // A job without a display (a batch job): the open fails.
        None => Err(IoFail::new(1217, "CPF4101", &[&fr.file, &fr.lib])),
    };
    let no_display = j.display.is_none();
    match opened {
        Ok(o) => {
            info.lib = o.lib;
            write_infds(
                infds,
                infds_len,
                &info,
                &IoResult::default(),
                true,
                "OPEN",
                j.abi.cur_stmt,
            );
            j.files.insert(
                key,
                FileState {
                    handle: o.handle,
                    infds,
                    infds_len,
                    info,
                    printer: None,
                    display: true,
                    fb: 0,
                },
            );
            0
        }
        Err(f) => {
            let r = IoResult {
                status: f.status,
                msgid: Some(f.msgid.clone()),
                ..IoResult::default()
            };
            write_infds(infds, infds_len, &info, &r, false, "OPEN", j.abi.cur_stmt);
            // An RPG module keeps a batch job's failure as the status.
            if !(rpg && no_display) {
                escape(j, &f);
            }
            i32::from(f.status)
        }
    }
}

/// `db_open(env, file, infds, infds_desc) -> status`.
pub extern "sysv64" fn db_open(env: *mut Env, file: i64, infds: i64, infds_desc: i64) -> i32 {
    let Some((key, fr)) = file_of(env, file) else {
        raise_on(job(env), Cond::Mch3601);
        return CLOSED;
    };
    let j = job(env);
    let infds_len = if infds == 0 {
        0
    } else {
        usize::try_from(ty(infds_desc).size()).unwrap_or(0)
    };
    if fr.dev == qir::FileDev::Printer {
        return open_printer(j, key, &fr, infds.cast_unsigned(), infds_len);
    }
    if fr.dev == qir::FileDev::Display {
        let rpg = module_lang(env) == qir::Lang::Rpgle;
        let j = job(env);
        return open_display(j, key, &fr, infds.cast_unsigned(), infds_len, rpg);
    }
    let spec = OpenSpec {
        file: fr.file.clone(),
        lib: fr.lib.clone(),
        usage: fr.usage,
        keyed: fr.keyed,
        lvlchk: fr.lvlchk.then(|| {
            fr.formats
                .iter()
                .filter_map(|f| f.level.clone().map(|l| (f.name.clone(), l)))
                .collect()
        }),
        member: None,
        actgrp: actgrp(j),
    };
    // Under debug (a program named by STRDBG) with UPDPROD(*NO), a
    // production file is not changed.
    if j.debug
        .as_ref()
        .is_some_and(|d| !d.updprod && !d.programs.is_empty())
        && spec.usage != qir::FileUsage::Input
        && !spec.lib.eq_ignore_ascii_case("QTEMP")
    {
        let f = IoFail::new(1299, "CPF4188", &[&fr.file, &fr.lib]);
        escape(j, &f);
        return i32::from(f.status);
    }
    let Some(io) = j.io.as_mut() else {
        let f = IoFail::new(1299, "CPF4101", &[&fr.file, &fr.lib]);
        escape(j, &f);
        return i32::from(f.status);
    };
    match io.open(&spec) {
        Ok((handle, info)) => {
            let r = IoResult::default();
            write_infds(
                infds.cast_unsigned(),
                infds_len,
                &info,
                &r,
                true,
                "OPEN",
                j.abi.cur_stmt,
            );
            j.files.insert(
                key,
                FileState {
                    handle,
                    infds: infds.cast_unsigned(),
                    infds_len,
                    info,
                    printer: None,
                    display: false,
                    fb: 0,
                },
            );
            0
        }
        Err(f) => {
            let info = FileInfo {
                file: fr.file.clone(),
                lib: fr.lib.clone(),
                ..FileInfo::default()
            };
            let r = IoResult {
                status: f.status,
                msgid: Some(f.msgid.clone()),
                ..IoResult::default()
            };
            write_infds(
                infds.cast_unsigned(),
                infds_len,
                &info,
                &r,
                false,
                "OPEN",
                j.abi.cur_stmt,
            );
            escape(j, &f);
            i32::from(f.status)
        }
    }
}

/// `db_close(env, file) -> status`.
pub extern "sysv64" fn db_close(env: *mut Env, file: i64) -> i32 {
    let Some((key, _)) = file_of(env, file) else {
        return CLOSED;
    };
    let j = job(env);
    let Some(mut state) = j.files.remove(&key) else {
        return CLOSED;
    };
    let r = if let Some(p) = state.printer.as_mut() {
        let closed = j.spool.as_mut().map(|s| p.close(s.as_mut()));
        match closed {
            Some(Err(f)) => IoResult {
                status: f.status,
                msgid: Some(f.msgid),
                ..IoResult::default()
            },
            _ => IoResult::default(),
        }
    } else if state.display {
        match j.display.as_mut().map(|d| d.close(state.handle)) {
            Some(Err(f)) => IoResult {
                status: f.status,
                msgid: Some(f.msgid),
                ..IoResult::default()
            },
            _ => IoResult::default(),
        }
    } else {
        j.io.as_mut()
            .map(|io| io.close(state.handle))
            .unwrap_or_default()
    };
    write_infds(
        state.infds,
        state.infds_len,
        &state.info,
        &r,
        false,
        "CLOSE",
        j.abi.cur_stmt,
    );
    i32::from(r.status)
}

/// `db_io(env, file, op, rec, rec_desc, key, key_desc, rrn, fmt,
/// nolock) -> status`.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn db_io(
    env: *mut Env,
    file: i64,
    op: i64,
    rec: i64,
    rec_desc: i64,
    key: i64,
    key_desc: i64,
    rrn: i64,
    fmt: i64,
    nolock: i64,
) -> i32 {
    let Some((fkey, _)) = file_of(env, file) else {
        return CLOSED;
    };
    if op == codegen::rt::DB_FEEDBACK {
        return job(env).files.get(&fkey).map_or(0, |s| s.fb | 8);
    }
    let Some(op) = IoOp::from_code(op) else {
        raise_on(job(env), Cond::Mch3601);
        return CLOSED;
    };
    let j = job(env);
    let Some(state) = j.files.get(&fkey) else {
        return CLOSED;
    };
    let (handle, infds, infds_len, info) = (
        state.handle,
        state.infds,
        state.infds_len,
        state.info.clone(),
    );
    let rec_len = if rec == 0 {
        0
    } else {
        usize::try_from(ty(rec_desc).size()).unwrap_or(0)
    };
    let record: Option<Vec<u8>> = (rec != 0 && (op == IoOp::Write || op == IoOp::Update))
        .then(|| bytes(rec, rec_len).to_vec());
    let key_bytes: Vec<u8> = if key == 0 {
        Vec::new()
    } else {
        let n = usize::try_from(ty(key_desc).size()).unwrap_or(0);
        bytes(key, n).to_vec()
    };
    let keyref = if key != 0 {
        KeyRef::Fields(&key_bytes)
    } else if rrn > 0 {
        KeyRef::Rrn(rrn.cast_unsigned())
    } else {
        match rrn {
            -1 => KeyRef::Start,
            -2 => KeyRef::End,
            -3 => KeyRef::LoVal,
            -4 => KeyRef::HiVal,
            _ => KeyRef::None,
        }
    };
    let format = (fmt != 0).then(|| ccsid::decode_trimmed(bytes(fmt, 10)));
    let req = IoRequest {
        op,
        record: record.as_deref(),
        key: keyref,
        format: format.as_deref(),
        nolock: nolock != 0,
    };
    let Some(io) = j.io.as_mut() else {
        return CLOSED;
    };
    let r = io.io(handle, &req);
    if let Some(data) = &r.record
        && rec != 0
    {
        let buf = bytes(rec, rec_len);
        let n = rec_len.min(data.len());
        buf[..n].copy_from_slice(&data[..n]);
        if n < rec_len {
            buf[n..].fill(0x40);
        }
    }
    write_infds(infds, infds_len, &info, &r, true, op.name(), j.abi.cur_stmt);
    if let Some(st) = j.files.get_mut(&fkey) {
        st.fb = i32::from(r.eof) | i32::from(r.found) << 1 | i32::from(r.equal) << 2;
    }
    i32::from(r.status)
}

/// `dev_io(env, file, op, fmt, rec, rec_desc) -> status`. A printer
/// file's `WRITE` prints the record: 0, or 1 when the carriage reached
/// the overflow line. A display file's `WRITE` (op 0), `READ` (1) and
/// `EXFMT` (2) go to the job's display source; a read copies the input
/// buffer back over the record and leaves the attention identifier,
/// cursor and subfile record number in the information data structure.
/// A display file that is not open yet opens with its first operation,
/// as CL's do.
pub extern "sysv64" fn dev_io(
    env: *mut Env,
    file: i64,
    op: i64,
    fmt: i64,
    rec: i64,
    rec_desc: i64,
) -> i32 {
    let Some((key, fr)) = file_of(env, file) else {
        return CLOSED;
    };
    let j = job(env);
    if !j.files.contains_key(&key) && fr.dev == qir::FileDev::Display {
        let st = open_display(j, key, &fr, 0, 0, false);
        if st != 0 {
            return st;
        }
    }
    let format = if fmt == 0 {
        String::new()
    } else {
        ccsid::decode_trimmed(bytes(fmt, 10))
    };
    let rec_len = if rec == 0 {
        0
    } else {
        usize::try_from(ty(rec_desc).size()).unwrap_or(0)
    };
    let record = if rec == 0 {
        Vec::new()
    } else {
        bytes(rec, rec_len).to_vec()
    };
    let Some(state) = j.files.get_mut(&key) else {
        return CLOSED;
    };
    if state.display {
        return display_io(j, key, op, &format, rec, rec_len, &record);
    }
    let Some(p) = state.printer.as_mut() else {
        return NO_DEVICE;
    };
    let Some(sink) = j.spool.as_mut() else {
        return NO_DEVICE;
    };
    match p.write(sink.as_mut(), &format, &record) {
        Ok(over) => i32::from(over),
        Err(f) => {
            escape(j, &f);
            i32::from(f.status)
        }
    }
}

/// `prt_ctl(env, file, fmt, rec, rec_desc, ctl) -> status`: one line of
/// a printer file with the program's carriage control, `ctl` packing
/// space before, space after, skip before and skip after in 16 bits
/// each from the top. 0, or 1 when the carriage reached the overflow
/// line.
pub extern "sysv64" fn prt_ctl(
    env: *mut Env,
    file: i64,
    _fmt: i64,
    rec: i64,
    rec_desc: i64,
    ctl: i64,
) -> i32 {
    let Some((key, _)) = file_of(env, file) else {
        return CLOSED;
    };
    let j = job(env);
    let rec_len = if rec == 0 {
        0
    } else {
        usize::try_from(ty(rec_desc).size()).unwrap_or(0)
    };
    let record = if rec == 0 {
        Vec::new()
    } else {
        bytes(rec, rec_len).to_vec()
    };
    let word = ctl.cast_unsigned();
    let part = |shift: u32| u16::try_from((word >> shift) & 0xFFFF).unwrap_or(0);
    let control = spool::printer::Control {
        space_before: part(48),
        space_after: part(32),
        skip_before: part(16),
        skip_after: part(0),
    };
    let Some(state) = j.files.get_mut(&key) else {
        return CLOSED;
    };
    let Some(p) = state.printer.as_mut() else {
        return NO_DEVICE;
    };
    let Some(sink) = j.spool.as_mut() else {
        return NO_DEVICE;
    };
    match p.write_ctl(sink.as_mut(), &record, control) {
        Ok(over) => i32::from(over),
        Err(f) => {
            escape(j, &f);
            i32::from(f.status)
        }
    }
}

/// A display file operation: the write, the read, or both.
fn display_io(
    j: &mut Job,
    key: FileKey,
    op: i64,
    format: &str,
    rec: i64,
    rec_len: usize,
    record: &[u8],
) -> i32 {
    let Some(state) = j.files.get(&key) else {
        return CLOSED;
    };
    let handle = state.handle;
    let (infds, infds_len, info) = (state.infds, state.infds_len, state.info.clone());
    let Some(d) = j.display.as_mut() else {
        return NO_DEVICE;
    };
    let r = match op {
        1 => d.write(handle, format, record, &[]).map(|()| None),
        2 => d.read(handle, format).map(Some),
        _ => d.exfmt(handle, format, record, &[]).map(Some),
    };
    match r {
        Ok(input) => {
            let mut fb = IoResult::default();
            if let Some(input) = input {
                if rec != 0 {
                    let dst = bytes(rec, rec_len);
                    let n = input.record.len().min(rec_len);
                    dst[..n].copy_from_slice(&input.record[..n]);
                }
                fb.format = input.format;
                fb.rrn = u64::from(input.sfl_rrn);
                fb.key = vec![
                    input.aid,
                    u8::try_from(input.cursor.0).unwrap_or(0),
                    u8::try_from(input.cursor.1).unwrap_or(0),
                ];
            }
            write_infds(infds, infds_len, &info, &fb, false, "DSPIO", j.abi.cur_stmt);
            0
        }
        Err(f) if f.status == IoFail::REQUEST_ENDED => {
            j.cancel_request(&f.msgid, "QCPFMSG", f.data);
            0
        }
        Err(f) => {
            escape(j, &f);
            i32::from(f.status)
        }
    }
}

/// The display-specific part of the information data structure: the
/// attention identifier at 368, the cursor row and column at 369, the
/// data length at 371, the subfile record number at 375, the lowest
/// one displayed at 377, the subfile's record count at 379 and the
/// major and minor return code at 400.
fn write_dsp_infds(addr: u64, len: usize, input: &DisplayInput) {
    if addr == 0 || len == 0 {
        return;
    }
    let buf = bytes(addr.cast_signed(), len);
    put(buf, 368, &[input.aid]);
    put(
        buf,
        369,
        &[
            u8::try_from(input.cursor.0).unwrap_or(0),
            u8::try_from(input.cursor.1).unwrap_or(0),
        ],
    );
    put(
        buf,
        371,
        &u32::try_from(input.record.len()).unwrap_or(0).to_be_bytes(),
    );
    let half = |n: u32| u16::try_from(n).unwrap_or(u16::MAX).to_be_bytes();
    put(buf, 375, &half(input.sfl_rrn));
    put(buf, 377, &half(input.sfl_top));
    put(buf, 379, &half(input.sfl_count));
    put_text(buf, 400, 4, "0000");
}

/// `dsp_io(env, file, op, fmt, rec, rec_desc, rrn, inds, inds_desc) ->
/// status`: a display file's `WRITE` (1; at `rrn` for a subfile
/// record), `READ` (2; an empty format reads the last one written),
/// `EXFMT` (3), subfile `CHAIN` (4), `UPDATE` (5) and `READC` (6). A
/// read copies the input buffer over the record and the response
/// indicators into the separate indicator area; the feedback keeps the
/// attention identifier and the subfile record number for `dbfb`, and
/// the information data structure its display part. `CHAIN` that finds
/// nothing clears the found bit, `READC` at the end sets end of file.
#[allow(clippy::too_many_arguments)]
pub extern "sysv64" fn dsp_io(
    env: *mut Env,
    file: i64,
    op: i64,
    fmt: i64,
    rec: i64,
    rec_desc: i64,
    rrn: i64,
    inds: i64,
    inds_desc: i64,
) -> i32 {
    let Some((key, fr)) = file_of(env, file) else {
        return CLOSED;
    };
    let j = job(env);
    // RPG keeps an I/O failure as a status; CL's is an escape.
    let rpg = module_lang(env) == qir::Lang::Rpgle;
    if !j.files.contains_key(&key) && fr.dev == qir::FileDev::Display && !rpg {
        // A CL program's file opens with its first operation.
        let st = open_display(j, key, &fr, 0, 0, false);
        if st != 0 {
            return st;
        }
    }
    let format = if fmt == 0 {
        String::new()
    } else {
        ccsid::decode_trimmed(bytes(fmt, 10))
    };
    let size = |p: i64, d: i64| {
        if p == 0 {
            0
        } else {
            usize::try_from(ty(d).size()).unwrap_or(0)
        }
    };
    let rec_len = size(rec, rec_desc);
    let inds_len = size(inds, inds_desc);
    let record = if rec == 0 {
        Vec::new()
    } else {
        bytes(rec, rec_len).to_vec()
    };
    let area = if inds == 0 {
        Vec::new()
    } else {
        bytes(inds, inds_len).to_vec()
    };
    let Some(state) = j.files.get(&key) else {
        return CLOSED;
    };
    if !state.display {
        return NO_DEVICE;
    }
    let (handle, infds, infds_len, info) = (
        state.handle,
        state.infds,
        state.infds_len,
        state.info.clone(),
    );
    let Some(d) = j.display.as_mut() else {
        return NO_DEVICE;
    };
    let n = u32::try_from(rrn).unwrap_or(0);
    // read that found nothing.
    let read = |o: Option<DisplayInput>| o.map_or(Got::None, Got::Record);
    let r: Result<Got, IoFail> = match op {
        1 if n > 0 => d.sfl_write(handle, &format, n, &record, &area).map(|()| Got::Output),
        1 => d.write(handle, &format, &record, &area).map(|()| Got::Output),
        2 => d.read(handle, &format).map(Got::Record),
        3 => d.exfmt(handle, &format, &record, &area).map(Got::Record),
        4 => d.sfl_chain(handle, &format, n).map(read),
        5 => d.sfl_update(handle, &format, &record, &area).map(|()| Got::Output),
        _ => d.sfl_readc(handle, &format).map(read),
    };
    let opname = match op {
        1 => "WRITE",
        2 => "READ",
        3 => "EXFMT",
        4 => "CHAIN",
        5 => "UPDATE",
        _ => "READC",
    };
    let at = DspAt {
        key,
        op,
        opname,
        rec,
        rec_len,
        inds,
        inds_len,
        infds,
        infds_len,
        rpg,
    };
    dsp_result(j, &at, &info, format, r)
}

/// What a display operation brought back.
enum Got {
    /// An output: nothing.
    Output,
    /// A record read.
    Record(DisplayInput),
    /// No subfile record: not found (`CHAIN`) or the end (`READC`).
    None,
}

/// Where a display operation's results go.
struct DspAt {
    key: FileKey,
    op: i64,
    opname: &'static str,
    rec: i64,
    rec_len: usize,
    inds: i64,
    inds_len: usize,
    infds: u64,
    infds_len: usize,
    rpg: bool,
}

/// A display operation's result into the record, the indicator area,
/// the feedback and the information data structure: its status.
fn dsp_result(
    j: &mut Job,
    at: &DspAt,
    info: &FileInfo,
    format: String,
    r: Result<Got, IoFail>,
) -> i32 {
    let DspAt {
        key,
        op,
        opname,
        rec,
        rec_len,
        inds,
        inds_len,
        infds,
        infds_len,
        rpg,
    } = *at;
    match r {
        Ok(got) => {
            let mut fb = IoResult {
                format: format.clone(),
                ..IoResult::default()
            };
            let mut bits = 0;
            let mut input = None;
            match got {
                Got::Record(i) => {
                    if rec != 0 {
                        let dst = bytes(rec, rec_len);
                        let m = i.record.len().min(rec_len);
                        dst[..m].copy_from_slice(&i.record[..m]);
                    }
                    if inds != 0 && !i.indicators.is_empty() {
                        let dst = bytes(inds, inds_len);
                        let m = i.indicators.len().min(inds_len);
                        dst[..m].copy_from_slice(&i.indicators[..m]);
                    }
                    fb.format.clone_from(&i.format);
                    bits = 2;
                    input = Some(i);
                }
                Got::None if op == 4 => {}
                Got::None => {
                    fb.eof = true;
                    fb.status = 11;
                    bits = 1;
                }
                Got::Output => {}
            }
            write_infds(infds, infds_len, info, &fb, true, opname, j.abi.cur_stmt);
            let mut word = bits;
            if let Some(i) = &input {
                write_dsp_infds(infds, infds_len, i);
                word |= i32::from(i.aid) << 8 | i32::try_from(i.sfl_rrn.min(0x7FFF)).unwrap_or(0) << 16;
            } else if let Some(st) = j.files.get(&key) {
                // An output keeps the last input's identifier and number.
                word |= st.fb & !0xFF;
            }
            if let Some(st) = j.files.get_mut(&key) {
                st.fb = word;
            }
            i32::from(fb.status)
        }
        Err(f) if f.status == IoFail::REQUEST_ENDED => {
            j.cancel_request(&f.msgid, "QCPFMSG", f.data);
            0
        }
        Err(f) => {
            let fb = IoResult {
                status: f.status,
                msgid: Some(f.msgid.clone()),
                format,
                ..IoResult::default()
            };
            write_infds(infds, infds_len, info, &fb, true, opname, j.abi.cur_stmt);
            if !rpg {
                escape(j, &f);
            }
            i32::from(f.status)
        }
    }
}

/// The language of the running entry's module.
fn module_lang(env: *mut Env) -> qir::Lang {
    let j = job(env);
    j.act
        .frames
        .last()
        .and_then(|f| {
            let p = f.program.as_ref()?;
            let (mi, _) = p.proc_of(f.code)?;
            Some(p.object.modules[mi as usize].module.ir.lang)
        })
        .unwrap_or_default()
}

/// The value of a numeric place as a decimal.
fn read_decimal(addr: i64, t: &Type) -> Option<Decimal> {
    match t {
        Type::Packed(s) => Packed::new(s.digits, s.scale)
            .ok()?
            .decode(bytes(addr, usize::from(s.digits) / 2 + 1))
            .ok(),
        Type::Zoned(s) => Zoned::new(s.digits, s.scale)
            .ok()?
            .decode(bytes(addr, usize::from(s.digits)))
            .ok(),
        _ => None,
    }
}

fn write_decimal(addr: i64, t: &Type, d: &Decimal) {
    match t {
        Type::Packed(s) => {
            if let Ok(p) = Packed::new(s.digits, s.scale) {
                let _ = d.rescale(s.scale, Rounding::Truncate).and_then(|d| {
                    p.encode_into(
                        &d,
                        Rounding::Truncate,
                        bytes(addr, usize::from(s.digits) / 2 + 1),
                    )
                });
            }
        }
        Type::Zoned(s) => {
            if let Ok(z) = Zoned::new(s.digits, s.scale) {
                let _ = d.rescale(s.scale, Rounding::Truncate).and_then(|d| {
                    z.encode_into(&d, Rounding::Truncate, bytes(addr, usize::from(s.digits)))
                });
            }
        }
        _ => {}
    }
}

/// `dtaara(env, op, name, lib, data, data_desc)`: 1 lock, 2 read, 3
/// write, 4 unlock; a character area copies, a decimal one converts
/// to and from the place's numeric type.
pub extern "sysv64" fn dtaara(
    env: *mut Env,
    op: i64,
    name: i64,
    lib: i64,
    data: i64,
    data_desc: i64,
) {
    let j = job(env);
    let n = ccsid::decode_trimmed(bytes(name, 10));
    let l = ccsid::decode_trimmed(bytes(lib, 10));
    let Some(src) = j.data.as_mut() else {
        escape(j, &IoFail::new(1299, "CPF1015", &[&n, &l]));
        return;
    };
    let r: Result<(), IoFail> = match op {
        1 => src.dtaara_lock(&n, &l),
        4 => src.dtaara_unlock(&n, &l),
        2 => src.dtaara_read(&n, &l).map(|v| {
            if data == 0 {
                return;
            }
            let t = ty(data_desc);
            match (v.kind, &t) {
                (DataAreaKind::Dec, Type::Packed(_) | Type::Zoned(_)) => {
                    let digits = u8::try_from(v.len).unwrap_or(15);
                    let scale = u8::try_from(v.decimals).unwrap_or(0);
                    if let Some(d) = Packed::new(digits, scale)
                        .ok()
                        .and_then(|p| p.decode(&v.value).ok())
                    {
                        write_decimal(data, &t, &d);
                    }
                }
                _ => {
                    let len = usize::try_from(t.size()).unwrap_or(0);
                    let buf = bytes(data, len);
                    let m = len.min(v.value.len());
                    buf[..m].copy_from_slice(&v.value[..m]);
                    if m < len {
                        buf[m..].fill(0x40);
                    }
                }
            }
        }),
        3 => {
            let t = ty(data_desc);
            let current = src.dtaara_read(&n, &l);
            match current {
                Ok(mut v) => {
                    match (v.kind, &t) {
                        (DataAreaKind::Dec, Type::Packed(_) | Type::Zoned(_)) => {
                            let digits = u8::try_from(v.len).unwrap_or(15);
                            let scale = u8::try_from(v.decimals).unwrap_or(0);
                            if let (Some(d), Ok(p)) =
                                (read_decimal(data, &t), Packed::new(digits, scale))
                            {
                                v.value = d
                                    .rescale(scale, Rounding::Truncate)
                                    .and_then(|d| p.encode(&d, Rounding::Truncate))
                                    .unwrap_or(v.value);
                            }
                        }
                        _ => {
                            let len = usize::try_from(t.size()).unwrap_or(0);
                            v.value = bytes(data, len).to_vec();
                        }
                    }
                    src.dtaara_write(&n, &l, &DataAreaValue { ..v })
                }
                Err(e) => Err(e),
            }
        }
        _ => Ok(()),
    };
    if let Err(f) = r {
        escape(j, &f);
    }
}

/// `dtaq_snd(env, name, lib, data, len)`.
pub extern "sysv64" fn dtaq_snd(env: *mut Env, name: i64, lib: i64, data: i64, len: i64) {
    let j = job(env);
    let n = ccsid::decode_trimmed(bytes(name, 10));
    let l = ccsid::decode_trimmed(bytes(lib, 10));
    let entry = bytes(data, usize::try_from(len).unwrap_or(0)).to_vec();
    let r = match j.data.as_mut() {
        Some(src) => src.dtaq_send(&n, &l, &entry),
        None => Err(IoFail::new(1299, "CPF9801", &[&n, &l])),
    };
    if let Err(f) = r {
        escape(j, &f);
    }
}

/// `dtaq_rcv(env, name, lib, data, cap, wait) -> len`.
pub extern "sysv64" fn dtaq_rcv(
    env: *mut Env,
    name: i64,
    lib: i64,
    data: i64,
    cap: i64,
    wait: i64,
) -> i32 {
    let j = job(env);
    let n = ccsid::decode_trimmed(bytes(name, 10));
    let l = ccsid::decode_trimmed(bytes(lib, 10));
    let r = match j.data.as_mut() {
        Some(src) => src.dtaq_receive(&n, &l, wait),
        None => Err(IoFail::new(1299, "CPF9801", &[&n, &l])),
    };
    match r {
        Ok(Some(e)) => {
            let room = usize::try_from(cap).unwrap_or(0);
            let m = room.min(e.len());
            bytes(data, room)[..m].copy_from_slice(&e[..m]);
            i32::try_from(m).unwrap_or(i32::MAX)
        }
        Ok(None) => 0,
        Err(f) => {
            escape(j, &f);
            0
        }
    }
}

/// `rslvsp(env, out, type, name, lib)`: a system pointer to the
/// object, `MCH3401` when there is none.
pub extern "sysv64" fn rslvsp(env: *mut Env, out: i64, obj_type: i64, name: i64, lib: i64) {
    let j = job(env);
    let t = ccsid::decode_trimmed(bytes(obj_type, 10));
    let n = ccsid::decode_trimmed(bytes(name, 10));
    let l = ccsid::decode_trimmed(bytes(lib, 10));
    match j.act.source.resolve(&n, &l, &t) {
        Some(id) => {
            let p = FatPtr {
                kind: PtrKind::System as u8,
                flags: 0,
                generation: 0,
                space: 0,
                offset: id,
            };
            bytes(out, 16).copy_from_slice(&p.to_bytes());
        }
        None => {
            bytes(out, 16).copy_from_slice(&FatPtr::null().to_bytes());
            raise_on(j, Cond::Mch3401);
        }
    }
}
