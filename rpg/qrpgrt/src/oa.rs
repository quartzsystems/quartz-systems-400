// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Open Access: a `HANDLER` file's open and operations go to its handler
//! with IBM's `QrnOpenAccess_T` (`QOAR/QRPGLESRC,QRNOPENACC`, 528 bytes,
//! format `ROAC0100`): the record in the input or output buffer, or each
//! field as a name and a value in text (`QrnNamesValues_T`) when the
//! handler asked for that at the open.

#![allow(clippy::match_same_arms, clippy::similar_names, clippy::too_many_lines, clippy::struct_excessive_bools, clippy::unnecessary_wraps, clippy::unused_self)]

use std::cell::RefCell;
use std::rc::Rc;

use codegen::abi::Env;
use mi::io::oa::{DevRequest, DevResult, OpenAccess};
use mi::io::{FileInfo, IoFail, IoOp, IoRequest, IoResult, KeyRef};

use crate::datainto::{Callee, Region, call_callee, callee};

/// The parameter's length.
const PARM_LEN: usize = 528;

/// `QrnNamesValues_T`'s and `QrnRecordLevels_T`'s sizes.
const NAMES_VALUES_SIZE: usize = 16 + 80 * 32_767;
const RECORD_LEVELS_SIZE: usize = 4 + 23 * 32_767;

/// Offsets in `QrnOpenAccess_T`.
mod at {
    pub const LENGTH: usize = 0;
    pub const FORMAT: usize = 8;
    pub const USER_AREA: usize = 16;
    pub const RECORD_LEVELS: usize = 48;
    pub const INPUT: usize = 64;
    pub const OUTPUT: usize = 96;
    pub const NAMES_VALUES: usize = 128;
    pub const KEY: usize = 144;
    pub const EXT_FILE: usize = 320;
    pub const EXT_MEMBER: usize = 340;
    pub const COMPILE_FILE: usize = 350;
    pub const RECORD_NAME: usize = 370;
    pub const OPERATION: usize = 400;
    pub const STATUS: usize = 404;
    pub const INPUT_LEN: usize = 408;
    pub const OUTPUT_LEN: usize = 416;
    pub const KEY_LEN: usize = 424;
    pub const NUM_KEYS: usize = 448;
    pub const RRN: usize = 452;
    pub const FORM_LEN: usize = 456;
    pub const FORM_OFL: usize = 460;
    pub const FUNCTION_KEY: usize = 480;
    pub const EXTERNAL: usize = 481;
    pub const KEYED: usize = 482;
    pub const BLOCKED: usize = 483;
    pub const EOF: usize = 484;
    pub const FOUND: usize = 485;
    pub const EQUAL: usize = 486;
    pub const OVERFLOW: usize = 487;
    pub const WITH_LOCK: usize = 488;
    pub const NAMES: usize = 489;
    pub const COMMIT: usize = 495;
    pub const DEVICE: usize = 496;
}

/// The subfields of `QrnOpenAccess_T` the runtime sets or reads: name,
/// offset from 0 and length (`docs/RPG.md`, `tests/rpg_layouts.rs`).
pub const FIELDS: &[(&str, (usize, usize))] = &[
    ("length", (at::LENGTH, 4)),
    ("parameterFormat", (at::FORMAT, 8)),
    ("userArea", (at::USER_AREA, 16)),
    ("recordLevels", (at::RECORD_LEVELS, 16)),
    ("inputBuffer", (at::INPUT, 16)),
    ("outputBuffer", (at::OUTPUT, 16)),
    ("namesValues", (at::NAMES_VALUES, 16)),
    ("key", (at::KEY, 16)),
    ("externalFile", (at::EXT_FILE, 20)),
    ("externalMember", (at::EXT_MEMBER, 10)),
    ("compileFile", (at::COMPILE_FILE, 20)),
    ("recordName", (at::RECORD_NAME, 10)),
    ("rpgOperation", (at::OPERATION, 4)),
    ("rpgStatus", (at::STATUS, 4)),
    ("inputBufferLen", (at::INPUT_LEN, 4)),
    ("outputBufferLen", (at::OUTPUT_LEN, 4)),
    ("keyLen", (at::KEY_LEN, 4)),
    ("numKeys", (at::NUM_KEYS, 4)),
    ("rrn", (at::RRN, 4)),
    ("formLen", (at::FORM_LEN, 4)),
    ("formOfl", (at::FORM_OFL, 4)),
    ("functionKey", (at::FUNCTION_KEY, 1)),
    ("externallyDescribed", (at::EXTERNAL, 1)),
    ("keyedFile", (at::KEYED, 1)),
    ("blocked", (at::BLOCKED, 1)),
    ("eof", (at::EOF, 1)),
    ("found", (at::FOUND, 1)),
    ("equal", (at::EQUAL, 1)),
    ("printerOverflow", (at::OVERFLOW, 1)),
    ("inputWithLock", (at::WITH_LOCK, 1)),
    ("useNamesValues", (at::NAMES, 1)),
    ("commit", (at::COMMIT, 1)),
    ("rpgDevice", (at::DEVICE, 1)),
];

