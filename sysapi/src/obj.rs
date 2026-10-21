// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object APIs: `QUSLOBJ`, `QUSROBJD`, `QLICOBJD`, `QLIRNMO`, and the
//! generic list header (`QUSGEN`) every list API writes into a user
//! space.

use mi::Job;

use crate::param::{Args, ErrCode, Out};
use crate::provider::{ApiFail, ObjInfo, Provider};

/// The APIs of this module.
pub const APIS: &[(&str, crate::ApiFn)] = &[
    ("QUSLOBJ", quslobj),
    ("QUSROBJD", qusrobjd),
    ("QLICOBJD", qlicobjd),
    ("QLIRNMO", qlirnmo),
];

/// Size of the generic header.
pub const GEN_HDR: usize = 192;

/// Write a list into the user space named at parameter `us`: the
/// generic header, the input parameter section (`input`), the header
/// section (`header`) and the entries. `None` when the space does not
/// exist (`CPF9801`).
pub fn write_list(
    j: &mut Job,
    a: &Args<'_>,
    us: usize,
    format: &str,
    api: &str,
    input: &[u8],
    header: &[u8],
    entries: &[Vec<u8>],
    entry_size: usize,
) -> Result<(), ApiFail> {
    let (name, lib) = a.qual(us);
    let lib = j
        .usrspcs
        .library_of(&name, &lib)
        .ok_or_else(|| ApiFail::not_found(&name, &lib))?;
    j.map_user_space(&name, &lib)
        .map_err(|_| ApiFail::not_found(&name, &lib))?;
    let mut out = Out::new();
    out.blank(0, 64);
    out.bin4(64, i32::try_from(GEN_HDR).expect("small"));
    out.char(68, 4, "0100");
    out.char(72, 8, format);
    out.char(80, 10, api);
    let now = crate::job::timestamp13(unix_now());
    out.char(90, 13, &now);
    out.ch(103, 'C');
    let input_off = GEN_HDR;
    let header_off = input_off + input.len();
    let data_off = header_off + header.len();
    let data_len = entries.len() * entry_size;
    out.bin4(104, i32::try_from(data_off + data_len).unwrap_or(i32::MAX));
    out.bin4(108, i32::try_from(input_off).unwrap_or(0));
    out.bin4(112, i32::try_from(input.len()).unwrap_or(0));
    out.bin4(116, i32::try_from(header_off).unwrap_or(0));
    out.bin4(120, i32::try_from(header.len()).unwrap_or(0));
    out.bin4(124, i32::try_from(data_off).unwrap_or(0));
    out.bin4(128, i32::try_from(data_len).unwrap_or(0));
    out.bin4(132, i32::try_from(entries.len()).unwrap_or(0));
    out.bin4(136, i32::try_from(entry_size).unwrap_or(0));
    out.bin4(140, 37);
    out.char(144, 2, "US");
    out.char(146, 3, "ENU");
    out.ch(149, '0');
    out.blank(150, 42);
    out.bytes(input_off, input);
    out.bytes(header_off, header);
    let mut at = data_off;
    for e in entries {
        let mut e = e.clone();
        e.resize(entry_size, 0x40);
        out.bytes(at, &e);
        at += entry_size;
    }
    let space = j
        .usrspcs
        .mapped_mut(&name, &lib)
        .ok_or_else(|| ApiFail::not_found(&name, &lib))?;
    if space.len() < out.buf.len() {
        space.resize(out.buf.len(), 0);
    }
    space[..out.buf.len()].copy_from_slice(&out.buf);
    j.sync_user_spaces();
    Ok(())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `QUSLOBJ(user space, format, object, type, errc)`: `OBJL0100` or
/// `OBJL0200`.
fn quslobj(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let format = a.char(1, 8);
    let (name, lib) = a.qual(2);
    let ty = a.char(3, 10);
    let entry_size = match format.as_str() {
        "OBJL0100" => 30,
        "OBJL0200" => 108,
        _ => {
            errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
            return;
        }
    };
    let lib = if lib.is_empty() { "*LIBL".into() } else { lib };
    let objs = p.list_objects(&lib, &name, &ty);
    let entries: Vec<Vec<u8>> = objs
        .iter()
        .map(|o| {
            let mut e = Out::new();
            e.char(0, 10, &o.name);
            e.char(10, 10, &o.lib);
            e.char(20, 10, &o.ty);
            if entry_size > 30 {
                e.ch(30, ' ');
                e.char(31, 10, &o.attr);
                e.char(41, 50, &o.text);
                e.char(91, 10, &o.user_attr);
                e.blank(101, 7);
            }
            e.buf
        })
        .collect();
    let mut input = Out::new();
    input.char(0, 20, "");
    input.bytes(0, &a.bytes(0, 20));
    input.char(20, 8, &format);
    input.bytes(28, &a.bytes(2, 20));
    input.char(48, 10, &ty);
    let mut header = Out::new();
    header.char(0, 10, &name);
    header.char(10, 10, &lib);
    header.char(20, 10, &ty);
    match write_list(
        j,
        &a,
        0,
        &format,
        "QUSLOBJ",
        &input.buf,
        &header.buf,
        &entries,
        entry_size,
    ) {
        Ok(()) => errc.ok(),
        Err(f) => errc.fail(j, &f),
    }
}

/// The `OBJD0100`..`OBJD0300` image of an object.
fn objd(o: &ObjInfo, format: &str) -> Out {
    let mut out = Out::new();
    out.char(8, 10, &o.name);
    out.char(18, 10, &o.lib);
    out.char(28, 10, &o.ty);
    out.char(38, 10, &o.lib);
    out.bin4(48, 1);
    out.char(52, 10, &o.owner);
    out.char(62, 2, "*S");
    out.char(64, 13, &o.created);
    out.char(77, 13, &o.changed);
    if format == "OBJD0100" {
        return out;
    }
    out.char(90, 10, &o.attr);
    out.char(100, 50, &o.text);
    let (sf, sl, sm) = o.source.clone().unwrap_or_default();
    out.char(150, 10, &sf);
    out.char(160, 10, &sl);
    out.char(170, 10, &sm);
    if format == "OBJD0200" {
        return out;
    }
    out.blank(180, 13);
    out.blank(193, 13);
    out.blank(206, 13);
    out.char(219, 10, &o.owner);
    out.char(229, 8, "QS400");
    out.blank(237, 7);
    out.bin4(244, 0);
    out.bin4(248, 0);
    out.char(252, 10, "*SYSBAS");
    out.blank(262, 10);
    out.blank(272, 71);
    out.blank(343, 10);
    out.blank(353, 10);
    out.blank(363, 10);
    out.blank(373, 17);
    out.char(390, 9, &release::RELEASE.to_string());
    out.blank(399, 16);
    out.blank(415, 8);
    out.ch(423, '0');
    out.blank(424, 16);
    out.blank(440, 10);
    out.blank(450, 10);
    out.char(460, 10, &o.user_attr);
    out
}

/// `QUSROBJD(receiver, len, format, object, type, errc)`.
fn qusrobjd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let format = a.char(2, 8);
    if !matches!(
        format.as_str(),
        "OBJD0100" | "OBJD0200" | "OBJD0300" | "OBJD0400"
    ) {
        errc.fail(j, &ApiFail::id("CPF3C21").text(&format, 8));
        return;
    }
    let (name, lib) = a.qual(3);
    let lib = if lib.is_empty() { "*LIBL".into() } else { lib };
    let ty = a.char(4, 10);
    let Some(o) = p.object(&lib, &name, &ty) else {
        errc.fail(j, &ApiFail::not_found(&name, &lib));
        return;
    };
    objd(&o, &format).deliver(&a, 0, 1);
    errc.ok();
}