/// `QrnOperation_*`.
mod op {
    pub const OPEN: u32 = 1;
    pub const POSITION_START: u32 = 2;
    pub const POSITION_END: u32 = 3;
    pub const READ: u32 = 4;
    pub const READC: u32 = 5;
    pub const READE: u32 = 6;
    pub const READP: u32 = 7;
    pub const READPE: u32 = 8;
    pub const CHAIN: u32 = 9;
    pub const EXFMT: u32 = 10;
    pub const SETGT: u32 = 11;
    pub const SETLL: u32 = 12;
    pub const UNLOCK: u32 = 13;
    pub const UPDATE: u32 = 14;
    pub const WRITE: u32 = 15;
    pub const DELETE: u32 = 16;
    pub const FEOD: u32 = 17;
    pub const CLOSE: u32 = 18;
}

/// A field of a record format, as the compiler described it.
#[derive(Clone, Debug, Default)]
struct Field {
    name: String,
    dtype: u8,
    len: u8,
    dec: u8,
    dtz: u8,
    sep: char,
    off: usize,
    bytes: usize,
    desc: i64,
}

/// A record format.
#[derive(Clone, Debug, Default)]
struct Format {
    name: String,
    level: String,
    len: usize,
    keys: Vec<String>,
    fields: Vec<Field>,
}

/// The file, as the compiler described it (`rpgc::codegen::oa`).
#[derive(Clone, Debug, Default)]
struct Desc {
    device: u8,
    external: bool,
    update: bool,
    keyed: bool,
    commit: bool,
    file: String,
    lib: String,
    member: String,
    compile_file: String,
    compile_lib: String,
    reclen: usize,
    form: (u32, u32),
    formats: Vec<Format>,
}

fn parse_desc(s: &str) -> Desc {
    let mut d = Desc::default();
    for rec in s.split(';') {
        let f: Vec<&str> = rec.split('|').collect();
        let num = |i: usize| f.get(i).and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
        match f.first().copied() {
            Some("F") => {
                d.device = f.get(1).and_then(|x| x.bytes().next()).unwrap_or(b'D');
                d.external = f.get(2) == Some(&"1");
                d.keyed = f.get(3) == Some(&"1");
                d.commit = f.get(5) == Some(&"1");
                d.file = f.get(6).unwrap_or(&"").to_string();
                d.lib = f.get(7).unwrap_or(&"").to_string();
                d.member = f.get(8).unwrap_or(&"").to_string();
                d.compile_file = f.get(9).unwrap_or(&"").to_string();
                d.compile_lib = f.get(10).unwrap_or(&"").to_string();
                d.reclen = num(11);
                d.form = (u32::try_from(num(12)).unwrap_or(0), u32::try_from(num(13)).unwrap_or(0));
                d.update = f.get(14) == Some(&"1");
            }
            Some("R") => d.formats.push(Format {
                name: f.get(1).unwrap_or(&"").to_string(),
                level: f.get(2).unwrap_or(&"").to_string(),
                len: num(3),
                keys: f.get(4).map(|k| k.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect()).unwrap_or_default(),
                fields: Vec::new(),
            }),
            Some("V") => {
                let byte = |i: usize| u8::try_from(num(i)).unwrap_or(0);
                let fld = Field {
                    name: f.get(1).unwrap_or(&"").to_string(),
                    dtype: byte(2),
                    len: byte(3),
                    dec: byte(4),
                    dtz: byte(5),
                    sep: f.get(6).and_then(|x| x.chars().next()).unwrap_or(' '),
                    off: num(7),
                    bytes: num(8),
                    desc: f.get(9).and_then(|x| x.parse().ok()).unwrap_or(0),
                };
                if let Some(fm) = d.formats.last_mut() {
                    fm.fields.push(fld);
                }
            }
            _ => {}
        }
    }
    d
}