/// The keyed records of a change-description parameter: `(key, data)`.
fn keyed_records(b: &[u8]) -> Vec<(i32, Vec<u8>)> {
    let n = usize::try_from(crate::param::bin4_at(b, 0)).unwrap_or(0);
    let mut at = 4;
    let mut v = Vec::new();
    for _ in 0..n {
        let key = crate::param::bin4_at(b, at);
        let len = usize::try_from(crate::param::bin4_at(b, at + 4)).unwrap_or(0);
        let data = b.get(at + 8..at + 8 + len).unwrap_or(&[]).to_vec();
        v.push((key, data));
        at += 8 + len;
    }
    v
}

/// `QLICOBJD(returned lib, object, type, changes, errc)`: keys 1 text
/// (`char(50)`) and 2 user-defined attribute (`char(10)`).
fn qlicobjd(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 4);
    let (name, lib) = a.qual(1);
    let lib = if lib.is_empty() { "*LIBL".into() } else { lib };
    let ty = a.char(2, 10);
    let head = a.bytes(3, 4);
    let n = usize::try_from(crate::param::bin4_at(&head, 0)).unwrap_or(0);
    let recs = keyed_records(&a.bytes(3, 4 + n * (8 + 50)));
    let mut text = None;
    let mut attr = None;
    for (key, data) in recs {
        match key {
            1 => text = Some(ccsid::decode_trimmed(&data)),
            2 => attr = Some(ccsid::decode_trimmed(&data)),
            k => {
                errc.fail(j, &ApiFail::id("CPF3C4D").bin4(k));
                return;
            }
        }
    }
    match p.change_object(&lib, &name, &ty, text, attr) {
        Ok(found) => {
            a.put_char(0, 10, &found);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}

/// `QLIRNMO(object, type, new object, replace, returned lib, errc)`.
fn qlirnmo(j: &mut Job, args: &[u64], p: &mut dyn Provider) {
    let a = Args(args);
    let errc = ErrCode::at(&a, 5);
    let (name, lib) = a.qual(0);
    let lib = if lib.is_empty() { "*LIBL".into() } else { lib };
    let ty = a.char(1, 10);
    let (new_name, new_lib) = a.qual(2);
    let replace = a.char(3, 1) == "1";
    match p.rename_object(&lib, &name, &ty, &new_lib, &new_name, replace) {
        Ok(found) => {
            a.put_char(4, 10, &found);
            errc.ok();
        }
        Err(f) => errc.fail(j, &f),
    }
}