/// One handler file.
pub struct Handler {
    callee: Callee,
    user_area: mi::FatPtr,
    desc: Desc,
    /// The parameter, kept from operation to operation.
    parm: Option<Region>,
    names_values: bool,
    /// The record format of the last record read.
    last_format: String,
}

fn put(b: &mut [u8], at: usize, v: &[u8]) {
    b[at..at + v.len()].copy_from_slice(v);
}

fn put_name(b: &mut [u8], at: usize, n: usize, s: &str) {
    let mut e = crate::value::ebcdic(s);
    e.resize(n, 0x40);
    put(b, at, &e);
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn ind(v: bool) -> u8 {
    if v { 0xF1 } else { 0xF0 }
}

/// A field's value in text, as a handler gets it: numbers without
/// leading zeros and with their decimals, characters at full length.
fn value_text(f: &Field, rec: &[u8]) -> String {
    let b = rec.get(f.off..f.off + f.bytes).unwrap_or(&[]);
    let d = crate::value::Desc::of(f.desc);
    if d.numeric() || matches!(d.kind, crate::value::Kind::Float) {
        let a = b.as_ptr() as u64;
        if d.kind == crate::value::Kind::Float {
            return crate::value::read_f64(a, &d).map(|v| v.to_string()).unwrap_or_default();
        }
        return match crate::value::read_num(a, &d) {
            Ok(v) => {
                let s = v.char_text(d.dec);
                // A whole part of zero is written.
                if s.starts_with('.') {
                    format!("0{s}")
                } else if let Some(r) = s.strip_prefix("-.") {
                    format!("-0.{r}")
                } else {
                    s
                }
            }
            Err(_) => String::new(),
        };
    }
    if d.kind == crate::value::Kind::Ind {
        return if b.first() == Some(&0xF1) { "1".into() } else { "0".into() };
    }
    ccsid::from_ebcdic(b)
}

/// The most bytes a field's text takes.
fn max_len(f: &Field) -> usize {
    match f.dtype {
        8 => usize::from(f.len) + 1 + usize::from(f.dec > 0),
        9 | 10 => 20,
        11 => 30,
        7 => 1,
        _ => f.bytes,
    }
}

impl Handler {
    fn format(&self, name: Option<&str>) -> Option<&Format> {
        match name.filter(|n| !n.is_empty()) {
            Some(n) => self.desc.formats.iter().find(|f| f.name.eq_ignore_ascii_case(n)),
            None => self.desc.formats.first(),
        }
    }

    /// Call the handler for one operation: the parameter after it.
    #[allow(clippy::too_many_arguments)]
    fn call(&mut self, j: &mut mi::Job, operation: u32, format: &str, input_len: usize, output: Option<&[u8]>, key: Option<&[u8]>, num_keys: usize, rrn: u64, with_lock: bool) -> (Vec<u8>, Vec<u8>, bool) {
        let mut regions = Vec::new();
        let mut region = |j: &mut mi::Job, data: Vec<u8>| {
            let (r, p) = Region::new(j, data);
            regions.push(r);
            p
        };
        let fmt = self.format(Some(format)).cloned();
        let nv = self.names_values && operation != op::OPEN && operation != op::CLOSE && fmt.is_some();
        // The record buffers, or the names and values.
        let mut input_p = mi::FatPtr::null();
        let mut output_p = mi::FatPtr::null();
        let mut nv_p = mi::FatPtr::null();
        let mut nv_values: Vec<(usize, usize)> = Vec::new();
        let input_op = matches!(operation, op::READ | op::READC | op::READE | op::READP | op::READPE | op::CHAIN | op::EXFMT);
        let output_op = matches!(operation, op::WRITE | op::UPDATE | op::EXFMT);
        if nv {
            let fm = fmt.clone().unwrap_or_default();
            let n = fm.fields.len();
            let mut values = Vec::new();
            for fd in &fm.fields {
                let text = if output_op { output.map(|r| value_text(fd, r)).unwrap_or_default() } else { String::new() };
                let mut e = crate::value::ebcdic(&text);
                let max = max_len(fd).max(e.len());
                let start = values.len();
                nv_values.push((start, e.len()));
                e.resize(max, 0x40);
                values.extend(e);
            }
            // Room for a handler's longer values.
            let room = values.len() + crate::datainto::DOC_ROOM;
            values.resize(room, 0);
            let vp = region(j, values);
            // The whole of `QrnNamesValues_T`: a handler bases it on the pointer.
            let mut area = vec![0u8; (16 + 80 * n).max(NAMES_VALUES_SIZE)];
            put(&mut area, 0, &i32::try_from(n).unwrap_or(0).to_be_bytes());
            for (i, fd) in fm.fields.iter().enumerate() {
                let b = 16 + 80 * i;
                put_name(&mut area, b, 10, &fd.name);
                area[b + 10] = fd.dtype;
                area[b + 11] = fd.len;
                area[b + 12] = fd.dec;
                area[b + 13] = fd.dtz;
                area[b + 14] = if fd.dtz == 0 { 0x40 } else { crate::value::ebcdic(&fd.sep.to_string())[0] };
                area[b + 15] = ind(input_op);
                area[b + 16] = ind(output_op && !input_op);
                area[b + 17] = 0xF0;
                area[b + 18] = 0xF0;
                let (start, len) = nv_values[i];
                put(&mut area, b + 32, &u32::try_from(len).unwrap_or(0).to_be_bytes());
                put(&mut area, b + 36, &u32::try_from(max_len(fd)).unwrap_or(0).to_be_bytes());
                let p = j.spaces.add(vp, i64::try_from(start).unwrap_or(0)).unwrap_or(vp);
                put(&mut area, b + 48, &p.to_bytes());
            }
            nv_p = region(j, area);
        } else {
            if input_op {
                input_p = region(j, vec![0x40; input_len.max(1)]);
            }
            if let Some(o) = output.filter(|_| output_op) {
                output_p = region(j, o.to_vec());
            }
        }
        let key_p = key.filter(|k| !k.is_empty()).map_or(mi::FatPtr::null(), |k| region(j, k.to_vec()));
        let ua = self.user_area;
        let d = self.desc.clone();
        let names_values = self.names_values;
        let Some(parm) = self.parm.as_mut() else { return (Vec::new(), Vec::new(), false) };
        let b = parm.buf.as_mut_slice();
        put(b, at::LENGTH, &u32::try_from(PARM_LEN).unwrap_or(0).to_be_bytes());
        put_name(b, at::FORMAT, 8, "ROAC0100");
        put(b, at::USER_AREA, &ua.to_bytes());
        put(b, at::INPUT, &input_p.to_bytes());
        put(b, at::OUTPUT, &output_p.to_bytes());
        put(b, at::NAMES_VALUES, &nv_p.to_bytes());
        put(b, at::KEY, &key_p.to_bytes());
        // The record format; blank at the open and the close, and for a
        // program-described file.
        let rname = if d.external && operation != op::OPEN && operation != op::CLOSE { fmt.as_ref().map_or("", |f| f.name.as_str()) } else { "" };
        put_name(b, at::RECORD_NAME, 10, rname);
        put(b, at::OPERATION, &operation.to_be_bytes());
        put(b, at::STATUS, &0i32.to_be_bytes());
        let lens = |n: usize| u32::try_from(n).unwrap_or(0).to_be_bytes();
        let ilen = if input_op && !nv { input_len } else { 0 };
        let olen = if output_op && !nv { output.map_or(0, <[u8]>::len) } else { 0 };
        put(b, at::INPUT_LEN, &lens(ilen));
        put(b, at::OUTPUT_LEN, &lens(olen));
        put(b, at::KEY_LEN, &lens(key.map_or(0, <[u8]>::len)));
        put(b, at::NUM_KEYS, &lens(num_keys));
        put(b, at::RRN, &u32::try_from(rrn).unwrap_or(0).to_be_bytes());
        b[at::EOF] = 0xF0;
        b[at::FOUND] = 0xF0;
        b[at::EQUAL] = 0xF0;
        b[at::OVERFLOW] = 0xF0;
        b[at::WITH_LOCK] = ind(with_lock);
        b[at::NAMES] = ind(names_values);
        b[at::FUNCTION_KEY] = 0;
        let addr = b.as_ptr() as u64;
        let called = call_callee(j, &self.callee, addr);
        let out = self.parm.as_ref().map(|p| p.buf.clone()).unwrap_or_default();
        // The record the handler gave.
        let mut record = Vec::new();
        if input_op {
            if nv {
                let fm = fmt.unwrap_or_default();
                record = vec![0x40; fm.len.max(input_len)];
                let area_addr = j.spaces.deref(nv_p).unwrap_or(0);
                for (i, fd) in fm.fields.iter().enumerate() {
                    let base = area_addr + 16 + 80 * i as u64;
                    let n = u32_at(crate::value::bytes(base, 80), 32) as usize;
                    let p: [u8; 16] = crate::value::bytes(base + 48, 16).try_into().unwrap_or([0; 16]);
                    let text = match j.spaces.deref(mi::FatPtr::from_bytes(&p)) {
                        Ok(a) if a != 0 => ccsid::from_ebcdic(crate::value::bytes(a, n)),
                        _ => String::new(),
                    };
                    let dst = record.as_mut_ptr() as u64 + fd.off as u64;
                    let d = crate::value::Desc::of(fd.desc);
                    let text = if d.numeric() { text.trim().to_string() } else { text };
                    let _ = crate::xml::store_scalar(dst, &d, &text);
                }
            } else if let Ok(a) = j.spaces.deref(input_p)
                && a != 0
            {
                record = crate::value::bytes(a, input_len.max(1)).to_vec();
            }
        }
        for r in regions {
            r.drop_in(j);
        }
        (out, record, called)
    }

    /// Status and flags from the parameter after a call.
    fn result(out: &[u8], operation: u32) -> IoResult {
        let status = i32::from_be_bytes([out[at::STATUS], out[at::STATUS + 1], out[at::STATUS + 2], out[at::STATUS + 3]]);
        let eof = out[at::EOF] == 0xF1;
        let found = out[at::FOUND] == 0xF1;
        let equal = out[at::EQUAL] == 0xF1;
        let mut r = IoResult {
            status: u16::try_from(status.max(0)).unwrap_or(1299),
            eof,
            found,
            equal,
            rrn: u64::from(u32_at(out, at::RRN)),
            ..IoResult::default()
        };
        if r.status == 0 {
            if eof && matches!(operation, op::READ | op::READE | op::READP | op::READPE | op::READC) {
                r.status = 11;
            } else if operation == op::CHAIN && !found {
                r.status = 12;
            }
        }
        if r.status >= 1000 {
            r.msgid = Some("CPF5149".into());
        }
        r
    }
}

impl OpenAccess for Handler {
    fn open(&mut self, j: &mut mi::Job, file: &qir::FileRef) -> Result<FileInfo, IoFail> {
        let mut b = vec![0u8; PARM_LEN];
        put_name(&mut b, at::EXT_FILE, 10, &self.desc.file);
        put_name(&mut b, at::EXT_FILE + 10, 10, &self.desc.lib);
        put_name(&mut b, at::EXT_MEMBER, 10, &self.desc.member);
        put_name(&mut b, at::COMPILE_FILE, 10, &self.desc.compile_file);
        put_name(&mut b, at::COMPILE_FILE + 10, 10, &self.desc.compile_lib);
        // A printer without FORMLEN and FORMOFL: the printer file's 66 and 60.
        let printer = self.desc.device == b'P';
        let form = match self.desc.form {
            (0, 0) if printer => (66, 60),
            f => f,
        };
        put(&mut b, at::FORM_LEN, &form.0.to_be_bytes());
        put(&mut b, at::FORM_OFL, &form.1.to_be_bytes());
        b[at::EXTERNAL] = ind(self.desc.external);
        b[at::KEYED] = ind(self.desc.keyed);
        b[at::BLOCKED] = 0xF0;
        b[at::COMMIT] = ind(self.desc.commit);
        b[at::DEVICE] = crate::value::ebcdic(&char::from(self.desc.device).to_string())[0];
        for i in [490, 491, 492, 493, 494] {
            b[i] = 0xF0;
        }
        // The formats' level identifiers.
        let (levels, lp) = if self.desc.external {
            let mut l = vec![0u8; (4 + 23 * self.desc.formats.len()).max(RECORD_LEVELS_SIZE)];
            put(&mut l, 0, &i32::try_from(self.desc.formats.len()).unwrap_or(0).to_be_bytes());
            for (i, f) in self.desc.formats.iter().enumerate() {
                put_name(&mut l, 4 + 23 * i, 10, &f.name);
                put_name(&mut l, 14 + 23 * i, 13, &f.level);
            }
            let (r, p) = Region::new(j, l);
            (Some(r), p)
        } else {
            (None, mi::FatPtr::null())
        };
        put(&mut b, at::RECORD_LEVELS, &lp.to_bytes());
        let (parm, _) = Region::new(j, b);
        self.parm = Some(parm);
        if let Some(r) = levels {
            // The levels stay for the file's life: the region is leaked
            // into the parameter's keeping.
            std::mem::forget(r);
        }
        let (out, _, called) = self.call(j, op::OPEN, "", 0, None, None, 0, 0, false);
        if !called || j.pending() {
            j.clear();
            // As a handler that cannot be called ends the program.
            j.cancel_request("CEE9901", "QCEEMSG", Vec::new());
            return Err(IoFail::new(1216, "CPF4101", &[&file.file, &file.lib]));
        }
        self.names_values = out[at::NAMES] == 0xF1;
        let r = Self::result(&out, op::OPEN);
        if r.status != 0 {
            return Err(IoFail::new(r.status, "CPF4101", &[&file.file, &file.lib]));
        }
        Ok(FileInfo {
            file: file.file.clone(),
            lib: file.lib.clone(),
            member: self.desc.member.clone(),
            format: self.desc.formats.first().map(|f| f.name.clone()).unwrap_or_default(),
            rec_len: u32::try_from(self.desc.reclen).unwrap_or(0),
            wait_ms: 0,
        })
    }

    fn io(&mut self, j: &mut mi::Job, req: &IoRequest<'_>) -> IoResult {
        let (operation, key, rrn) = match (req.op, req.key) {
            (IoOp::SetLL, KeyRef::Start | KeyRef::LoVal) => (op::POSITION_START, None, 0),
            (IoOp::SetGT, KeyRef::End | KeyRef::HiVal) => (op::POSITION_END, None, 0),
            (IoOp::SetLL, KeyRef::End | KeyRef::HiVal) => (op::POSITION_END, None, 0),
            (o, k) => {
                let operation = match o {
                    IoOp::Read => op::READ,
                    IoOp::ReadP => op::READP,
                    IoOp::ReadE => op::READE,
                    IoOp::ReadPE => op::READPE,
                    IoOp::Chain => op::CHAIN,
                    IoOp::SetLL => op::SETLL,
                    IoOp::SetGT => op::SETGT,
                    IoOp::Write => op::WRITE,
                    IoOp::Update => op::UPDATE,
                    IoOp::Delete => op::DELETE,
                    IoOp::Unlock => op::UNLOCK,
                    IoOp::Feod => op::FEOD,
                };
                match k {
                    KeyRef::Fields(b) => (operation, Some(b), 0),
                    KeyRef::Rrn(n) => (operation, None, n),
                    _ => (operation, None, 0),
                }
            }
        };
        let format = req.format.map_or_else(|| self.last_format.clone(), str::to_string);
        let fm = self.format(Some(&format)).cloned();
        let reclen = fm.as_ref().map_or(self.desc.reclen, |f| f.len).max(self.desc.reclen);
        // How many key fields the key's bytes hold.
        let num_keys = match (key, &fm) {
            (Some(k), Some(f)) => {
                let mut n = 0;
                let mut total = 0;
                for name in &f.keys {
                    if total >= k.len() {
                        break;
                    }
                    total += f.fields.iter().find(|x| &x.name == name).map_or(k.len(), |x| x.bytes);
                    n += 1;
                }
                n
            }
            _ => 0,
        };
        // An input operation of an update file locks the record.
        let with_lock = self.desc.update && !req.nolock && matches!(operation, op::READ | op::READE | op::READP | op::READPE | op::CHAIN);
        let (out, record, _) = self.call(j, operation, &format, reclen, req.record, key, num_keys, rrn, with_lock);
        let mut r = Self::result(&out, operation);
        if matches!(operation, op::READ | op::READE | op::READP | op::READPE | op::CHAIN) && r.status == 0 {
            r.record = Some(record);
            r.format = fm.map(|f| f.name).unwrap_or_default();
            r.found = true;
        }
        r
    }

    fn dev(&mut self, j: &mut mi::Job, req: &DevRequest) -> DevResult {
        let operation = match req.op {
            0 | 1 | 11 => op::WRITE,
            2 | 12 => op::READ,
            14 => op::CHAIN,
            15 => op::UPDATE,
            16 => op::READC,
            _ => op::EXFMT,
        };
        let fm = self.format(Some(&req.format)).cloned();
        let reclen = fm.as_ref().map_or(self.desc.reclen, |f| f.len).max(req.record.len());
        let (out, record, _) = self.call(j, operation, &req.format, reclen, Some(&req.record), None, 0, 0, false);
        let r = Self::result(&out, operation);
        DevResult {
            status: if r.status == 11 || r.status == 12 { 0 } else { r.status },
            overflow: out[at::OVERFLOW] == 0xF1,
            input: matches!(operation, op::READ | op::EXFMT | op::CHAIN | op::READC).then_some(record),
        }
    }

    fn close(&mut self, j: &mut mi::Job) -> IoResult {
        let (out, _, _) = self.call(j, op::CLOSE, "", 0, None, None, 0, 0, false);
        let r = Self::result(&out, op::CLOSE);
        if let Some(p) = self.parm.take() {
            p.drop_in(j);
        }
        r
    }
}

/// `_QRNX_OAOPEN(name, nd, ptr, userArea, has, desc)`: the handler the
/// next open goes to.
pub extern "sysv64" fn oa_open(env: *mut Env, _argc: i64, _od: i64, name: i64, nd: i64, ptr: i64, ua: i64, has: i32, desc: i64) {
    let j = mi::storage::job_of(env);
    let c = match crate::xml::proc_at(ptr) {
        Some(a) => Callee::Ptr(a),
        None => callee(&crate::xml::text_of(name, nd)),
    };
    // `has`: the user area's size, 0 for none.
    let user_area = if has > 0 {
        let depth = j.depth;
        j.spaces.addr(ua.cast_unsigned(), u64::from(has.unsigned_abs()), depth, false)
    } else {
        mi::FatPtr::null()
    };
    let h = Handler {
        callee: c,
        user_area,
        desc: parse_desc(&crate::xml::varchar_at(desc)),
        parm: None,
        names_values: false,
        last_format: String::new(),
    };
    j.oa_next = Some(Rc::new(RefCell::new(h)));
}
